//! Process management syscalls
use crate::config::{CLOCK_FREQ, PAGE_SIZE};
use crate::mm::{
    translated_byte_buffer, MapPermission, PTEFlags, PageTable, VirtAddr, VirtPageNum,
};
use crate::task::{
    change_program_brk, current_syscall_times, current_task_mmap, current_task_munmap,
    current_user_token, exit_current_and_run_next, suspend_current_and_run_next,
};
use crate::timer::get_time;
use core::mem::size_of;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

fn user_range_perm_ok(
    token: usize,
    start: usize,
    len: usize,
    need_read: bool,
    need_write: bool,
) -> bool {
    if len == 0 {
        return true;
    }
    let end = if let Some(end) = start.checked_add(len) {
        end
    } else {
        return false;
    };
    let page_table = PageTable::from_token(token);
    let start_vpn = VirtAddr::from(start).floor().0;
    let end_vpn = VirtAddr::from(end - 1).floor().0;
    for vpn in start_vpn..=end_vpn {
        let pte = if let Some(pte) = page_table.translate(VirtPageNum(vpn)) {
            pte
        } else {
            return false;
        };
        if !pte.is_valid() {
            return false;
        }
        let flags = pte.flags();
        if !flags.contains(PTEFlags::U) {
            return false;
        }
        if need_read && !pte.readable() {
            return false;
        }
        if need_write && !pte.writable() {
            return false;
        }
    }
    true
}

fn prot_to_map_permission(prot: usize) -> Option<MapPermission> {
    if (prot & !0x7) != 0 || (prot & 0x7) == 0 {
        return None;
    }
    let mut perm = MapPermission::U;
    if (prot & 0x1) != 0 {
        perm |= MapPermission::R;
    }
    if (prot & 0x2) != 0 {
        perm |= MapPermission::W;
    }
    if (prot & 0x4) != 0 {
        perm |= MapPermission::X;
    }
    Some(perm)
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let token = current_user_token();
    let len = size_of::<TimeVal>();
    let start = ts as usize;
    if !user_range_perm_ok(token, start, len, false, true) {
        return -1;
    }
    let us = get_time() * 1_000_000 / CLOCK_FREQ;
    let tv = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    let src = unsafe { core::slice::from_raw_parts((&tv as *const TimeVal) as *const u8, len) };
    let mut copied = 0usize;
    for buffer in translated_byte_buffer(token, ts as *const u8, len) {
        let blen = buffer.len();
        buffer.copy_from_slice(&src[copied..copied + blen]);
        copied += blen;
    }
    0
}

/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    match trace_request {
        0 => {
            let token = current_user_token();
            if !user_range_perm_ok(token, id, 1, true, false) {
                return -1;
            }
            let buffers = translated_byte_buffer(token, id as *const u8, 1);
            buffers[0][0] as isize
        }
        1 => {
            let token = current_user_token();
            if !user_range_perm_ok(token, id, 1, false, true) {
                return -1;
            }
            let mut buffers = translated_byte_buffer(token, id as *const u8, 1);
            buffers[0][0] = data as u8;
            0
        }
        2 => current_syscall_times(id),
        _ => -1,
    }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel: sys_mmap");
    if (start & (PAGE_SIZE - 1)) != 0 {
        return -1;
    }
    let permission = if let Some(permission) = prot_to_map_permission(prot) {
        permission
    } else {
        return -1;
    };
    if len == 0 {
        return 0;
    }
    let aligned_len = if let Some(value) = len
        .checked_add(PAGE_SIZE - 1)
        .map(|value| value / PAGE_SIZE * PAGE_SIZE)
    {
        value
    } else {
        return -1;
    };
    if start.checked_add(aligned_len).is_none() {
        return -1;
    }
    if current_task_mmap(start, aligned_len, permission) {
        0
    } else {
        -1
    }
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap");
    if (start & (PAGE_SIZE - 1)) != 0 || (len & (PAGE_SIZE - 1)) != 0 {
        return -1;
    }
    if len == 0 {
        return 0;
    }
    if start.checked_add(len).is_none() {
        return -1;
    }
    if current_task_munmap(start, len) {
        0
    } else {
        -1
    }
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
