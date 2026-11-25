在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

交流群 
qemu10疑似不行
可以降低qemu版本试下

此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：


3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。

---

## trap.S 深入理解

### L40：刚进入 __restore 时，sp 代表了什么值。请指出 __restore 的两种使用情景。

**刚进入 __restore 时，sp 的值：**

`sp` 指向 **内核栈上的 TrapContext**（陷阱上下文结构体）。此时：
- `sp` = 内核栈上已分配好的 TrapContext 的起始地址（经过 `__alltraps` 的 `addi sp, sp, -34*8` 后）
- `sscratch` = 用户栈指针（进入 `__alltraps` 时通过 `csrrw sp, sscratch, sp` 交换得到）

**__restore 的两种使用情景：**

1. **从 trap handler 返回用户态**：当操作系统处理完一个 trap（系统调用或外设中断），需要恢复用户态进程的上下文。此时调用 `__restore` 恢复所有寄存器，然后通过 `sret` 返回 U 态。

2. **首次启动用户进程**：当创建第一个用户态进程时，操作系统初始化好 TrapContext（设置好用户态寄存器值），然后调用 `__restore` 恢复上下文并通过 `sret` 跳转到用户态入口点执行。

---

### L43-L48：这几行汇编代码特殊处理了哪些寄存器？这些寄存器的值对于进入用户态有何意义？请分别解释。

```asm
ld t0, 32*8(sp)
ld t1, 33*8(sp)
ld t2, 2*8(sp)
csrw sstatus, t0
csrw sepc, t1
csrw sscratch, t2
```

**特殊处理的寄存器：**

| 指令 | 源偏移 | 目标寄存器 | 含义 |
|------|--------|------------|------|
| `ld t0, 32*8(sp)` | 32*8 | t0 | 从 TrapContext 恢复 sstatus（状态寄存器） |
| `ld t1, 33*8(sp)` | 33*8 | t1 | 从 TrapContext 恢复 sepc（ trap 发生时的 pc） |
| `ld t2, 2*8(sp)` | 2*8 | t2 | 从 TrapContext 恢复用户栈指针 |

**对进入用户态的意义：**

1. **sstatus**：控制 CPU 的特权模式、终端使能位等关键状态。恢复 sstatus 使得 `sret` 执行后 CPU 处于正确的特权模式（S 态，且用户态中断/异常处理位正确）。

2. **sepc**：`sret` 指令会将 sepc 的值复制到 pc，从而恢复 trap 发生时的下一条指令继续执行。如果没有 sepc，CPU 不知道该返回到哪里继续执行。

3. **sscratch**：保存用户栈指针。恢复后在 `sret` 前通过 `csrrw sp, sscratch, sp` 将 sp 切换回用户栈，确保用户态程序使用自己的栈空间而非内核栈。

---

### L50-L56：为何跳过了 x2 和 x4？

```asm
ld x1, 1*8(sp)
ld x3, 3*8(sp)
.set n, 5
.rept 27
   LOAD_GP %n
   .set n, n+1
.endr
```

**跳过 x2 (sp) 和 x4 (tp) 的原因：**

- **x2 (sp)**：栈指针寄存器。sp 不能简单地从 TrapContext 中恢复，因为 sp 在 `__restore` 执行过程中需要用来访问 TrapContext 结构体本身（用于加载其他寄存器）。sp 的恢复是通过 `csrrw sp, sscratch, sp` 指令完成的——它直接用 sscratch 的值写入 sp（sscratch 在 L48 已被恢复为用户栈指针）。

- **x4 (tp)**：线程指针（Thread Pointer）。在 rCore/OS 的实现中，内核线程不使用 tp 寄存器，应用程序也不应该使用 tp。如果保存/恢复 tp，会破坏内核调度器的线程局部存储机制。因此跳过。

---

### L60：该指令之后，sp 和 sscratch 中的值分别有什么意义？

```asm
csrrw sp, sscratch, sp
```

**执行后的值：**

- **sp** = 原 `sscratch` 的值 = **用户栈指针 (user stack pointer)**。此时 sp 指向用户态进程的栈空间。

- **sscratch** = 原 `sp` 的值 = **内核栈指针 (kernel stack pointer)**。sscratch 保存了内核栈的地址，以便下次进入 S 态时能够恢复内核栈。

**为何这样设计：**

这种交换机制确保每次 trap 进入内核时，sscratch 都保存着用户栈指针，使得内核能够正确恢复用户态上下文。同时，内核栈指针被保存在 sscratch 中，下次 trap 时可以恢复。

---

### __restore 中发生状态切换在哪一条指令？为何该指令执行之后会进入用户态？

**状态切换指令：`sret`**

`__restore` 的最后两条指令：
```asm
csrrw sp, sscratch, sp   # L60: 恢复 sp 为用户栈
sret                       # L61: 执行状态切换
```

**sret 执行后进入用户态的原因：**

`sret`（Supervisor Return）是 RISC-V 特权指令，专门用于从 S 态返回到 U 态。它的行为：

1. **pc ← sepc**：将 sepc（之前在 L47 恢复）的值写入 pc，CPU 从该地址继续执行
2. **sstatus.SIE ← sstatus.SPIE**：恢复中断使能状态
3. **sstatus.SPP ← 0**：将之前的特权模式位清零（表示返回到 U 态）
4. **特权模式切换**：CPU 从 S 态切换到 U 态

`__restore` 在 `sret` 之前通过 `csrrw sp, sscratch, sp` 将 sp 切换为用户栈，并将 sstatus/sepc/sscratch 恢复为用户态的值，`sret` 正是利用这些值完成最终的切换。

---

### L13：该指令之后，sp 和 sscratch 中的值分别有什么意义？

```asm
csrrw sp, sscratch, sp
```

**进入 `__alltraps` 时：**

- 此时 CPU 刚从 U 态进入 S 态，`sscratch` 持有**用户栈指针**（这是硬件/上次 `sret` 留下的状态）
- `sp` 持有**内核栈指针**（这是内核代码设置的）

**执行 `csrrw sp, sscratch, sp` 后（原子交换）：**

- **sp** = 原 `sscratch` = **用户栈指针**。进入 trap handler 后，内核用用户栈的地址初始化 sp（虽然稍后会被重新分配 TrapContext 覆盖）。

- **sscratch** = 原 `sp` = **内核栈指针**。sscratch 现在保存内核栈地址，这样 trap 处理完毕后能够通过 sscratch 恢复内核栈。

**Trap Frame 布局（34*8 = 272 字节）：**

| 偏移 | 寄存器 | 说明 |
|------|--------|------|
| 0*8 | (空) | 对齐 |
| 1*8 | x1 | |
| 2*8 | x2 (sp) | 稍后从 sscratch 保存用户 sp |
| 3*8 | x3 | |
| 4*8 | x4 (tp) | 不保存 |
| 5*8 ~ 31*8 | x5 ~ x31 | 27 个通用寄存器 |
| 32*8 | sstatus | |
| 33*8 | sepc | |

---

### 从 U 态进入 S 态是哪一条指令发生的？

**从 U 态进入 S 态：`sret`**

当 trap（系统调用或外设中断）发生在 U 态时：

1. **硬件自动完成**（不需要软件指令）：
   - sepc ← 当前的 pc
   - sstatus.SPP ← U 态 (1)
   - pc ← stvec（trap 向量地址）
   - sstatus.SIE ← 0（关闭中断）
   - 切换到 S 态

2. **`sret` 指令**是将 CPU 从 S 态**返回到 U 态**的指令。

3. **从 U 态进入 S 态**并不是一条单独的指令，而是 **trap 发生时硬件自动完成的状态切换**。硬件通过 stvec 寄存器跳转到 trap handler（`__alltraps`），从而进入 S 态执行内核代码。
