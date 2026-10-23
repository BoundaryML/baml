//! The sole allocation boundary for VM call-frame backing storage.

use std::{
    alloc::{Layout, alloc, realloc},
    ops::{Deref, DerefMut},
};

use super::Frame;
use crate::{VmPanic, vec_ext::VecExt};

/// Per-VM backing-array budget, including spare capacity and inline frame data.
/// Pointees, value-stack storage, allocator overhead, and the native stack are
/// separate. See `FRAME_STORAGE.md` for the accounting and measurements.
pub const FRAME_STORAGE_BUDGET_BYTES: usize = 10 * 1024 * 1024;

/// A frame array whose capacity can only grow through a fallible byte-budget
/// check. Slice access permits inspecting/mutating frames, never growing storage.
#[derive(Default)]
pub struct FrameStack {
    storage: Vec<Frame>,
    #[cfg(test)]
    refuse_next_growth: bool,
}

impl FrameStack {
    pub fn capacity(&self) -> usize {
        self.storage.capacity()
    }

    /// Reserve before constructing a frame or publishing call-setup side effects.
    #[inline]
    pub(super) fn reserve_one(&mut self) -> Result<(), VmPanic> {
        if self.storage.len() < self.storage.capacity() {
            return Ok(());
        }
        #[cfg(test)]
        if std::mem::take(&mut self.refuse_next_growth) {
            // SAFETY: a null result preserves the old allocation.
            return unsafe {
                self.grow_with(FRAME_STORAGE_BUDGET_BYTES, |_, _, _| std::ptr::null_mut())
            };
        }
        // SAFETY: alloc/realloc use the global allocator, preserve the old
        // allocation on null, and return exactly the requested usable layout.
        unsafe {
            self.grow_with(FRAME_STORAGE_BUDGET_BYTES, |ptr, old, new| match old {
                Some(old) => realloc(ptr, old, new.size()),
                None => alloc(new),
            })
        }
    }

    /// # Safety
    /// `allocate` must obey the global alloc/realloc contract: `old` describes
    /// `ptr` when present; on success it transfers the initialized prefix to a
    /// new allocation of `new`'s layout, releasing the old allocation; on null
    /// it leaves the old allocation untouched. It must not unwind after
    /// releasing the old allocation. Used to inject refusal in tests.
    #[cold]
    unsafe fn grow_with(
        &mut self,
        budget: usize,
        allocate: impl FnOnce(*mut u8, Option<Layout>, Layout) -> *mut u8,
    ) -> Result<(), VmPanic> {
        debug_assert_eq!(self.storage.len(), self.storage.capacity());
        let maximum = budget / size_of::<Frame>();
        let needed = self
            .storage
            .len()
            .checked_add(1)
            .ok_or(VmPanic::StackOverflow)?;
        if needed > maximum {
            return Err(VmPanic::StackOverflow);
        }
        let capacity = self
            .storage
            .capacity()
            .saturating_mul(2)
            .max(4)
            .min(maximum);
        let layout = Layout::array::<Frame>(capacity).map_err(|_| VmPanic::StackOverflow)?;
        let old_layout = if self.storage.capacity() == 0 {
            None
        } else {
            Some(
                Layout::array::<Frame>(self.storage.capacity())
                    .map_err(|_| VmPanic::StackOverflow)?,
            )
        };
        let len = self.storage.len();
        let ptr = allocate(self.storage.as_mut_ptr().cast(), old_layout, layout);
        if ptr.is_null() {
            return Err(VmPanic::AllocFailure {
                message: format!(
                    "unable to allocate {} bytes for VM call-frame storage (budget {budget} bytes)",
                    layout.size()
                ),
            });
        }
        // No panicking operations between successful realloc and replacing the
        // old Vec: it no longer owns its allocation. Retain the initialized
        // prefix and let Vec handle dropping frames and deallocating normally.
        std::mem::forget(std::mem::take(&mut self.storage));
        // SAFETY: the allocator contract above supplies the global allocation
        // with Frame's alignment and exactly `capacity * size_of::<Frame>()`
        // usable bytes, containing the old `len` initialized frames. `len` was
        // the old full capacity and `capacity >= needed > len`. Ownership is
        // transferred once; the old vector was forgotten without dropping it.
        self.storage = unsafe { Vec::from_raw_parts(ptr.cast(), len, capacity) };
        Ok(())
    }

    /// Write only into a previously reserved slot. In particular, Vec's
    /// infallible geometric growth must never allocate on a frame insertion.
    #[inline]
    pub(super) fn push_reserved_with(&mut self, frame: impl FnOnce() -> Frame) {
        assert!(
            self.storage.len() < self.storage.capacity(),
            "reserve a frame slot before pushing"
        );
        self.storage.extend(std::iter::once_with(frame));
    }

    pub(super) fn push_reserved(&mut self, frame: Frame) {
        self.push_reserved_with(|| frame);
    }

    pub(super) fn pop(&mut self) -> Option<Frame> {
        self.storage.pop()
    }
}

impl Deref for FrameStack {
    type Target = [Frame];
    fn deref(&self) -> &Self::Target {
        &self.storage
    }
}

impl DerefMut for FrameStack {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.storage
    }
}

impl<'a> IntoIterator for &'a FrameStack {
    type Item = &'a Frame;
    type IntoIter = std::slice::Iter<'a, Frame>;
    fn into_iter(self) -> Self::IntoIter {
        self.storage.iter()
    }
}

impl<'a> IntoIterator for &'a mut FrameStack {
    type Item = &'a mut Frame;
    type IntoIter = std::slice::IterMut<'a, Frame>;
    fn into_iter(self) -> Self::IntoIter {
        self.storage.iter_mut()
    }
}

impl VecExt for FrameStack {
    unsafe fn no_return_pop(&mut self) {
        // SAFETY: the caller guarantees a nonempty frame stack.
        unsafe { self.storage.no_return_pop() };
    }
}

#[cfg(test)]
mod tests {
    use bex_vm_types::{HeapPtr, StackIndex};
    use btel_types::context::Context;

    use super::*;
    use crate::vm::BytecodeFrame;

    fn frame(marker: usize) -> Frame {
        Frame::Bytecode(BytecodeFrame {
            function: HeapPtr::null(),
            instruction_ptr: marker,
            locals_offset: StackIndex::from_raw(0),
            type_args: Vec::new(),
            type_metadata: None,
            faulting_pc: 0,
            telemetry: None,
            context: Context::default(),
        })
    }

    #[test]
    fn fills_the_budget_without_geometric_overshoot_and_reuses_slots() {
        let mut frames = FrameStack::default();
        assert_eq!(frames.capacity(), 0, "shallow tasks do not reserve 10 MiB");
        let maximum = FRAME_STORAGE_BUDGET_BYTES / size_of::<Frame>();
        for marker in 0..maximum {
            frames.reserve_one().unwrap();
            frames.push_reserved_with(|| frame(marker));
            assert!(frames.capacity() * size_of::<Frame>() <= FRAME_STORAGE_BUDGET_BYTES);
        }
        assert_eq!(frames.capacity(), maximum);
        assert_eq!(frames.reserve_one(), Err(VmPanic::StackOverflow));
        assert_eq!(frames.len(), maximum);
        for (marker, frame) in frames.iter().enumerate() {
            let Frame::Bytecode(frame) = frame else {
                unreachable!()
            };
            assert_eq!(
                frame.instruction_ptr, marker,
                "growth preserves live frames"
            );
        }
        drop(frames.pop());
        frames.reserve_one().unwrap();
        frames.push_reserved(frame(42));
        assert_eq!(frames.capacity(), maximum);
        assert_eq!(frames.reserve_one(), Err(VmPanic::StackOverflow));
    }

    #[test]
    fn allocator_refusal_preserves_live_frames_and_is_a_controlled_panic() {
        let mut frames = FrameStack::default();
        frames.reserve_one().unwrap();
        for marker in 0..frames.capacity() {
            frames.push_reserved(frame(marker));
        }
        let original_ptr = frames.as_ptr();
        let original_capacity = frames.capacity();
        // SAFETY: a null result leaves the original allocation untouched.
        let error =
            unsafe { frames.grow_with(FRAME_STORAGE_BUDGET_BYTES, |_, _, _| std::ptr::null_mut()) }
                .unwrap_err();
        assert!(matches!(error, VmPanic::AllocFailure { .. }));
        assert_eq!(frames.as_ptr(), original_ptr);
        assert_eq!(frames.capacity(), original_capacity);
        assert_eq!(frames.len(), original_capacity);
        frames.reserve_one().unwrap();
        frames.push_reserved(frame(42));
    }

    #[test]
    fn a_budget_smaller_than_one_frame_fails_before_allocating() {
        let mut frames = FrameStack::default();
        // SAFETY: this allocator is never called; the budget check must reject.
        let result = unsafe {
            frames.grow_with(size_of::<Frame>() - 1, |_, _, _| {
                panic!("must not allocate")
            })
        };
        assert_eq!(result, Err(VmPanic::StackOverflow));
        assert_eq!(frames.capacity(), 0);
    }
    #[test]
    fn entry_allocation_refusal_surfaces_on_exec_and_can_be_retried() {
        use std::sync::{Arc, atomic::AtomicBool};

        use baml_test_support::compile_source;
        use bex_vm_types::{Object, errors::VmError};

        use crate::{BexVm, VmExecState};

        let program = compile_source("function main() -> int { 42 }");
        let entry = program.rendered_callables()["user.main"].object.raw();
        let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
        let entry = vm.heap.compile_time_ptr(entry);
        vm.frames.refuse_next_growth = true;
        vm.set_entry_point(entry, &[]);
        assert!(vm.frames.is_empty());
        assert_eq!(vm.frames.capacity(), 0);
        assert!(vm.stack.is_empty());
        let Err(VmError::ThrownUnhandled { value, trace }) = vm.exec() else {
            panic!("entry allocation refusal must propagate as a BAML panic")
        };
        assert!(trace.is_empty());
        let Object::Instance(instance) = vm.get_object(value.as_object_ptr().unwrap()) else {
            panic!("expected panic instance")
        };
        let Object::Class(class) = vm.get_object(instance.class) else {
            panic!("expected panic class")
        };
        assert_eq!(class.path.to_string(), "panics.AllocFailure");
        vm.set_entry_point(entry, &[]);
        let Ok(VmExecState::Complete(value)) = vm.exec() else {
            panic!("retry should execute normally")
        };
        assert_eq!(value.as_int(), Some(42));
    }
}
