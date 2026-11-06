//! Low-level PowerPC support.
//!
//! This file is heavily based on the x86_64 implementation.
//! Relevant differences are highlighted in comments, but otherwise most
//! comments have been removed to avoid duplication. Refer to x86_64.rs for
//! detailed comments about what is happening in this file.
//!
//! ## Stack layout
//!
//! Note: "link area" refers to the minimal 32-byte stack frame header that is
//! required by the PowerPC ABI.
//!
//! Here is what the layout of the stack looks like when a coroutine is
//! suspended.
//!
//! ```text
//! +--------------+  <- Stack base
//! | Padding      |  <-
//! +--------------+   |
//! | Initial func |  <-
//! +--------------+   | This mirrors the 32-byte link area layout.
//! | Padding      |  <-
//! +--------------+   |
//! | Parent link  |  <-
//! +--------------+ <- The 4 fields above form a fake link area which
//! |              |    backchains back to the parent stack.
//! ~     ...      ~
//! |              |
//! +--------------+
//! |              |  <- 32-byte link area of the last frame before the suspend.
//! | Link area    |   | The LR field holds the PC to resume execution at.
//! |              |  <- The TOC field holds the R2 to resume execution with.
//! +--------------+  <- Saved stack pointer points here.
//! | Saved R31    |
//! +--------------+
//! | Saved R30    |
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
//! |           |  <- 32-byte link area of the last frame before the suspend.
//! | Link area |   | The LR field holds the PC to resume execution at.
//! |           |  <- The TOC field holds the R2 to resume execution with.
//! +-----------+  <- Parent link points here.
//! | Saved R31 |
//! +-----------+
//! | Saved R30 |
//! +-----------+
//! ```
//!
//! And finally, this is the stack layout of a coroutine that has just been
//! initialized:
//!
//! ```text
//! +--------------+  <- Stack base
//! | Padding      |
//! +--------------+
//! | Initial func |
//! +--------------+
//! | Padding      |
//! +--------------+
//! | Parent link  |
//! +--------------+
//! |              |
//! ~ Initial obj  ~
//! |              |
//! +--------------+
//! | Padding      |  <-
//! +--------------+   |
//! | Initial PC   |  <-
//! +--------------+   | This mirrors the 32-byte link area layout.
//! | Padding      |  <-
//! +--------------+   |
//! | Padding      |  <-
//! +--------------+  <- Initial SP points here
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
pub const PARENT_LINK_OFFSET: usize = 32;
pub type StackWord = u64;

global_asm!(
    ".balign 4",
    asm_function_begin!("stack_init_trampoline"),
    ".cfi_startproc",
    cfi_signal_frame!(),
    // At this point our register state contains the following:
    // - SP points to the top of the parent stack.
    // - LR contains the return address in the parent context.
    // - R30 and R31 contain their values from the parent context.
    // - R5 points to the top of the initial coroutine stack.
    // - R4 points to the base of the initial coroutine stack.
    // - R3 contains the argument passed from switch_and_link.
    //
    // Save the R2, R30, R31 and LR of the parent context to the parent stack.
    "mflr 0",
    "std 0, 16(1)",
    "std 31, -8(1)",
    "std 30, -16(1)",
    // Write the parent stack pointer to the parent link and adjust R4 to point
    // to the parent link.
    "stdu 1, -32(4)",
    // Switch to the coroutine stack.
    "mr 1, 5",
    // Create a fake frame to complete the back-chain to the parent link frame.
    "std 4, 0(1)",
    // The actual meanings of the magic bytes are:
    // 0x0f: DW_CFA_def_cfa_expression
    // 4: byte length of the following DWARF expression
    // 0x71 0x00: DW_OP_breg1 (1 + 0)
    // 0x06: DW_OP_deref
    // 0x06: DW_OP_deref
    //
    // The double deref is used to first get the address of the parent link from
    // the backchain field of our frame and then dereference the parent link to
    // get the actual parent stack pointer value.
    ".cfi_escape 0x0f, 4, 0x71, 0x00, 0x06, 0x06",
    // Now we can tell the unwinder how to restore the registers that were
    // pushed on the parent stack. These are described as offsets from the CFA
    // that we just calculated.
    ".cfi_offset r31, -8",
    ".cfi_offset r30, -16",
    ".cfi_offset lr, 16",
    // Set up the 3rd argument to the initial function to point to the object
    // that init_stack() set up on the stack.
    "addi 5, 1, 32",
    // Calculate the address of stack_init_trampoline_return in a way that works
    // with position-independent code. Since POWER9 doesn't have PC-relative
    // addressing, we use a call instruction instead.
    "bl 2f",
    "2:",
    // We now have the address of "2" in LR. Ideally we would adjust it
    // so that it points to stack_init_trampoline_return but this isn't actually
    // necessary since the initial function never returns. The unwinding
    // information at 2: and stack_init_trampoline_return is identical so we
    // can just leave the return address as it is.
    //
    // As in the original x86_64 code, hand-write the call operation so that it
    // doesn't push an entry into the CPU's return prediction stack. The ABI
    // requires that the function address be in R12.
    "ld 12, 16(4)",
    "mtctr 12",
    "bctr",
    asm_function_alt_entry!("stack_init_trampoline_return"),
    // This 'trap' instruction is necessary because of our use of .cfi_signal_frame earlier.
    "trap",
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
    // - R12 is the function that should be called.
    // - R4 points to the top of our stack.
    // - R3 contains the argument to be passed to the function.
    //
    // Save the LR of the parent context to the parent stack.
    "mflr 0",
    "std 0, 16(1)",
    // Allocate a stack frame on the new stack, saving the parent stack pointer
    // in the back-chain slot.
    "stdu 1, -32(4)",
    // Switch to the new stack.
    "mr 1, 4",
    // The actual meanings of the magic bytes are:
    // 0x0f: DW_CFA_def_cfa_expression
    // 3: byte length of the following DWARF expression
    // 0x71 0x00: DW_OP_breg1 (1 + 0)
    // 0x06: DW_OP_deref
    ".cfi_escape 0x0f, 3, 0x71, 0x00, 0x06",
    ".cfi_offset lr, 16",
    // Call the function pointer. The argument is already in the correct
    // register for the ABI linkage.
    "mtctr 12",
    "bctrl",
    // Switch back to the original stack.
    "ld 1, 0(1)",
    // Load the original LR and return.
    "ld 0, 16(1)",
    "mtlr 0",
    "blr",
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

    // The following 4 slots form a fake 32-byte link area layout.
    push(&mut sp, None);

    // Initial function.
    push(&mut sp, Some(func as StackWord));

    push(&mut sp, None);

    // Placeholder for parent link.
    push(&mut sp, None);

    // Allocate space on the stack for the initial object, rounding to
    // STACK_ALIGNMENT.
    allocate_obj_on_stack(&mut sp, 32, obj);

    // This mirrors the 32-byte link area layout.
    push(&mut sp, None);

    // Set the LR slot with the entry point called by switch_and_link().
    push(
        &mut sp,
        Some(stack_init_trampoline as *const () as StackWord),
    );

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

        // Save R2 in the slot in the parent stack frame. This must be done here
        // to properly support unwinding.
        "std 2, 24(1)",

        // Read the saved PC from the link area of the coroutine stack and call it.
        "ld 0, 16(5)",
        "mtctr 0",
        "bctrl",

        // The unwinder sees this instruction immediately after the call and
        // will restore R2 automatically.
        "ld 2, 24(1)",

        // Upon returning from switch_yield or switch_and_reset, our register
        // state contains the following:
        // - SP: parent stack pointer.
        // - R4: The top of the coroutine stack, or 0 if coming from
        //       switch_and_reset.
        // - R3: The return value from the coroutine.

        // Pass the argument in R3.
        inlateout("3") arg => ret_val,

        // We get the coroutine stack pointer back in R4.
        lateout("4") ret_sp,

        // We pass the stack base in R4.
        in("4") stack_base.get() as u64,

        // We pass the target stack pointer in R5.
        in("5") sp.get() as u64,

        // Mark all registers as clobbered.
        lateout("14") _, lateout("15") _, lateout("16") _, lateout("17") _,
        lateout("18") _, lateout("19") _, lateout("20") _, lateout("21") _,
        lateout("22") _, lateout("23") _, lateout("24") _, lateout("25") _,
        lateout("26") _, lateout("27") _, lateout("28") _, lateout("29") _,
        lateout("f14") _, lateout("f15") _, lateout("f16") _, lateout("f17") _,
        lateout("f18") _, lateout("f19") _, lateout("f20") _, lateout("f21") _,
        lateout("f22") _, lateout("f23") _, lateout("f24") _, lateout("f25") _,
        lateout("f26") _, lateout("f27") _, lateout("f28") _, lateout("f29") _,
        lateout("f30") _, lateout("f31") _,
        lateout("v20") _, lateout("v21") _, lateout("v22") _, lateout("v23") _,
        lateout("v24") _, lateout("v25") _, lateout("v26") _, lateout("v27") _,
        lateout("v28") _, lateout("v29") _, lateout("v30") _, lateout("v31") _,
        lateout("cr2") _, lateout("cr3") _, lateout("cr4") _,
        clobber_abi("C"),
    );

    (ret_val, StackPointer::new(ret_sp))
}

#[inline(always)]
pub unsafe fn switch_yield(arg: EncodedValue, parent_link: *mut StackPointer) -> EncodedValue {
    let ret_val;

    asm_may_unwind_yield!(
        // Save R2, R30 and R31. Ideally this would be done by specifying them
        // as clobbers but that is not possible since they are LLVM reserved
        // registers.
        "std 31, -8(1)",
        "std 30, -16(1)",
        "std 2, 24(1)",

        // Get the return address.
        // FIXME: Workaround for P9 and earlier that do not have pc-rel instructions.
        "bl 1f",
        "1:",
        "mflr 6",
        "addi 6, 6, 0f - 1b",
        // Save return address in the parent frame.
        "std 6, 16(1)",

        // Save our stack pointer to R4.
        "mr 4, 1",

        // Switch to the parent stack from the parent link.
        "ld 1, 0(5)",

        // Restore R2, R30, R31, and PC from the parent stack.
        "ld 31, -8(1)",
        "ld 30, -16(1)",
        "ld 0, 16(1)",
        "mtlr 0",

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
        "blr",

        // This gets called by switch_and_link(). At this point our register
        // state contains the following:
        // - SP points to the top of the parent stack.
        // - LR contains the return address in the parent context.
        // - R2, R30, and R31 contain their value from the parent context.
        // - R5 points to the top of the coroutine stack.
        // - R4 points to the base of our stack.
        // - R3 contains the argument passed from switch_and_link.
        "0:",

        // This must be the first instruction at the return address for the
        // unwinder to properly restore R2.
        "ld 2, 24(1)",

        // Save the R30, R31 and LR values of the parent context to
        // the parent stack.
        "mflr 0",
        "std 31, -8(1)",
        "std 30, -16(1)",
        "std 0, 16(1)",

        // Write the parent stack pointer to the parent link.
        "std 1, -32(4)",

        // Switch to the coroutine stack.
        "mr 1, 5",

        // Load R30 and R31 values from the coroutine stack.
        "ld 31, -8(1)",
        "ld 30, -16(1)",

        // Pass the argument in R3.
        inlateout("3") arg => ret_val,

        // The parent link can be in any register, R5 is arbitrarily chosen
        // here.
        in("5") parent_link as u64,

        // See switch_and_link() for an explanation of the clobbers.
        lateout("14") _, lateout("15") _, lateout("16") _, lateout("17") _,
        lateout("18") _, lateout("19") _, lateout("20") _, lateout("21") _,
        lateout("22") _, lateout("23") _, lateout("24") _, lateout("25") _,
        lateout("26") _, lateout("27") _, lateout("28") _, lateout("29") _,
        lateout("f14") _, lateout("f15") _, lateout("f16") _, lateout("f17") _,
        lateout("f18") _, lateout("f19") _, lateout("f20") _, lateout("f21") _,
        lateout("f22") _, lateout("f23") _, lateout("f24") _, lateout("f25") _,
        lateout("f26") _, lateout("f27") _, lateout("f28") _, lateout("f29") _,
        lateout("f30") _, lateout("f31") _,
        lateout("v20") _, lateout("v21") _, lateout("v22") _, lateout("v23") _,
        lateout("v24") _, lateout("v25") _, lateout("v26") _, lateout("v27") _,
        lateout("v28") _, lateout("v29") _, lateout("v30") _, lateout("v31") _,
        lateout("cr2") _, lateout("cr3") _, lateout("cr4") _,
        clobber_abi("C"),
    );

    ret_val
}

#[inline(always)]
pub unsafe fn switch_and_reset(arg: EncodedValue, parent_link: *mut StackPointer) -> ! {
    // Most of this code is identical to switch_yield(), refer to the
    // comments there. Only the differences are commented.
    asm!(
        // Load the parent context's stack pointer and switch to it.
        "ld 1, 0({parent_link})",

        // Restore R30, R31, and PC from the parent stack.
        "ld 31, -8(1)",
        "ld 30, -16(1)",
        "ld 0, 16(1)",
        "mtlr 0",

        // Return into the parent context
        "blr",

        parent_link = in(reg) parent_link as u64,

        in("3") arg,

        // Hard-code the returned stack pointer value to 0 to indicate that this
        // coroutine is done.
        in("4") 0,

        options(noreturn),
    )
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
        // FIXME: Workaround for P9 and earlier that do not have pc-rel instructions.
        "bl 1f",
        "1:",
        "mflr 6",
        "addi 6, 6, 0f - 1b",

        // Save the R2, R30, R31 and LR of the parent context to the parent stack.
        "std 6, 16(1)",
        "std 31, -8(1)",
        "std 30, -16(1)",
        "std 2, 24(1)",

        // Update the parent link near the base of the coroutine stack.
        "std 1, -32(4)",

        // Switch to the coroutine stack.
        "mr 1, 5",

        // Load the coroutine registers, with the saved LR value into LR.
        "ld 0, 16(1)",
        "mtlr 0",
        "ld 31, -8(1)",
        "ld 30, -16(1)",
        // We don't need to load the TOC pointer here, either `throw` already
        // uses our TOC or the linker will insert a stub to initialize R2.

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
        "ld 2, 24(1)",

        // Helper function to trigger stack unwinding.
        throw = sym throw,

        // Argument to pass to the throw function.
        in("3") forced_unwind.0.get(),

        // Same output registers as switch_and_link().
        lateout("3") ret_val,
        lateout("4") ret_sp,

        // Stack pointer and stack base inputs for stack switching.
        in("4") stack_base.get() as u64,
        in("5") sp.get() as u64,

        // See switch_and_link() for an explanation of the clobbers.
        lateout("14") _, lateout("15") _, lateout("16") _, lateout("17") _,
        lateout("18") _, lateout("19") _, lateout("20") _, lateout("21") _,
        lateout("22") _, lateout("23") _, lateout("24") _, lateout("25") _,
        lateout("26") _, lateout("27") _, lateout("28") _, lateout("29") _,
        lateout("f14") _, lateout("f15") _, lateout("f16") _, lateout("f17") _,
        lateout("f18") _, lateout("f19") _, lateout("f20") _, lateout("f21") _,
        lateout("f22") _, lateout("f23") _, lateout("f24") _, lateout("f25") _,
        lateout("f26") _, lateout("f27") _, lateout("f28") _, lateout("f29") _,
        lateout("f30") _, lateout("f31") _,
        lateout("v20") _, lateout("v21") _, lateout("v22") _, lateout("v23") _,
        lateout("v24") _, lateout("v25") _, lateout("v26") _, lateout("v27") _,
        lateout("v28") _, lateout("v29") _, lateout("v30") _, lateout("v31") _,
        lateout("cr2") _, lateout("cr3") _, lateout("cr4") _,
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
/// # let regs = TrapHandlerRegs { pc: 0, sp: 0, r3: 0, r4: 0, r12: 0, lr: 0 };
/// let TrapHandlerRegs { pc, sp, r3, r4, r12, lr } = regs;
/// ```
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug)]
pub struct TrapHandlerRegs {
    pub pc: u64,
    pub sp: u64,
    pub r3: u64,
    pub r4: u64,
    pub r12: u64,
    pub lr: u64,
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
    allocate_obj_on_stack(&mut sp, 32, val);
    let val_ptr = sp;

    // Create a back-chain pointing to the parent link.
    push(&mut sp, None);
    push(&mut sp, None);
    push(&mut sp, None);
    push(&mut sp, Some(parent_link as StackWord));

    // Set up registers for entry into the function.
    TrapHandlerRegs {
        pc: handler as u64,
        sp: sp as u64,
        r3: val_ptr as u64,
        r4: parent_link as u64,
        r12: handler as u64,
        lr: stack_init_trampoline_return.as_ptr() as u64,
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
        // R2 needs to be saved and restored here because the unwinder
        // specifically looks for the "ld" instruction as a sign to restore R2.
        "std 2, 24(1)",
        concat!("bl ", asm_mangle!("stack_call_trampoline")),
        "ld 2, 24(1)",
        in("3") arg,
        in("4") adjusted_stack_base(&stack).get(),
        in("12") f,
        clobber_abi("C"),
    );
}
