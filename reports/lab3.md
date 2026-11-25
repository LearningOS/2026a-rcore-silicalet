本次 lab3 在 ch5 分支上完成了进程与调度相关功能。系统调用方面补齐了 `sys_spawn`、`sys_set_priority`，并把上一章的 `sys_get_time`、`sys_mmap`、`sys_munmap` 迁移到当前进程结构。`sys_spawn` 直接按程序名创建新子进程并加入就绪队列，成功返回子进程 pid，找不到程序或参数非法时返回 -1；`sys_set_priority` 对 `prio >= 2` 返回设置值，否则返回 -1。

调度器改为 stride 策略：在 runnable 队列中线性扫描 stride 最小的任务进行调度，然后按 `pass = BIG_STRIDE / priority` 累加该任务 stride。为此在 TCB 中新增了 `stride` 与 `priority` 字段，并设置初始值为 `stride = 0`、`priority = 16`。这样在测例中六个不同优先级进程的运行次数能够保持大致正比关系，满足 ch5 对公平性的要求。

内存相关迁移里，`sys_get_time` 按微秒时间源填充用户态 `TimeVal`，并通过跨页安全拷贝写回用户缓冲区；`sys_mmap` 对地址对齐、长度、权限位进行合法性检查后建立用户映射；`sys_munmap` 仅允许按已记录区间完整解除映射。调试过程中修复了 `ch4_unmap` 失败问题，根因是映射冲突判断把无效页表项也当成已映射，修正为只在 PTE 有效时判冲突后该测例恢复通过。

先在仓库根目录运行 `make fmt`，再运行 `make -C ci-user test CHAPTER=5`。功能检查阶段结果为 `Test passed: 15/15`，`ch5_usertest` 输出通过，`spawn/set_priority/mmap/munmap/get_time/stride` 相关测例均通过
