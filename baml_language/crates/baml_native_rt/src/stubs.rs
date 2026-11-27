//! The slow paths compiled code branches to from its fast paths: the
//! allocation that does not fit and the loop poll that is set. The stubs
//! that throw are here too.
//!
//! Each is called with a convention under which the callee preserves every
//! register, so the call costs the fast path nothing: no value around it has
//! to sit in a callee-saved register or be saved and restored. The symbols
//! are these stubs, which save every register the C convention lets a
//! callee change, call the Rust function that does the work, and restore
//! them. They take and return nothing in registers; the allocation slow
//! path's size and result travel in the mutator's scratch word.
//!
//! A stub's first two instructions make the frame record the collector's
//! stack walk starts from, so the compiled caller's stack pointer at the
//! call is just above it, as for any runtime function. References the
//! caller holds are in its frame, in the slots its stack map names: the
//! copies saved here are not roots, and compiled code reads none of them
//! after the call.

use crate::{
    heap::{alloc_slow, poll_slow},
    unwind,
};

#[cfg(not(any(
    target_arch = "aarch64",
    all(target_arch = "x86_64", not(target_os = "windows"))
)))]
compile_error!("the slow path stubs are written for arm64 and for x86-64 with the System V ABI");

/// A stub named `$name` that calls `$slow` with the mutator and the stub's
/// frame record.
///
/// On arm64 it saves x0-x17 and all of v0-v31: the C convention keeps only
/// the low halves of v8-v15. x18 is the platform's and never allocated;
/// x19-x28 are the callee's to keep anyway.
#[cfg(target_arch = "aarch64")]
macro_rules! preserving_stub {
    ($(#[$attribute:meta])* $name:ident => $slow:path) => {
        $(#[$attribute])*
        ///
        /// # Safety
        ///
        /// Called from compiled code only, with the mutator in x21.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "stp x29, x30, [sp, #-16]!",
                "mov x29, sp",
                "sub sp, sp, #656",
                "stp x0, x1, [sp, #0]",
                "stp x2, x3, [sp, #16]",
                "stp x4, x5, [sp, #32]",
                "stp x6, x7, [sp, #48]",
                "stp x8, x9, [sp, #64]",
                "stp x10, x11, [sp, #80]",
                "stp x12, x13, [sp, #96]",
                "stp x14, x15, [sp, #112]",
                "stp x16, x17, [sp, #128]",
                "stp q0, q1, [sp, #144]",
                "stp q2, q3, [sp, #176]",
                "stp q4, q5, [sp, #208]",
                "stp q6, q7, [sp, #240]",
                "stp q8, q9, [sp, #272]",
                "stp q10, q11, [sp, #304]",
                "stp q12, q13, [sp, #336]",
                "stp q14, q15, [sp, #368]",
                "stp q16, q17, [sp, #400]",
                "stp q18, q19, [sp, #432]",
                "stp q20, q21, [sp, #464]",
                "stp q22, q23, [sp, #496]",
                "stp q24, q25, [sp, #528]",
                "stp q26, q27, [sp, #560]",
                "stp q28, q29, [sp, #592]",
                "stp q30, q31, [sp, #624]",
                "mov x0, x21",
                "mov x1, x29",
                "bl {slow}",
                "ldp x0, x1, [sp, #0]",
                "ldp x2, x3, [sp, #16]",
                "ldp x4, x5, [sp, #32]",
                "ldp x6, x7, [sp, #48]",
                "ldp x8, x9, [sp, #64]",
                "ldp x10, x11, [sp, #80]",
                "ldp x12, x13, [sp, #96]",
                "ldp x14, x15, [sp, #112]",
                "ldp x16, x17, [sp, #128]",
                "ldp q0, q1, [sp, #144]",
                "ldp q2, q3, [sp, #176]",
                "ldp q4, q5, [sp, #208]",
                "ldp q6, q7, [sp, #240]",
                "ldp q8, q9, [sp, #272]",
                "ldp q10, q11, [sp, #304]",
                "ldp q12, q13, [sp, #336]",
                "ldp q14, q15, [sp, #368]",
                "ldp q16, q17, [sp, #400]",
                "ldp q18, q19, [sp, #432]",
                "ldp q20, q21, [sp, #464]",
                "ldp q22, q23, [sp, #496]",
                "ldp q24, q25, [sp, #528]",
                "ldp q26, q27, [sp, #560]",
                "ldp q28, q29, [sp, #592]",
                "ldp q30, q31, [sp, #624]",
                "mov sp, x29",
                "ldp x29, x30, [sp], #16",
                "ret",
                slow = sym $slow,
            )
        }
    };
}

/// On x86-64 it saves rax, rcx, rdx, rsi, rdi, r8-r11 and xmm0-xmm15, the
/// registers the System V convention lets a callee change. Compiled code
/// uses no vector register wider than that. The mutator is in r15.
///
/// Not built or run: written on an arm64 host.
#[cfg(all(target_arch = "x86_64", not(target_os = "windows")))]
macro_rules! preserving_stub {
    ($(#[$attribute:meta])* $name:ident => $slow:path) => {
        $(#[$attribute])*
        ///
        /// # Safety
        ///
        /// Called from compiled code only, with the mutator in r15.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push rbp",
                "mov rbp, rsp",
                "sub rsp, 336",
                "mov [rsp], rax",
                "mov [rsp + 8], rcx",
                "mov [rsp + 16], rdx",
                "mov [rsp + 24], rsi",
                "mov [rsp + 32], rdi",
                "mov [rsp + 40], r8",
                "mov [rsp + 48], r9",
                "mov [rsp + 56], r10",
                "mov [rsp + 64], r11",
                "movups [rsp + 80], xmm0",
                "movups [rsp + 96], xmm1",
                "movups [rsp + 112], xmm2",
                "movups [rsp + 128], xmm3",
                "movups [rsp + 144], xmm4",
                "movups [rsp + 160], xmm5",
                "movups [rsp + 176], xmm6",
                "movups [rsp + 192], xmm7",
                "movups [rsp + 208], xmm8",
                "movups [rsp + 224], xmm9",
                "movups [rsp + 240], xmm10",
                "movups [rsp + 256], xmm11",
                "movups [rsp + 272], xmm12",
                "movups [rsp + 288], xmm13",
                "movups [rsp + 304], xmm14",
                "movups [rsp + 320], xmm15",
                "mov rdi, r15",
                "mov rsi, rbp",
                "call {slow}",
                "mov rax, [rsp]",
                "mov rcx, [rsp + 8]",
                "mov rdx, [rsp + 16]",
                "mov rsi, [rsp + 24]",
                "mov rdi, [rsp + 32]",
                "mov r8, [rsp + 40]",
                "mov r9, [rsp + 48]",
                "mov r10, [rsp + 56]",
                "mov r11, [rsp + 64]",
                "movups xmm0, [rsp + 80]",
                "movups xmm1, [rsp + 96]",
                "movups xmm2, [rsp + 112]",
                "movups xmm3, [rsp + 128]",
                "movups xmm4, [rsp + 144]",
                "movups xmm5, [rsp + 160]",
                "movups xmm6, [rsp + 176]",
                "movups xmm7, [rsp + 192]",
                "movups xmm8, [rsp + 208]",
                "movups xmm9, [rsp + 224]",
                "movups xmm10, [rsp + 240]",
                "movups xmm11, [rsp + 256]",
                "movups xmm12, [rsp + 272]",
                "movups xmm13, [rsp + 288]",
                "movups xmm14, [rsp + 304]",
                "movups xmm15, [rsp + 320]",
                "mov rsp, rbp",
                "pop rbp",
                "ret",
                slow = sym $slow,
            )
        }
    };
}

/// A stub named `$name` for a function of the runtime that may collect, or
/// that reads the stack of the task it is called on. Compiled code calls
/// the stub, which records in the worker where it was called from and goes
/// on in `$function`: that gets the stub's arguments as they came and
/// returns to the stub's caller.
///
/// What is recorded is the worker's anchor: the caller's stack pointer,
/// the return address into it and its frame pointer, as they are on entry
/// to the stub, before any Rust has run. So what a collection walks from
/// does not depend on how the compiler lays out the Rust function's frame,
/// or on whether it gives it one: a function whose call the compiler made
/// a jump, or that made a frame only on a rare path, used to be read as
/// its caller.
///
/// The worker is the pinned register, not the first argument. On arm64
/// the stub is four instructions and uses x16, which a call may change.
/// The stub is its symbol: nothing in the runtime calls it by name.
#[cfg(target_arch = "aarch64")]
macro_rules! anchored {
    ($(#[$attribute:meta])* $name:ident => $function:path) => {
        $(#[$attribute])*
        ///
        /// # Safety
        ///
        /// Called from compiled code only, with the worker in x21 and the
        /// arguments the function it goes on in takes.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "mov x16, sp",
                "stp x16, x30, [x21, #{anchor}]",
                "str x29, [x21, #{anchor} + 16]",
                "b {function}",
                anchor = const $crate::layout::MUTATOR_ANCHOR,
                function = sym $function,
            )
        }
    };
}

/// On x86-64 the return address is on the stack and the caller's stack
/// pointer just above it. The stub uses rax, which carries no argument.
///
/// Not built or run: written on an arm64 host.
#[cfg(all(target_arch = "x86_64", not(target_os = "windows")))]
macro_rules! anchored {
    ($(#[$attribute:meta])* $name:ident => $function:path) => {
        $(#[$attribute])*
        ///
        /// # Safety
        ///
        /// Called from compiled code only, with the worker in r15 and the
        /// arguments the function it goes on in takes.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "lea rax, [rsp + 8]",
                "mov [r15 + {anchor}], rax",
                "mov rax, [rsp]",
                "mov [r15 + {anchor} + 8], rax",
                "mov [r15 + {anchor} + 16], rbp",
                "jmp {function}",
                anchor = const $crate::layout::MUTATOR_ANCHOR,
                function = sym $function,
            )
        }
    };
}

pub(crate) use anchored;

preserving_stub! {
    /// The mutator's span cannot take the bytes its scratch word asks for.
    /// On return the scratch word holds the address of that much
    /// uninitialized memory. May collect.
    baml_rt_alloc_slow => alloc_slow
}

preserving_stub! {
    /// A compiled loop found the poll word set.
    baml_rt_poll_slow => poll_slow
}

/// A stub named `$name` that never returns to its caller: it calls
/// `$throw` with its three arguments, the mutator, its own frame record and
/// room for an [`unwind::Resume`], and then goes on where that says: in the
/// handler's frame, with the tag and the payload of the value thrown in
/// the two registers an exception edge reads them from, on the worker
/// named. The context the value travels with is in that worker's scratch
/// word.
///
/// On arm64 a handler lands with x0 and x1, on x86-64 with rax and rdx.
#[cfg(target_arch = "aarch64")]
macro_rules! throwing_stub {
    ($(#[$attribute:meta])* $name:ident => $throw:path) => {
        $(#[$attribute])*
        ///
        /// # Safety
        ///
        /// Called from compiled code only, with the mutator in x21.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "stp x29, x30, [sp, #-16]!",
                "mov x29, sp",
                "sub sp, sp, #48",
                "mov x3, x21",
                "mov x4, x29",
                "mov x5, sp",
                "bl {throw}",
                "ldp x2, x3, [sp, #0]",
                "ldp x4, x0, [sp, #16]",
                "ldp x1, x21, [sp, #32]",
                "mov sp, x2",
                "mov x29, x3",
                "br x4",
                throw = sym $throw,
            )
        }
    };
}

/// Not built or run: written on an arm64 host.
#[cfg(all(target_arch = "x86_64", not(target_os = "windows")))]
macro_rules! throwing_stub {
    ($(#[$attribute:meta])* $name:ident => $throw:path) => {
        $(#[$attribute])*
        ///
        /// # Safety
        ///
        /// Called from compiled code only, with the mutator in r15.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push rbp",
                "mov rbp, rsp",
                "sub rsp, 48",
                "mov rcx, r15",
                "mov r8, rbp",
                "mov r9, rsp",
                "call {throw}",
                "mov rax, [rsp + 24]",
                "mov rdx, [rsp + 32]",
                "mov r15, [rsp + 40]",
                "mov rcx, [rsp + 16]",
                "mov rbp, [rsp + 8]",
                "mov rsp, [rsp]",
                "jmp rcx",
                throw = sym $throw,
            )
        }
    };
}

throwing_stub! {
    /// `throw`: takes the tag and the payload of the value thrown, and a
    /// word it does not read.
    baml_rt_throw => unwind::throw
}

throwing_stub! {
    /// Throw a caught value again: takes its tag, its payload, and the
    /// context it was caught with.
    baml_rt_rethrow => unwind::rethrow
}

throwing_stub! {
    /// Throw `baml.panics.DivisionByZero`: takes the dividend.
    baml_rt_panic_division_by_zero => unwind::panic_division_by_zero
}

throwing_stub! {
    /// Throw `baml.panics.NegativeBitShift`: takes the shift count.
    baml_rt_panic_negative_shift => unwind::panic_negative_shift
}

throwing_stub! {
    /// Throw `baml.panics.IndexOutOfBounds`: takes the index and the
    /// length.
    baml_rt_panic_index_out_of_bounds => unwind::panic_index_out_of_bounds
}

throwing_stub! {
    /// Throw `baml.panics.StackOverflow`: a compiled function was entered
    /// with its stack pointer below the limit. Takes words it does not
    /// read.
    baml_rt_panic_stack_overflow => unwind::panic_stack_overflow
}

throwing_stub! {
    /// Throw `baml.panics.MapKeyNotFound`: takes words it does not read.
    baml_rt_panic_map_key_not_found => unwind::panic_map_key_not_found
}
