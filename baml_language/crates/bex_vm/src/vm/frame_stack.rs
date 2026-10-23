//! Safe, fallible backing storage shared by VM frames and evaluation values.

#![forbid(unsafe_code)]

use std::{
    ops::{Deref, DerefMut},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use super::Frame;
use crate::VmPanic;

/// Per-VM budget for allocated frame and evaluation-value backing capacity.
/// Referenced heap objects and the host native stack are separate.
pub const STACK_STORAGE_BUDGET_BYTES: usize = 10 * 1024 * 1024;

/// Accounting shared only by the two backing arrays belonging to one VM.
#[derive(Clone, Default)]
pub(crate) struct StackBudget(Arc<AtomicUsize>);

impl StackBudget {
    /// Charge capacity only if the combined backing arrays still fit.
    fn charge(&self, bytes: usize) -> Result<(), VmPanic> {
        let mut used = self.0.load(Ordering::Relaxed);
        loop {
            let next = used
                .checked_add(bytes)
                .filter(|&n| n <= STACK_STORAGE_BUDGET_BYTES)
                .ok_or(VmPanic::StackOverflow)?;
            match self
                .0
                .compare_exchange_weak(used, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return Ok(()),
                Err(actual) => used = actual,
            }
        }
    }
    /// Reserve additional live elements without exceeding the shared byte budget.
    #[inline]
    pub(crate) fn try_reserve<T>(
        &self,
        storage: &mut Vec<T>,
        additional: usize,
        #[cfg(test)] refuse_next_growth: &mut bool,
    ) -> Result<(), VmPanic> {
        let needed = storage
            .len()
            .checked_add(additional)
            .ok_or(VmPanic::StackOverflow)?;
        if needed <= storage.capacity() {
            return Ok(());
        }
        self.grow(
            storage,
            needed,
            #[cfg(test)]
            refuse_next_growth,
        )
    }

    /// Allocate a replacement safely, validate its actual capacity, then move
    /// live elements. The old array stays intact on allocation/budget failure.
    #[cold]
    fn grow<T>(
        &self,
        storage: &mut Vec<T>,
        needed: usize,
        #[cfg(test)] refuse_next_growth: &mut bool,
    ) -> Result<(), VmPanic> {
        let old_bytes = storage.capacity() * size_of::<T>();
        let used = self.0.load(Ordering::Relaxed);
        let maximum = (STACK_STORAGE_BUDGET_BYTES - used + old_bytes) / size_of::<T>();
        if needed > maximum {
            return Err(VmPanic::StackOverflow);
        }
        let capacity = storage
            .capacity()
            .saturating_mul(2)
            .max(4)
            .max(needed)
            .min(maximum);
        #[cfg(test)]
        if std::mem::take(refuse_next_growth) {
            return Err(VmPanic::AllocFailure {
                message: "injected VM stack allocation refusal".into(),
            });
        }
        let mut replacement = Vec::new();
        replacement
            .try_reserve_exact(capacity)
            .map_err(|error| VmPanic::AllocFailure {
                message: format!("unable to allocate VM stack storage: {error}"),
            })?;
        // try_reserve_exact promises at least the request, so charge the
        // observed capacity and reject any over-allocation before installation.
        let new_bytes = replacement
            .capacity()
            .checked_mul(size_of::<T>())
            .ok_or(VmPanic::StackOverflow)?;
        self.charge(new_bytes - old_bytes)?;
        replacement.append(storage); // capacity already covers len
        *storage = replacement;
        Ok(())
    }

    /// Release the charge when a backing array is dropped.
    pub(crate) fn release(&self, bytes: usize) {
        self.0.fetch_sub(bytes, Ordering::Relaxed);
    }
}

/// A vector that exposes slice mutation, but never infallible capacity growth.
/// Both kinds of VM stack use the same fallible reservation boundary.
pub struct StackStorage<T> {
    storage: Vec<T>,
    budget: StackBudget,
    #[cfg(test)]
    pub(super) refuse_next_growth: bool,
}

/// Both bytecode and native continuations occupy a full enum slot.
pub type FrameStack = StackStorage<Frame>;

impl<T> Default for StackStorage<T> {
    fn default() -> Self {
        Self::new(StackBudget::default())
    }
}

impl<T> StackStorage<T> {
    /// Construct one backing array sharing this VM's budget.
    pub(crate) fn new(budget: StackBudget) -> Self {
        assert_ne!(size_of::<T>(), 0, "VM stack records must have a byte size");
        Self {
            storage: Vec::new(),
            budget,
            #[cfg(test)]
            refuse_next_growth: false,
        }
    }

    /// Allocated slots, including unused slots charged to the shared budget.
    pub fn capacity(&self) -> usize {
        self.storage.capacity()
    }

    /// Reserve before publishing call-setup side effects.
    #[inline]
    pub(crate) fn reserve_one(&mut self) -> Result<(), VmPanic> {
        self.try_reserve(1)
    }

    /// Reserve live slots through the shared allocation boundary.
    #[inline]
    pub(crate) fn try_reserve(&mut self, additional: usize) -> Result<(), VmPanic> {
        self.budget.try_reserve(
            &mut self.storage,
            additional,
            #[cfg(test)]
            &mut self.refuse_next_growth,
        )
    }

    /// Insert into pre-reserved capacity without allocating.
    #[inline]
    pub(crate) fn push_reserved_with(&mut self, value: impl FnOnce() -> T) {
        assert!(
            self.len() < self.capacity(),
            "reserve a VM stack slot before pushing"
        );
        self.storage.push(value());
    }

    /// Insert a frame into a slot already reserved by call setup.
    pub(crate) fn push_reserved(&mut self, value: T) {
        self.push_reserved_with(|| value);
    }

    /// Fallibly push a value, charging any capacity growth to the shared budget.
    #[inline]
    pub fn try_push(&mut self, value: T) -> Result<(), VmPanic> {
        self.reserve_one()?;
        self.storage.push(value);
        Ok(())
    }

    /// Resize live storage, reserving fallibly before initializing new slots.
    #[cfg(test)]
    pub(crate) fn try_resize(&mut self, len: usize, value: T) -> Result<(), VmPanic>
    where
        T: Clone,
    {
        self.try_reserve(len.saturating_sub(self.len()))?;
        self.storage.resize(len, value);
        Ok(())
    }

    /// Remove a live element while retaining charged backing capacity.
    pub fn pop(&mut self) -> Option<T> {
        self.storage.pop()
    }

    /// Remove trailing elements while retaining charged backing capacity.
    pub fn truncate(&mut self, len: usize) {
        self.storage.truncate(len);
    }
}

impl<T> Drop for StackStorage<T> {
    /// Release the backing-capacity charge when the owning array is dropped.
    fn drop(&mut self) {
        self.budget.release(self.capacity() * size_of::<T>());
    }
}

impl<T> Deref for StackStorage<T> {
    type Target = [T];
    /// Expose initialized elements without permitting unaccounted vector growth.
    fn deref(&self) -> &Self::Target {
        &self.storage
    }
}
impl<T> DerefMut for StackStorage<T> {
    /// Mutate initialized elements without permitting vector growth.
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.storage
    }
}
impl<'a, T> IntoIterator for &'a StackStorage<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    /// Iterate over initialized elements.
    fn into_iter(self) -> Self::IntoIter {
        self.storage.iter()
    }
}
impl<'a, T> IntoIterator for &'a mut StackStorage<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    /// Mutably iterate over initialized elements.
    fn into_iter(self) -> Self::IntoIter {
        self.storage.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_and_values_share_one_budget_and_preserve_live_data() {
        let budget = StackBudget::default();
        let mut records = StackStorage::<[u8; 112]>::new(budget.clone());
        let mut values = StackStorage::<[u8; 16]>::new(budget.clone());
        records.try_push([42; 112]).unwrap();
        values
            .try_resize(STACK_STORAGE_BUDGET_BYTES / 32, [7; 16])
            .unwrap();
        while records.reserve_one().is_ok() {
            records.push_reserved([42; 112]);
        }
        assert!(budget.0.load(Ordering::Relaxed) <= STACK_STORAGE_BUDGET_BYTES);
        assert!(records.len() > 256);
        assert!(records.iter().all(|record| *record == [42; 112]));
        assert!(values.iter().all(|value| *value == [7; 16]));
        assert_eq!(values.try_reserve(usize::MAX), Err(VmPanic::StackOverflow));
        let used = budget.0.load(Ordering::Relaxed);
        records.pop();
        records.try_push([1; 112]).unwrap();
        assert_eq!(budget.0.load(Ordering::Relaxed), used);
        drop(records);
        assert_eq!(budget.0.load(Ordering::Relaxed), values.capacity() * 16);
        drop(values);
        assert_eq!(budget.0.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn allocator_refusal_preserves_contents_capacity_and_charge() {
        let mut storage = StackStorage::<usize>::default();
        storage.try_resize(4, 42).unwrap();
        let capacity = storage.capacity();
        let ptr = storage.as_ptr();
        storage.refuse_next_growth = true;
        assert!(matches!(
            storage.try_push(7),
            Err(VmPanic::AllocFailure { .. })
        ));
        assert_eq!(&*storage, &[42; 4]);
        assert_eq!(storage.as_ptr(), ptr);
        assert_eq!(storage.capacity(), capacity);
        assert_eq!(
            storage.budget.0.load(Ordering::Relaxed),
            capacity * size_of::<usize>()
        );
        storage.try_push(7).unwrap();
        assert_eq!(storage.last(), Some(&7));
    }

    #[test]
    fn budget_refusal_precedes_allocation_and_preserves_live_elements() {
        let mut storage = StackStorage::<usize>::default();
        storage.try_push(42).unwrap();
        storage.refuse_next_growth = true;
        assert_eq!(
            storage.try_reserve(STACK_STORAGE_BUDGET_BYTES),
            Err(VmPanic::StackOverflow)
        );
        assert!(storage.refuse_next_growth);
        assert_eq!(&*storage, &[42]);
    }
    #[test]
    /// Entry allocation refusal must become a BAML panic rather than an empty run.
    fn entry_allocation_refusal_surfaces_on_exec_and_can_be_retried() {
        use std::sync::{Arc, atomic::AtomicBool};

        use baml_test_support::compile_source;
        use bex_vm_types::{Object, errors::VmError};

        use crate::{BexVm, VmExecState};

        for refuse_frames in [true, false] {
            let program = compile_source("function main(n: int) -> int { n }");
            let entry = program.rendered_callables()["user.main"].object.raw();
            let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
            let entry = vm.heap.compile_time_ptr(entry);
            if refuse_frames {
                vm.frames.refuse_next_growth = true;
            } else {
                vm.stack.refuse_next_growth = true;
            }
            vm.set_entry_point(entry, &[bex_vm_types::Value::int(42)]);
            assert!(vm.frames.is_empty());
            if refuse_frames {
                assert_eq!(vm.frames.capacity(), 0);
            }
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
            vm.set_entry_point(entry, &[bex_vm_types::Value::int(42)]);
            let Ok(VmExecState::Complete(value)) = vm.exec() else {
                panic!("retry should execute normally")
            };
            assert_eq!(value.as_int(), Some(42));
        }
    }
    #[test]
    fn host_result_allocation_refusal_surfaces_on_resume() {
        use std::sync::{Arc, atomic::AtomicBool};

        use baml_test_support::compile_source;
        use bex_vm_types::{Object, Value, errors::VmError};

        use crate::BexVm;

        let program = compile_source("function main() -> int { 42 }");
        let entry = program.rendered_callables()["user.main"].object.raw();
        let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
        vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
        vm.stack.refuse_next_growth = true;
        vm.stack.push(Value::NULL);
        assert!(vm.stack.is_empty());
        let Err(VmError::ThrownUnhandled { value, .. }) = vm.exec() else {
            panic!("host-result allocation refusal must propagate as a BAML panic")
        };
        let Object::Instance(instance) = vm.get_object(value.as_object_ptr().unwrap()) else {
            panic!("expected panic instance")
        };
        let Object::Class(class) = vm.get_object(instance.class) else {
            panic!("expected panic class")
        };
        assert_eq!(class.path.to_string(), "panics.AllocFailure");
    }
}
