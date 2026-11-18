//! The stack switches: the only code that changes which stack a worker runs
//! on, and so the only way a task starts, stops or goes on.
//!
//! A task that is not running keeps its registers in a switch frame at its
//! saved stack pointer. The frame record comes first in it on arm64, as in
//! any frame: the frame pointer, then the address to resume at, which for a
//! task that has started is a return address into compiled code with a
//! stack map. Then come the registers the calling convention has a callee
//! keep, less the pinned one: whoever resumes the task writes its own
//! worker there. The compiled frame that called the stub is just above the
//! switch frame.
//!
//! One word of the frame is what the task resumes with in its result
//! register: zero or a reference, which the collector keeps current. In the
//! frame a worker leaves on its own stack that word holds the worker's
//! caller's pinned register instead, since Rust code keeps a value there.
//!
//! A resume ends in an indirect branch, not a return: a return address
//! that was not pushed by the matching call would be mispredicted every
//! time.
//!
//! The stubs compiled code calls ([`baml_rt_spawn`], [`baml_rt_await`],
//! [`baml_rt_sleep`]) build the switch frame first and then call Rust with
//! its address, on the task's own stack. Rust decides; the stub switches.

use super::{Task, await_park, exit_pick, sleep_park, spawn_prepare};
use crate::layout::{MUTATOR_CUR_TASK, MUTATOR_G0_SP, MUTATOR_STACK_LIMIT};

/// The size of a switch frame, and where in it the frame record and the
/// result word are.
#[cfg(target_arch = "aarch64")]
pub(crate) const SWITCH_FRAME: usize = 160;
#[cfg(target_arch = "aarch64")]
pub(crate) const SWITCH_FRAME_RECORD: usize = 0;
#[cfg(target_arch = "aarch64")]
pub(crate) const SWITCH_FRAME_RESULT: usize = 88;
/// The word of a new task's first frame that the boot code finds the
/// function it runs in (x19 on arm64, rbx on x86-64).
#[cfg(target_arch = "aarch64")]
pub(crate) const SWITCH_FRAME_BOOT: usize = 16;

#[cfg(target_arch = "x86_64")]
pub(crate) const SWITCH_FRAME: usize = 64;
#[cfg(target_arch = "x86_64")]
pub(crate) const SWITCH_FRAME_RECORD: usize = 48;
#[cfg(target_arch = "x86_64")]
pub(crate) const SWITCH_FRAME_RESULT: usize = 8;
#[cfg(target_arch = "x86_64")]
pub(crate) const SWITCH_FRAME_BOOT: usize = 40;

const TASK_SP: usize = std::mem::offset_of!(Task, sp);
const TASK_STACK_LIMIT: usize = std::mem::offset_of!(Task, stack_limit);

/// arm64: push a switch frame holding x19, x20, x22-x28, the frame record
/// and d8-d15.
#[cfg(target_arch = "aarch64")]
macro_rules! save {
    () => {
        "stp x29, x30, [sp, #-160]!
         stp x19, x20, [sp, #16]
         stp x22, x23, [sp, #32]
         stp x24, x25, [sp, #48]
         stp x26, x27, [sp, #64]
         str x28, [sp, #80]
         stp d8, d9, [sp, #96]
         stp d10, d11, [sp, #112]
         stp d12, d13, [sp, #128]
         stp d14, d15, [sp, #144]"
    };
}

/// arm64: reload what [`save`] stored but the frame record, from the frame
/// the stack pointer is at.
#[cfg(target_arch = "aarch64")]
macro_rules! restore {
    () => {
        "ldp x19, x20, [sp, #16]
         ldp x22, x23, [sp, #32]
         ldp x24, x25, [sp, #48]
         ldp x26, x27, [sp, #64]
         ldr x28, [sp, #80]
         ldp d8, d9, [sp, #96]
         ldp d10, d11, [sp, #112]
         ldp d12, d13, [sp, #128]
         ldp d14, d15, [sp, #144]"
    };
}

/// arm64: go on as the task in x0, on the worker in x21: it becomes the
/// worker's current task, its stack limit the worker's, and it resumes
/// where its switch frame says with the frame's result word in x0.
#[cfg(target_arch = "aarch64")]
macro_rules! switch_in {
    () => {
        concat!(
            "str x0, [x21, #{cur_task}]
             ldr x9, [x0, #{task_stack_limit}]
             str x9, [x21, #{stack_limit}]
             ldr x9, [x0, #{task_sp}]
             mov sp, x9\n",
            restore!(),
            "\nldr x0, [sp, #88]
             ldp x29, x16, [sp], #160
             br x16"
        )
    };
}

/// arm64: leave the task's stack for the worker's own, on the worker in
/// x21: return from the [`baml_rt_resume`] that worker last called.
#[cfg(target_arch = "aarch64")]
macro_rules! switch_to_worker {
    () => {
        concat!(
            "ldr x9, [x21, #{g0_sp}]
             mov sp, x9\n",
            restore!(),
            "\nldr x21, [sp, #88]
             ldp x29, x30, [sp], #160
             ret"
        )
    };
}

/// Run `task` on `worker` until a task leaves for the worker's own stack:
/// one that parks or that ends with nothing else to run. The worker's
/// current task is then the one that left.
///
/// # Safety
///
/// Called on a worker's own stack with a task that is not running and whose
/// saved stack pointer is at a switch frame.
#[cfg(target_arch = "aarch64")]
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn baml_rt_resume(worker: *mut crate::heap::Mutator, task: *mut Task) {
    core::arch::naked_asm!(
        save!(),
        "str x21, [sp, #88]",
        "mov x21, x0",
        "mov x9, sp",
        "str x9, [x21, #{g0_sp}]",
        "mov x0, x1",
        switch_in!(),
        g0_sp = const MUTATOR_G0_SP,
        cur_task = const MUTATOR_CUR_TASK,
        stack_limit = const MUTATOR_STACK_LIMIT,
        task_sp = const TASK_SP,
        task_stack_limit = const TASK_STACK_LIMIT,
    )
}

/// `spawn`: takes the closure to run, the compiled function a task running
/// one of its result type starts in, and the future's flags. Returns the
/// new task's future, after the new task has run until it first parks or
/// ends.
///
/// # Safety
///
/// Called from compiled code only, on a task, with the worker in x21.
#[cfg(target_arch = "aarch64")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_spawn() {
    core::arch::naked_asm!(
        save!(),
        "mov x29, sp",
        "str xzr, [sp, #88]",
        "mov x3, x21",
        "mov x4, sp",
        "bl {prepare}",
        switch_in!(),
        prepare = sym spawn_prepare,
        cur_task = const MUTATOR_CUR_TASK,
        stack_limit = const MUTATOR_STACK_LIMIT,
        task_sp = const TASK_SP,
        task_stack_limit = const TASK_STACK_LIMIT,
    )
}

/// A stub that parks the calling task when `$park` says to: it is called
/// with the stub's one argument, the worker and the switch frame, and
/// returns zero when the task goes on at once.
#[cfg(target_arch = "aarch64")]
macro_rules! parking_stub {
    ($(#[$attribute:meta])* $name:ident => $park:path) => {
        $(#[$attribute])*
        ///
        /// # Safety
        ///
        /// Called from compiled code only, on a task, with the worker in
        /// x21.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                save!(),
                "mov x29, sp",
                "str xzr, [sp, #88]",
                "mov x1, x21",
                "mov x2, sp",
                "bl {park}",
                "cbz x0, 2f",
                switch_to_worker!(),
                // Rust kept every register the frame holds.
                "2:",
                "ldp x29, x30, [sp], #160",
                "ret",
                park = sym $park,
                g0_sp = const MUTATOR_G0_SP,
            )
        }
    };
}

/// Where a new task starts: its first switch frame resumes here with the
/// function it runs in x19. When that returns the task is over.
///
/// # Safety
///
/// Reached only by resuming a new task.
#[cfg(target_arch = "aarch64")]
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn baml_rt_task_boot() {
    core::arch::naked_asm!(
        "mov x0, x21",
        "blr x19",
        "mov x0, x21",
        "bl {exit}",
        "cbz x0, 2f",
        switch_in!(),
        "2:",
        switch_to_worker!(),
        exit = sym exit_pick,
        g0_sp = const MUTATOR_G0_SP,
        cur_task = const MUTATOR_CUR_TASK,
        stack_limit = const MUTATOR_STACK_LIMIT,
        task_sp = const TASK_SP,
        task_stack_limit = const TASK_STACK_LIMIT,
    )
}

/// Call `run(data)` with the stack pointer at `stack`, which is aligned as
/// a call needs, and return on the stack this was called on. The frame
/// pointer keeps the caller's stack pointer through the call.
///
/// # Safety
///
/// Nothing else uses the memory below `stack` until this returns, and
/// there is enough of it for `run`.
#[cfg(target_arch = "aarch64")]
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn call_on(
    data: *mut u8,
    run: unsafe extern "C" fn(*mut u8),
    stack: usize,
) {
    core::arch::naked_asm!(
        "stp x29, x30, [sp, #-16]!",
        "mov x29, sp",
        "mov sp, x2",
        "blr x1",
        "mov sp, x29",
        "ldp x29, x30, [sp], #16",
        "ret",
    )
}

/// See the arm64 [`call_on`].
///
/// # Safety
///
/// As there.
#[cfg(all(target_arch = "x86_64", not(target_os = "windows")))]
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn call_on(
    data: *mut u8,
    run: unsafe extern "C" fn(*mut u8),
    stack: usize,
) {
    core::arch::naked_asm!(
        "push rbp",
        "mov rbp, rsp",
        "mov rsp, rdx",
        "call rsi",
        "mov rsp, rbp",
        "pop rbp",
        "ret",
    )
}

// x86-64, System V. Not built or run: written on an arm64 host.
//
// The frame, from the saved stack pointer up: a pad word, the result word,
// r14, r13, r12, rbx, rbp, the return address. The worker is in r15.

#[cfg(all(target_arch = "x86_64", not(target_os = "windows")))]
macro_rules! save {
    () => {
        "push rbp
         push rbx
         push r12
         push r13
         push r14
         push 0
         push 0"
    };
}

/// x86-64: go on as the task in rdi, on the worker in r15.
#[cfg(all(target_arch = "x86_64", not(target_os = "windows")))]
macro_rules! switch_in {
    () => {
        "mov [r15 + {cur_task}], rdi
         mov rax, [rdi + {task_stack_limit}]
         mov [r15 + {stack_limit}], rax
         mov rsp, [rdi + {task_sp}]
         pop rax
         pop rax
         pop r14
         pop r13
         pop r12
         pop rbx
         pop rbp
         pop rcx
         jmp rcx"
    };
}

/// x86-64: leave the task's stack for the worker's own, on the worker in
/// r15.
#[cfg(all(target_arch = "x86_64", not(target_os = "windows")))]
macro_rules! switch_to_worker {
    () => {
        "mov rsp, [r15 + {g0_sp}]
         pop rax
         pop r15
         pop r14
         pop r13
         pop r12
         pop rbx
         pop rbp
         ret"
    };
}

/// See the arm64 [`baml_rt_resume`].
///
/// # Safety
///
/// As there.
#[cfg(all(target_arch = "x86_64", not(target_os = "windows")))]
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn baml_rt_resume(worker: *mut crate::heap::Mutator, task: *mut Task) {
    core::arch::naked_asm!(
        save!(),
        "mov [rsp + 8], r15",
        "mov r15, rdi",
        "mov [r15 + {g0_sp}], rsp",
        "mov rdi, rsi",
        switch_in!(),
        g0_sp = const MUTATOR_G0_SP,
        cur_task = const MUTATOR_CUR_TASK,
        stack_limit = const MUTATOR_STACK_LIMIT,
        task_sp = const TASK_SP,
        task_stack_limit = const TASK_STACK_LIMIT,
    )
}

/// See the arm64 [`baml_rt_spawn`].
///
/// # Safety
///
/// Called from compiled code only, on a task, with the worker in r15.
#[cfg(all(target_arch = "x86_64", not(target_os = "windows")))]
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_spawn() {
    core::arch::naked_asm!(
        save!(),
        "mov rcx, r15",
        "mov r8, rsp",
        "call {prepare}",
        "mov rdi, rax",
        switch_in!(),
        prepare = sym spawn_prepare,
        cur_task = const MUTATOR_CUR_TASK,
        stack_limit = const MUTATOR_STACK_LIMIT,
        task_sp = const TASK_SP,
        task_stack_limit = const TASK_STACK_LIMIT,
    )
}

#[cfg(all(target_arch = "x86_64", not(target_os = "windows")))]
macro_rules! parking_stub {
    ($(#[$attribute:meta])* $name:ident => $park:path) => {
        $(#[$attribute])*
        ///
        /// # Safety
        ///
        /// Called from compiled code only, on a task, with the worker in
        /// r15.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                save!(),
                "mov rsi, r15",
                "mov rdx, rsp",
                "call {park}",
                "test rax, rax",
                "jz 2f",
                switch_to_worker!(),
                // Rust kept every register the frame holds.
                "2:",
                "add rsp, 48",
                "pop rbp",
                "ret",
                park = sym $park,
                g0_sp = const MUTATOR_G0_SP,
            )
        }
    };
}

/// See the arm64 [`baml_rt_task_boot`]: the function to run is in rbx. The
/// stack pointer is 16-aligned here, as the convention wants it at a call.
///
/// # Safety
///
/// Reached only by resuming a new task.
#[cfg(all(target_arch = "x86_64", not(target_os = "windows")))]
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn baml_rt_task_boot() {
    core::arch::naked_asm!(
        "mov rdi, r15",
        "call rbx",
        "mov rdi, r15",
        "call {exit}",
        "test rax, rax",
        "jz 2f",
        "mov rdi, rax",
        switch_in!(),
        "2:",
        switch_to_worker!(),
        exit = sym exit_pick,
        g0_sp = const MUTATOR_G0_SP,
        cur_task = const MUTATOR_CUR_TASK,
        stack_limit = const MUTATOR_STACK_LIMIT,
        task_sp = const TASK_SP,
        task_stack_limit = const TASK_STACK_LIMIT,
    )
}

parking_stub! {
    /// `await` of a future that is not ready: takes the future and returns
    /// once it has settled, with a value or with an error.
    baml_rt_await => await_park
}

parking_stub! {
    /// `baml.sys.sleep`: takes a count of nanoseconds and returns once that
    /// long has passed.
    baml_rt_sleep => sleep_park
}
