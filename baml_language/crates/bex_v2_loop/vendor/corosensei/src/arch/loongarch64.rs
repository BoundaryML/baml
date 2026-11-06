//! Low-level LoongArch64 support.
//!
//! This file is heavily based on the x86_64 implementation.
//! Relevant differences are highlighted in comments, but otherwise most
//! comments have been removed to avoid duplication. Refer to x86_64.rs for
//! detailed comments about what is happening in this file.
//!
//! ## Stack layout
//!
//! Here is what the layout of the stack looks like when a coroutine is
//! suspended.
//!
//! ```text
//! +--------------+  <- Stack base
//! | Initial func |
//! +--------------+
//! | Parent link  |
//! +--------------+
//! |              |
//! ~     ...      ~
//! |              |
//! +--------------+
//! | Padding      |
//! +--------------+
//! | Saved PC     |
//! +--------------+
//! | Saved FP     |
//! +--------------+
//! | Saved S8     |
//! +--------------+
//! ```
//!
//! And this is the layout of the parent stack when a coroutine is running:
//!
//! ```text
//! |           |
//! ~    ...    ~
//! |           |
//! +-----------+
//! | Padding   |
//! +-----------+
//! | Saved S8  |
//! +-----------+  <- The parent link points to here instead of pointing to the
//! | Saved PC  |     top of the stack. This matches the GCC/LLVM behavior of
//! +-----------+     having the frame pointer point to the address above the
//! | Saved FP  |     saved RA/FP.
//! +-----------+
//! ```
//!
//! And finally, this is the stack layout of a coroutine that has just been
//! initialized:
//!
//! ```text
//! +--------------+  <- Stack base
//! | Initial func |
//! +--------------+
//! | Parent link  |
//! +--------------+
//! |              |
//! ~ Initial obj  ~
//! |              |
//! +--------------+
//! | Padding      |
//! +--------------+
//! | Initial PC   |
//! +--------------+
//! | Padding      |
//! +--------------+
//! | Padding      |
//! +--------------+  <- Initial stack pointer
//! ```

use core::arch::{asm, global_asm};

use super::{allocate_obj_on_stack, push};
use crate::coroutine::adjusted_stack_base;
use crate::stack::{Stack, StackPointer};
use crate::unwind::{
    asm_may_unwind_root, asm_may_unwind_yield, cfi_reset_args_size_root, cfi_reset_args_size_yield,
    InitialFunc, StackCallFunc, TrapHandler,
};
use crate::util::EncodedValue;

pub const STACK_ALIGNMENT: usize = 16;
pub const PARENT_STACK_OFFSET: usize = 16;
pub const PARENT_LINK_OFFSET: usize = 16;
pub type StackWord = u64;

global_asm!(
    ".balign 4",
    asm_function_begin!("stack_init_trampoline"),
    ".cfi_startproc",
    cfi_signal_frame!(),
    // At this point our register state contains the following:
    // - SP points to the top of the parent stack.
    // - RA contains the return address in the parent context.
    // - S8 and FP contain their value from the parent context.
    // - A2 points to the top of the coroutine stack.
    // - A1 points to the base of our stack.
    // - A0 contains the argument passed from switch_and_link.
    //
    // Push the S8, FP and PC values of the parent context onto the parent
    // stack.
    "addi.d $sp, $sp, -32",
    "st.d $s8, $sp, 16",
    "st.d $ra, $sp, 8",
    "st.d $fp, $sp, 0",
    // Write the parent stack pointer to the parent link. This is adjusted to
    // point just above the saved PC/RA to match the GCC/LLVM ABI.
    "addi.d $t0, $sp, 16",
    "st.d $t0, $a1, -16",
    // Set up the frame pointer to point at the parent link. This is needed for
    // the unwinding code below.
    "move $fp, $a1",
    // Adjust A1 to point to the parent link.
    "addi.d $a1, $a1, -16",
    // Pop the padding and initial PC from the coroutine stack. This also sets
    // up the 3rd argument to the initial function to point to the object that
    // init_stack() set up on the stack.
    "addi.d $a2, $a2, 32",
    // Switch to the coroutine stack.
    "move $sp, $a2",
    // The actual meanings of the magic bytes are:
    // 0x0f: DW_CFA_def_cfa_expression
    // 5: byte length of the following DWARF expression
    // 0x86 0x70: DW_OP_breg22 (fp - 16)
    // 0x06: DW_OP_deref
    // 0x23, 0x10: DW_OP_plus_uconst 16
    ".cfi_escape 0x0f, 5, 0x86, 0x70, 0x06, 0x23, 0x10",
    // Now we can tell the unwinder how to restore the 3 registers that were
    // pushed on the parent stack. These are described as offsets from the CFA
    // that we just calculated.
    ".cfi_offset 31, -16",
    ".cfi_offset 1, -24",
    ".cfi_offset 22, -32",
    // As in the original x86_64 code, hand-write the call operation so that it
    // doesn't push an entry into the CPU's return prediction stack.
    "la.pcrel $ra, 0f",
    "ld.d $t0, $a1, 8",
    "jr $t0",
    // Use a local label because stack_init_trampoline_return is a global
    // symbol, which can cause issues with relocations.
    "0:",
    asm_function_alt_entry!("stack_init_trampoline_return"),
    // This BREAK is necessary because of our use of .cfi_signal_frame earlier.
    "break 0",
    ".cfi_endproc",
    asm_function_end!("stack_init_trampoline"),
);

global_asm!(
    // See stack_init_trampoline for an explanation of the assembler directives
    // used here.
    ".balign 4",
    asm_function_begin!("stack_call_trampoline"),
    ".cfi_startproc",
    cfi_signal_frame!(),
    // At this point our register state contains the following:
    // - SP points to the top of the parent stack.
    // - FP holds its value from the parent context.
    // - A2 is the function that should be called.
    // - A1 points to the top of our stack.
    // - A0 contains the argument to be passed to the function.
    //
    // Create a stack frame and point the frame pointer at it.
    "addi.d $sp, $sp, -16",
    ".cfi_def_cfa_offset 16",
    "st.d $ra, $sp, 8",
    "st.d $fp, $sp, 0",
    "addi.d $fp, $sp, 16",
    ".cfi_def_cfa 22, 0",
    ".cfi_offset 1, -8",
    ".cfi_offset 22, -16",
    // Switch to the new stack.
    "move $sp, $a1",
    // Call the function pointer. The argument is already in the correct
    // register for the function.
    "jirl $ra, $a2, 0",
    // Switch back to the original stack by restoring from the frame pointer,
    // then return.
    "addi.d $sp, $fp, -16",
    "ld.d $ra, $sp, 8",
    "ld.d $fp, $sp, 0",
    "addi.d $sp, $sp, 16",
    "jr $ra",
    ".cfi_endproc",
    asm_function_end!("stack_call_trampoline"),
);

// These trampolines use a custom calling convention and should only be called
// with inline assembly.
extern "C" {
    fn stack_init_trampoline(arg: EncodedValue, stack_base: StackPointer, stack_ptr: StackPointer);
    static stack_init_trampoline_return: [u8; 0];
    #[allow(dead_code)]
    fn stack_call_trampoline(arg: *mut u8, sp: StackPointer, f: StackCallFunc);
}

#[inline]
pub unsafe fn init_stack<T>(stack: &impl Stack, func: InitialFunc<T>, obj: T) -> StackPointer {
    let mut sp = adjusted_stack_base(stack).get();

    // Initial function.
    push(&mut sp, Some(func as StackWord));

    // Placeholder for parent link.
    push(&mut sp, None);

    // Allocate space on the stack for the initial object, rounding to
    // STACK_ALIGNMENT.
    allocate_obj_on_stack(&mut sp, 16, obj);

    // The stack is aligned to STACK_ALIGNMENT at this point.
    debug_assert_eq!(sp % STACK_ALIGNMENT, 0);

    // Padding so the final stack pointer value is properly aligned.
    push(&mut sp, None);

    // Entry point called by switch_and_link().
    push(
        &mut sp,
        Some(stack_init_trampoline as *const () as StackWord),
    );

    // Add a 16-byte offset because switch_and_link() looks for the target PC
    // 16 bytes above the stack pointer.
    push(&mut sp, None);
    push(&mut sp, None);

    StackPointer::new_unchecked(sp)
}

#[inline]
pub unsafe fn switch_and_link(
    arg: EncodedValue,
    sp: StackPointer,
    stack_base: StackPointer,
) -> (EncodedValue, Option<StackPointer>) {
    let (ret_val, ret_sp);

    asm_may_unwind_root!(
        // DW_CFA_GNU_args_size 0
        //
        // Indicate to the unwinder that this "call" does not take any arguments
        // and no stack space needs to be popped before executing a landing pad.
        // This is mainly here to undo the effect of any previous
        // DW_CFA_GNU_args_size that may have been set in the current function.
        cfi_reset_args_size_root!(),

        // Read the saved PC from the coroutine stack and call it.
        "ld.d $t0, $a2, 16",
        "jirl $ra, $t0, 0",

        // Upon returning, our register state contains the following:
        // - A2: Our stack pointer.
        // - A1: The top of the coroutine stack, or 0 if coming from
        //       switch_and_reset.
        // - A0: The argument passed from the coroutine.

        // Switch back to our stack and free the saved registers.
        "addi.d $sp, $a2, 16",

        // Pass the argument in A0.
        inlateout("$a0") arg => ret_val,

        // We get the coroutine stack pointer back in A1.
        lateout("$a1") ret_sp,

        // We pass the stack base in A1.
        in("$a1") stack_base.get() as u64,

        // We pass the target stack pointer in A2.
        in("$a2") sp.get() as u64,

        // Mark all registers as clobbered.
        lateout("$s0") _, lateout("$s1") _, lateout("$s2") _, lateout("$s3") _,
        lateout("$s4") _, lateout("$s5") _, lateout("$s6") _, lateout("$s7") _,
        lateout("$fs0") _, lateout("$fs1") _, lateout("$fs2") _, lateout("$fs3") _,
        lateout("$fs4") _, lateout("$fs5") _, lateout("$fs6") _, lateout("$fs7") _,
        clobber_abi("C"),
    );

    (ret_val, StackPointer::new(ret_sp))
}

#[inline(always)]
pub unsafe fn switch_yield(arg: EncodedValue, parent_link: *mut StackPointer) -> EncodedValue {
    let ret_val;

    asm_may_unwind_yield!(
        // Save S8 and FP while also reserving space on the stack for our
        // saved PC.
        "addi.d $sp, $sp, -32",
        "st.d $fp, $sp, 0",
        "st.d $s8, $sp, 8",

        // Write our return address to its expected position on the stack.
        "la.pcrel $ra, 0f",
        "st.d $ra, $sp, 16",

        // Get the parent stack pointer from the parent link.
        "ld.d $a2, $t0, 0",

        // Save our stack pointer to A1.
        "move $a1, $sp",

        // Restore S8, FP and RA from the parent stack.
        "ld.d $s8, $a2, 0",
        "ld.d $ra, $a2, -8",
        "ld.d $fp, $a2, -16",

        // DW_CFA_GNU_args_size 0
        //
        // Indicate to the unwinder that this "call" does not take any arguments
        // and no stack space needs to be popped before executing a landing pad.
        // This is mainly here to undo the effect of any previous
        // DW_CFA_GNU_args_size that may have been set in the current function.
        //
        // This is needed here even though we don't call anything because
        // switch_and_throw may inject a call which returns to this point.
        cfi_reset_args_size_yield!(),

        // Return into the parent context
        "jr $ra",

        // This gets called by switch_and_link(). At this point our register
        // state contains the following:
        // - SP points to the top of the parent stack.
        // - RA contains the return address in the parent context.
        // - S8 and FP contain their value from the parent context.
        // - A2 points to the top of the coroutine stack.
        // - A1 points to the base of our stack.
        // - A0 contains the argument passed from switch_and_link.
        "0:",

        // Push the S8, FP and PC values of the parent context onto the parent
        // stack.
        "addi.d $sp, $sp, -32",
        "st.d $s8, $sp, 16",
        "st.d $ra, $sp, 8",
        "st.d $fp, $sp, 0",

        // Write the parent stack pointer to the parent link. This is adjusted
        // to point just above the saved PC/RA to match the GCC/LLVM ABI.
        "addi.d $t0, $sp, 16",
        "st.d $t0, $a1, -16",

        // Load our S8 and FP values from the coroutine stack.
        "ld.d $s8, $a2, 8",
        "ld.d $fp, $a2, 0",

        // Switch to the coroutine stack while popping the saved registers and
        // padding.
        "addi.d $sp, $a2, 32",

        // Pass the argument in A0.
        inlateout("$a0") arg => ret_val,

        // The parent link can be in any register, T0 is arbitrarily chosen
        // here.
        in("$t0") parent_link as u64,

        // See switch_and_link() for an explanation of the clobbers.
        lateout("$s0") _, lateout("$s1") _, lateout("$s2") _, lateout("$s3") _,
        lateout("$s4") _, lateout("$s5") _, lateout("$s6") _, lateout("$s7") _,
        lateout("$fs0") _, lateout("$fs1") _, lateout("$fs2") _, lateout("$fs3") _,
        lateout("$fs4") _, lateout("$fs5") _, lateout("$fs6") _, lateout("$fs7") _,
        clobber_abi("C"),
    );

    ret_val
}

#[inline(always)]
pub unsafe fn switch_and_reset(arg: EncodedValue, parent_link: *mut StackPointer) -> ! {
    // Most of this code is identical to switch_yield(), refer to the
    // comments there. Only the differences are commented.
    asm!(
        // Load the parent context's stack pointer.
        "ld.d $a2, {parent_link}, 0",

        // Restore S8, FP and RA from the parent stack.
        "ld.d $s8, $a2, 0",
        "ld.d $ra, $a2, -8",
        "ld.d $fp, $a2, -16",

        // Return into the parent context
        "jr $ra",

        parent_link = in(reg) parent_link as u64,

        in("$a0") arg,

        // Hard-code the returned stack pointer value to 0 to indicate that this
        // coroutine is done.
        in("$a1") 0,

        options(noreturn),
    );
}

#[inline]
#[cfg(feature = "asm-unwind")]
pub unsafe fn switch_and_throw(
    forced_unwind: crate::unwind::ForcedUnwind,
    sp: StackPointer,
    stack_base: StackPointer,
) -> (EncodedValue, Option<StackPointer>) {
    extern "C-unwind" fn throw(forced_unwind: crate::unwind::ForcedUnwind) -> ! {
        extern crate std;
        use std::boxed::Box;
        std::panic::resume_unwind(Box::new(forced_unwind));
    }

    let (ret_val, ret_sp);

    asm_may_unwind_root!(
        // Set up a return address.
        "la.pcrel $ra, 0f",

        // Save the registers of the parent context.
        "addi.d $sp, $sp, -32"
        "st.d $s8, $sp, 16"
        "st.d $ra, $sp, 8"
        "st.d $fp, $sp, 0"

        // Update the parent link near the base of the coroutine stack.
        "addi.d $t1, $sp, 16"
        "st.d $t1, $a1, -16"

        // Load the coroutine registers, with the saved PC into RA.
        "ld.d $ra, $t0, 16"
        "ld.d $s8, $t0, 8"
        "ld.d $fp, $t0, 0"

        // Switch to the coroutine stack while popping the saved registers and
        // padding.
        "addi.d $sp, $t0, 32",

        // DW_CFA_GNU_args_size 0
        //
        // Indicate to the unwinder that this "call" does not take any arguments
        // and no stack space needs to be popped before executing a landing pad.
        // This is mainly here to undo the effect of any previous
        // DW_CFA_GNU_args_size that may have been set in the current function.
        cfi_reset_args_size_root!(),

        // Simulate a call with an artificial return address so that the throw
        // function will unwind straight into the switch_and_yield() call with
        // the register state expected outside the asm! block.
        "b {throw}",

        // Upon returning, our register state is just like a normal return into
        // switch_and_link().
        "0:",

        // Switch back to our stack and free the saved registers.
        "addi.d $sp, $a2, 32",

        // Helper function to trigger stack unwinding.
        throw = sym throw,

        // Argument to pass to the throw function.
        in("$a0") forced_unwind.0.get(),

        // Same output registers as switch_and_link().
        lateout("$a0") ret_val,
        lateout("$a1") ret_sp,

        // Stack pointer and stack base inputs for stack switching.
        in("$a1") stack_base.get() as u64,
        in("$t0") sp.get() as u64,

        // See switch_and_link() for an explanation of the clobbers.
        lateout("$s0") _, lateout("$s1") _, lateout("$s2") _, lateout("$s3") _,
        lateout("$s4") _, lateout("$s5") _, lateout("$s6") _, lateout("$s7") _,
        lateout("$fs0") _, lateout("$fs1") _, lateout("$fs2") _, lateout("$fs3") _,
        lateout("$fs4") _, lateout("$fs5") _, lateout("$fs6") _, lateout("$fs7") _,
        clobber_abi("C"),
    );

    (ret_val, StackPointer::new(ret_sp))
}

#[inline]
pub unsafe fn drop_initial_obj(
    _stack_base: StackPointer,
    stack_ptr: StackPointer,
    drop_fn: unsafe fn(ptr: *mut u8),
) {
    let ptr = (stack_ptr.get() as *mut u8).add(32);
    drop_fn(ptr);
}

/// Registers which must be updated upon return from a trap handler.
///
/// The exact set of registers that need to be updated varies depending on the
/// target. Note that *all* registers must be updated to the specified values,
/// otherwise behavior is undefined.
///
/// To catch any issues at compilation time, it is recommended to use Rust's
/// pattern matching syntax to extract the individual registers from this
/// struct.
///
/// ```
/// # use corosensei::trap::TrapHandlerRegs;
/// # let regs = TrapHandlerRegs { pc: 0, sp: 0, a0: 0, a1: 0, fp: 0, ra: 0 };
/// let TrapHandlerRegs { pc, sp, a0, a1, fp, ra } = regs;
/// ```
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug)]
pub struct TrapHandlerRegs {
    pub pc: u64,
    pub sp: u64,
    pub a0: u64,
    pub a1: u64,
    pub fp: u64,
    pub ra: u64,
}

pub unsafe fn setup_trap_trampoline<T>(
    stack_base: StackPointer,
    val: T,
    handler: TrapHandler<T>,
) -> TrapHandlerRegs {
    // Preserve the top 16 bytes of the stack since they contain the parent
    // link.
    let parent_link = stack_base.get() - PARENT_LINK_OFFSET;

    // Everything below this can be overwritten. Write the object to the stack.
    let mut sp = parent_link;
    allocate_obj_on_stack(&mut sp, 16, val);
    let val_ptr = sp;

    // Set up registers for entry into the function.
    TrapHandlerRegs {
        pc: handler as u64,
        sp: sp as u64,
        a0: val_ptr as u64,
        a1: parent_link as u64,
        fp: stack_base.get() as u64,
        ra: stack_init_trampoline_return.as_ptr() as u64,
    }
}

/// This function executes a function on the given stack. The argument is passed
/// through to the called function.
#[inline]
pub unsafe fn on_stack(arg: *mut u8, stack: impl Stack, f: StackCallFunc) {
    // This is a bit subtle: because we use .cfi_signal_frame in the trampoline,
    // the unwinder will look for unwinding information at the instruction
    // after the return address. Normal compiler code generation does not
    // expect this and may generate incorrect entries in the exception handling
    // table. We work around this by adding a NOP instruction after the call.
    asm_may_unwind_root!(
        // DW_CFA_GNU_args_size 0
        cfi_reset_args_size_root!(),
        concat!("bl ", asm_mangle!("stack_call_trampoline")),
        "nop",
        in("$a0") arg,
        in("$a1") adjusted_stack_base(&stack).get(),
        in("$a2") f,
        clobber_abi("C"),
    );
}
