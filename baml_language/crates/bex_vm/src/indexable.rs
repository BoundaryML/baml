use std::ops::{Bound, Deref, DerefMut, Index, IndexMut, RangeBounds};

use bex_vm_types::{StackIndex, Value};

use crate::{VmPanic, vec_ext::CopyVecExt, vm::frame_stack::StackBudget};

/// Argument, local, and operand slots charged to the VM's shared stack budget.
#[derive(Default)]
pub struct EvalStack {
    values: Vec<Value>,
    pending_panic: Option<VmPanic>,
    budget: StackBudget,
    #[cfg(test)]
    pub(crate) refuse_next_growth: bool,
}

impl EvalStack {
    /// Create value storage sharing the call-frame storage budget.
    pub(crate) fn new(budget: StackBudget) -> Self {
        Self {
            values: Vec::new(),
            pending_panic: None,
            budget,
            #[cfg(test)]
            refuse_next_growth: false,
        }
    }

    /// Allocated value slots, including unused reserved capacity.
    pub fn capacity(&self) -> usize {
        self.values.capacity()
    }

    /// Push one value with fallible, budgeted backing growth.
    pub fn try_push(&mut self, value: Value) -> Result<(), VmPanic> {
        self.try_reserve(1)?;
        self.values.push(value);
        Ok(())
    }

    /// Accept a host operation's result between exec calls. Allocation failure
    /// is delivered through the normal unwinder when execution next resumes.
    pub fn push(&mut self, value: Value) {
        if self.pending_panic.is_none() {
            self.pending_panic = self.try_push(value).err();
        }
    }

    /// Take a deferred host-result allocation failure on execution entry.
    pub(crate) fn take_pending_panic(&mut self) -> Option<VmPanic> {
        self.pending_panic.take()
    }

    /// Extend an exact-sized sequence after reserving the whole range.
    pub(crate) fn try_extend(
        &mut self,
        values: impl IntoIterator<Item = Value, IntoIter: ExactSizeIterator>,
    ) -> Result<(), VmPanic> {
        let values = values.into_iter();
        self.try_reserve(values.len())?;
        for value in values {
            self.try_push(value)?;
        }
        Ok(())
    }

    /// Initialize real-local slots after fallibly reserving their backing array.
    pub(crate) fn try_resize(&mut self, len: usize, value: Value) -> Result<(), VmPanic> {
        self.try_reserve(len.saturating_sub(self.len()))?;
        self.values.resize(len, value);
        Ok(())
    }

    /// Reserve value slots through the shared frame/value allocation boundary.
    #[inline]
    pub(crate) fn try_reserve(&mut self, additional: usize) -> Result<(), VmPanic> {
        self.budget.try_reserve(
            &mut self.values,
            additional,
            #[cfg(test)]
            &mut self.refuse_next_growth,
        )
    }

    /// Insert a receiver after fallibly reserving its backing slot.
    pub(crate) fn try_insert(&mut self, index: usize, value: Value) -> Result<(), VmPanic> {
        self.try_reserve(1)?;
        self.values.insert(index, value);
        Ok(())
    }

    /// Borrow initialized values without exposing unaccounted Vec growth.
    pub(crate) fn as_slice(&self) -> &[Value] {
        &self.values
    }

    /// Drop trailing live values, keeping their charged capacity for reuse.
    pub fn truncate(&mut self, len: usize) {
        self.values.truncate(len);
    }

    /// Remove a typed range of live slots, retaining backing capacity.
    pub(crate) fn drain(
        &mut self,
        range: impl RangeBounds<StackIndex>,
    ) -> std::vec::Drain<'_, Value> {
        self.values.drain(raw_bounds(range))
    }
}

impl Drop for EvalStack {
    /// Release the value backing-capacity charge when this owner is dropped.
    fn drop(&mut self) {
        self.budget
            .release(self.values.capacity() * size_of::<Value>());
    }
}

#[allow(unsafe_code, clippy::inline_always)]
impl CopyVecExt<Value> for EvalStack {
    /// Preserve the existing unchecked operand replacement; no storage grows.
    #[inline(always)]
    unsafe fn replace_top_n<const N: usize>(&mut self, value: Value) {
        // SAFETY: the caller supplies the existing CopyVecExt slot contract.
        unsafe { self.values.replace_top_n::<N>(value) };
    }

    /// Preserve the existing unchecked return/operand transition.
    #[inline(always)]
    unsafe fn replace_top_n_dynamic(&mut self, n: usize, value: Value) {
        // SAFETY: the caller supplies the existing CopyVecExt slot contract.
        unsafe { self.values.replace_top_n_dynamic(n, value) };
    }

    /// Discard consumed values without allocation or a new bounds check.
    #[inline(always)]
    unsafe fn truncate_suffix(&mut self, n: usize) {
        // SAFETY: the caller guarantees n <= len, as before this budget change.
        unsafe { self.values.truncate_suffix(n) };
    }
}

/// Convert typed stack-index bounds while retaining inclusive/exclusive ends.
fn raw_bounds(range: impl RangeBounds<StackIndex>) -> (Bound<usize>, Bound<usize>) {
    (
        range.start_bound().map(|index| index.raw()),
        range.end_bound().map(|index| index.raw()),
    )
}

impl Deref for EvalStack {
    type Target = [Value];
    /// Borrow live slots without exposing unaccounted Vec growth.
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl DerefMut for EvalStack {
    /// Mutably borrow live slots without exposing Vec growth.
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}
impl Index<StackIndex> for EvalStack {
    type Output = Value;
    /// Read a typed stack slot.
    fn index(&self, index: StackIndex) -> &Self::Output {
        &self.values[index.raw()]
    }
}
impl IndexMut<StackIndex> for EvalStack {
    /// Mutate a typed stack slot.
    fn index_mut(&mut self, index: StackIndex) -> &mut Self::Output {
        &mut self.values[index.raw()]
    }
}
macro_rules! typed_range_index {
    ($($range:ty),* $(,)?) => {$(
        impl Index<$range> for EvalStack {
            type Output = [Value];
            /// Read a typed range of stack slots.
            fn index(&self, range: $range) -> &Self::Output { self.values.index(raw_bounds(range)) }
        }
        impl IndexMut<$range> for EvalStack {
            /// Mutate a typed range of stack slots.
            fn index_mut(&mut self, range: $range) -> &mut Self::Output { self.values.index_mut(raw_bounds(range)) }
        }
    )*};
}
typed_range_index!(
    std::ops::Range<StackIndex>,
    std::ops::RangeFrom<StackIndex>,
    std::ops::RangeTo<StackIndex>,
    std::ops::RangeFull,
    std::ops::RangeInclusive<StackIndex>,
    std::ops::RangeToInclusive<StackIndex>,
);
impl<'a> IntoIterator for &'a mut EvalStack {
    type Item = &'a mut Value;
    type IntoIter = std::slice::IterMut<'a, Value>;
    /// Visit mutable initialized values for GC forwarding.
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter_mut()
    }
}

pub(crate) trait EvalStackTrait {
    /// Pop a value from the eval stack without bounds checking.
    ///
    /// The compiler guarantees the stack is non-empty at every pop site —
    /// each instruction's stack effect is validated during bytecode emission.
    /// Skipping the bounds check avoids a branch + Option unwrap on every
    /// arithmetic op, comparison, store, call arg, etc. (~43 call sites in
    /// the dispatch loop).
    fn ensure_pop(&mut self) -> Value;
    fn ensure_stack_top(&self) -> StackIndex;
    fn ensure_slot_from_top(&self, index_from_top: usize) -> StackIndex;
    /// Read a slot without bounds checking. Same invariant as `ensure_pop`:
    /// the compiler assigns every local/temp a slot within the live frame's
    /// region, validated at emit time, so the index is always in bounds.
    fn get_at(&self, idx: StackIndex) -> Value;
    /// Write a slot without bounds checking (same invariant). `Value: Copy`,
    /// so the overwrite needs no drop.
    fn set_at(&mut self, idx: StackIndex, v: Value);
}

impl EvalStackTrait for EvalStack {
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn ensure_pop(&mut self) -> Value {
        let new_len = self.values.len() - 1;
        #[allow(unsafe_code)]
        // SAFETY: The bytecode compiler tracks stack depth at every instruction
        // and rejects programs where a pop could underflow. At runtime the stack
        // is always at least as deep as the compiler computed, so this is safe.
        //
        // We bypass Vec::pop() because it returns Option<T> which compiles to a
        // branch even when the None path is unreachable. Instead we decrement
        // len and read the slot directly — zero branches, ~3 native instructions.
        unsafe {
            self.values.set_len(new_len);
            std::ptr::read(self.values.as_ptr().add(new_len))
        }
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn ensure_stack_top(&self) -> StackIndex {
        self.ensure_slot_from_top(0)
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn ensure_slot_from_top(&self, index_from_top: usize) -> StackIndex {
        #[allow(unsafe_code)]
        unsafe {
            StackIndex::from_raw(self.values.len().unchecked_sub(index_from_top + 1))
        }
    }

    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    fn get_at(&self, idx: StackIndex) -> Value {
        // SAFETY: compiler-validated slot (see trait docs).
        unsafe { *self.values.get_unchecked(idx.raw()) }
    }

    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    fn set_at(&mut self, idx: StackIndex, v: Value) {
        // SAFETY: compiler-validated slot (see trait docs).
        unsafe {
            *self.values.get_unchecked_mut(idx.raw()) = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(unsafe_code)]
    fn operand_replacements_preserve_prefix_capacity_and_budget() {
        let mut stack = EvalStack::default();
        stack
            .try_extend([
                Value::int(10),
                Value::int(20),
                Value::int(30),
                Value::int(40),
            ])
            .unwrap();
        let capacity = stack.capacity();
        let ptr = stack.as_ptr();
        // SAFETY: every replacement/truncation consumes initialized live slots.
        unsafe {
            stack.replace_top_n::<1>(Value::int(41));
            stack.replace_top_n::<2>(Value::int(50));
            assert_eq!(
                stack
                    .iter()
                    .map(|value| value.as_int().unwrap())
                    .collect::<Vec<_>>(),
                [10, 20, 50]
            );
            stack.replace_top_n_dynamic(3, Value::int(80));
            assert_eq!(stack.len(), 1);
            assert_eq!(stack[StackIndex::from_raw(0)], Value::int(80));
            stack.truncate_suffix(0);
            stack.truncate_suffix(1);
        }
        assert!(stack.is_empty());
        assert_eq!(stack.capacity(), capacity);
        assert_eq!(stack.as_ptr(), ptr);
    }
}
