//! Proof that a bytecode frame's stack room is reserved before it runs.
//!
//! The dispatch loop pushes operands without a capacity check. That is sound
//! only because entering a bytecode frame first reserves the frame's locals
//! and its operand bound, which the load-time stack check proved
//! ([`Function::frame_reserve`]). A [`FrameRoom`] is that reservation as a
//! value: every [`BytecodeFrame`] holds one, and only this module can make
//! one, so no path into a frame can skip the reservation and still compile.
//!
//! There are three ways to get one, each checked once per frame entry and
//! never per push:
//!
//! - [`BexVm::reserve_frame_room`] grows the stack within the thread's
//!   budget, or fails with a stack overflow: the general call paths.
//! - [`BexVm::frame_room_within`] finds the room already there and grows
//!   nothing: the dispatch fast path, which hands the call back to the full
//!   dispatch when it is missing.
//! - [`FrameRoom::for_compiled_body`] is for a compiled body's frame, which
//!   runs native code and its resume stub, whose instructions push with
//!   checks. Only [`BytecodeFrame::compiled`] uses it.
//!
//! [`BytecodeFrame`]: super::BytecodeFrame
//! [`BytecodeFrame::compiled`]: super::BytecodeFrame::compiled

use bex_vm_types::{Function, bytecode::CompactCode};

use super::BexVm;
use crate::errors::VmError;

/// The stack room a bytecode frame needs, reserved. Zero-sized: it costs
/// nothing at run time, and its only field is private to this module.
#[derive(Debug)]
pub(crate) struct FrameRoom {
    _reserved: (),
}

impl FrameRoom {
    /// The room of a compiled body's frame. Compiled code checks its own
    /// stack use, and the dispatch loop runs only the body's resume stub,
    /// whose instructions push with checks.
    pub(crate) const fn for_compiled_body() -> Self {
        Self { _reserved: () }
    }

    /// For tests that build frames by hand.
    #[cfg(test)]
    pub(crate) const fn for_test() -> Self {
        Self { _reserved: () }
    }
}

impl BexVm {
    /// Reserve the room a frame of `callee` needs past the values already on
    /// the stack (its arguments): its locals and its operand bound. Fails with
    /// a stack overflow past the thread's stack budget, and for code no
    /// load-time check verified, whose bound no stack can hold. Inlined, so
    /// a call site compiles exactly as the bare reservation did.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    pub(crate) fn reserve_frame_room(&mut self, callee: &Function) -> Result<FrameRoom, VmError> {
        self.reserve_stack(callee.frame_reserve())?;
        Ok(FrameRoom { _reserved: () })
    }

    /// The same room when a stack `len` values long with room for `capacity`
    /// already holds it, without growing anything: the fast path's frame
    /// entry, which hands the call back to the full dispatch on `None`.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    pub(crate) fn frame_room_within(
        capacity: usize,
        len: usize,
        code: &CompactCode,
    ) -> Option<FrameRoom> {
        (capacity - len >= code.frame_slots).then_some(FrameRoom { _reserved: () })
    }
}
