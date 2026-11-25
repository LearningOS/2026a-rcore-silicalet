//! `Arc<Inode>` -> `OSInodeInner`: In order to open files concurrently
//! we need to wrap `Inode` into `Arc`,but `Mutex` in `Inode` prevents
//! file systems from being accessed simultaneously
//!
//! `UPSafeCell<OSInodeInner>` -> `OSInode`: for static `ROOT_INODE`,we
//! need to wrap `OSInodeInner` into `UPSafeCell`
use super::{File, Stat, StatMode};
use crate::drivers::BLOCK_DEVICE;
use crate::mm::UserBuffer;
use crate::sync::UPSafeCell;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::*;
use easy_fs::{EasyFileSystem, Inode};
use lazy_static::*;

/// inode in memory
/// A wrapper around a filesystem inode
/// to implement File trait atop
pub struct OSInode {
    readable: bool,
    writable: bool,
    inner: UPSafeCell<OSInodeInner>,
}
/// The OS inode inner in 'UPSafeCell'
pub struct OSInodeInner {
    offset: usize,
    inode: Arc<Inode>,
}

lazy_static! {
    static ref OPEN_INODES: UPSafeCell<BTreeMap<u32, usize>> =
        unsafe { UPSafeCell::new(BTreeMap::new()) };
}

impl Drop for OSInode {
    fn drop(&mut self) {
        let inner = self.inner.exclusive_access();
        let inode_id = inner.inode.inode_id();
        let mut open = OPEN_INODES.exclusive_access();
        let count = open.get_mut(&inode_id).unwrap();
        *count -= 1;
        let last = *count == 0;
        if last {
            open.remove(&inode_id);
        }
        drop(open);
        if last && count_links(inode_id) == 0 {
            inner.inode.reclaim();
        } else if self.writable {
            inner.inode.sync();
        }
    }
}

impl OSInode {
    /// create a new inode in memory
    pub fn new(readable: bool, writable: bool, inode: Arc<Inode>) -> Self {
        *OPEN_INODES.exclusive_access().entry(inode.inode_id()).or_default() += 1;
        Self {
            readable,
            writable,
            inner: unsafe { UPSafeCell::new(OSInodeInner { offset: 0, inode }) },
        }
    }
    /// read all data from the inode
    pub fn read_all(&self) -> Vec<u8> {
        let mut inner = self.inner.exclusive_access();
        let mut buffer: Vec<u8> = Vec::with_capacity(512);
        buffer.resize(512, 0);
        let mut v: Vec<u8> = Vec::new();
        loop {
            let len = inner.inode.read_at(inner.offset, &mut buffer);
            if len == 0 {
                break;
            }
            inner.offset += len;
            v.extend_from_slice(&buffer[..len]);
        }
        v
    }
}

lazy_static! {
    pub static ref ROOT_INODE: Arc<Inode> = {
        let efs = EasyFileSystem::open(BLOCK_DEVICE.clone());
        Arc::new(EasyFileSystem::root_inode(&efs))
    };
}

/// List all apps in the root directory
pub fn list_apps() {
    println!("/**** APPS ****");
    for app in ROOT_INODE.ls() {
        println!("{}", app);
    }
    println!("**************/");
}

bitflags! {
    ///  The flags argument to the open() system call is constructed by ORing together zero or more of the following values:
    pub struct OpenFlags: u32 {
        /// readyonly
        const RDONLY = 0;
        /// writeonly
        const WRONLY = 1 << 0;
        /// read and write
        const RDWR = 1 << 1;
        /// create new file
        const CREATE = 1 << 9;
        /// truncate file size to 0
        const TRUNC = 1 << 10;
    }
}

impl OpenFlags {
    /// Do not check validity for simplicity
    /// Return (readable, writable)
    pub fn read_write(&self) -> (bool, bool) {
        if self.is_empty() {
            (true, false)
        } else if self.contains(Self::WRONLY) {
            (false, true)
        } else {
            (true, true)
        }
    }
}

/// Open a file
pub fn open_file(name: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    if name.is_empty() || name.len() >= DIRENT_NAME_SIZE {
        return None;
    }
    let (readable, writable) = flags.read_write();
    if flags.contains(OpenFlags::CREATE) {
        if let Some(inode) = ROOT_INODE.find(name) {
            // clear size
            inode.clear();
            Some(Arc::new(OSInode::new(readable, writable, inode)))
        } else {
            // create file
            ROOT_INODE
                .create(name)
                .map(|inode| Arc::new(OSInode::new(readable, writable, inode)))
        }
    } else {
        ROOT_INODE.find(name).map(|inode| {
            if flags.contains(OpenFlags::TRUNC) {
                inode.clear();
            }
            Arc::new(OSInode::new(readable, writable, inode))
        })
    }
}

const DIRENT_SIZE: usize = 32;
const DIRENT_NAME_SIZE: usize = 28;

fn parse_dirent_name(entry: &[u8; DIRENT_SIZE]) -> &str {
    let len = entry[..DIRENT_NAME_SIZE]
        .iter()
        .position(|ch| *ch == 0)
        .unwrap_or(DIRENT_NAME_SIZE);
    core::str::from_utf8(&entry[..len]).unwrap_or("")
}

fn parse_dirent_inode(entry: &[u8; DIRENT_SIZE]) -> u32 {
    u32::from_le_bytes([entry[28], entry[29], entry[30], entry[31]])
}

fn build_dirent(name: &str, inode_id: u32) -> [u8; DIRENT_SIZE] {
    let mut entry = [0u8; DIRENT_SIZE];
    let bytes = name.as_bytes();
    entry[..bytes.len()].copy_from_slice(bytes);
    entry[28..32].copy_from_slice(&inode_id.to_le_bytes());
    entry
}

fn read_dir_entry(offset: usize) -> Option<[u8; DIRENT_SIZE]> {
    let mut entry = [0u8; DIRENT_SIZE];
    let len = ROOT_INODE.read_at(offset, &mut entry);
    if len == DIRENT_SIZE {
        Some(entry)
    } else {
        None
    }
}

fn find_dir_entry(name: &str) -> Option<(usize, u32)> {
    let mut offset = 0usize;
    while let Some(entry) = read_dir_entry(offset) {
        if parse_dirent_inode(&entry) != 0 && parse_dirent_name(&entry) == name {
            return Some((offset, parse_dirent_inode(&entry)));
        }
        offset += DIRENT_SIZE;
    }
    None
}

fn count_links(inode_id: u32) -> u32 {
    if inode_id == 0 {
        return 0;
    }
    let mut cnt = 0u32;
    let mut offset = 0usize;
    while let Some(entry) = read_dir_entry(offset) {
        if parse_dirent_inode(&entry) == inode_id {
            cnt += 1;
        }
        offset += DIRENT_SIZE;
    }
    cnt
}

fn first_empty_slot_or_end() -> usize {
    let mut offset = 0usize;
    while let Some(entry) = read_dir_entry(offset) {
        if parse_dirent_inode(&entry) == 0 {
            return offset;
        }
        offset += DIRENT_SIZE;
    }
    offset
}

pub(crate) fn link_file(old_name: &str, new_name: &str) -> bool {
    if old_name == new_name || new_name.is_empty() || new_name.len() >= DIRENT_NAME_SIZE {
        return false;
    }
    let (_, inode_id) = if let Some(entry) = find_dir_entry(old_name) {
        entry
    } else {
        return false;
    };
    if find_dir_entry(new_name).is_some() {
        return false;
    }
    let offset = first_empty_slot_or_end();
    let entry = build_dirent(new_name, inode_id);
    ROOT_INODE.write_at(offset, &entry);
    true
}

pub(crate) fn unlink_file(name: &str) -> bool {
    let (offset, inode_id) = if let Some(entry) = find_dir_entry(name) {
        entry
    } else {
        return false;
    };
    let inode = ROOT_INODE.find(name).unwrap();
    ROOT_INODE.write_at(offset, &[0u8; DIRENT_SIZE]);
    if !OPEN_INODES.exclusive_access().contains_key(&inode_id) && count_links(inode_id) == 0 {
        inode.reclaim();
    }
    true
}

impl File for OSInode {
    fn readable(&self) -> bool {
        self.readable
    }
    fn writable(&self) -> bool {
        self.writable
    }
    fn read(&self, mut buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_read_size = 0usize;
        for slice in buf.buffers.iter_mut() {
            let read_size = inner.inode.read_at(inner.offset, *slice);
            if read_size == 0 {
                break;
            }
            inner.offset += read_size;
            total_read_size += read_size;
        }
        total_read_size
    }
    fn write(&self, buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_write_size = 0usize;
        for slice in buf.buffers.iter() {
            let write_size = inner.inode.write_at_cached(inner.offset, slice);
            assert_eq!(write_size, slice.len());
            inner.offset += write_size;
            total_write_size += write_size;
        }
        total_write_size
    }

    fn stat(&self) -> Option<Stat> {
        let inode_id = self.inner.exclusive_access().inode.inode_id();
        let mut stat: Stat = unsafe { core::mem::zeroed() };
        stat.dev = 0;
        stat.ino = inode_id as u64;
        stat.mode = StatMode::FILE;
        stat.nlink = count_links(inode_id);
        Some(stat)
    }
}
