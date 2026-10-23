use std::ops::{Bound, Deref, DerefMut, Index, IndexMut, RangeBounds};

use bex_vm_types::{StackIndex, Value};

use crate::{
    VmPanic,
    vm::frame_stack::{StackBudget, StackStorage},
};

/// Argument, local, and operand slots charged to the VM's shared stack budget.
#[derive(Default)]
pub struct EvalStack(pub(crate) StackStorage<Value>, Option<VmPanic>);

impl EvalStack {
    /// Create value storage sharing the call-frame storage budget.
    pub(crate) fn new(budget: StackBudget) -> Self {
        Self(StackStorage::new(budget), None)
    }

    /// Allocated value slots, including unused reserved capacity.
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    /// Push one value with fallible, budgeted backing growth.
    pub fn try_push(&mut self, value: Value) -> Result<(), VmPanic> {
        self.0.try_push(value)
    }

    /// Accept a host operation's result between exec calls. Allocation failure
    /// is delivered through the normal unwinder when execution next resumes.
    pub fn push(&mut self, value: Value) {
        if self.1.is_none() {
            self.1 = self.try_push(value).err();
        }
    }

    /// Take a deferred host-result allocation failure on execution entry.
    pub(crate) fn take_pending_panic(&mut self) -> Option<VmPanic> {
        self.1.take()
    }

    /// Extend an exact-sized sequence after reserving the whole range.
    pub(crate) fn try_extend(
        &mut self,
        values: impl IntoIterator<Item = Value, IntoIter: ExactSizeIterator>,
    ) -> Result<(), VmPanic> {
        let values = values.into_iter();
        self.0.try_reserve(values.len())?;
        for value in values {
            self.0.push_reserved(value);
        }
        Ok(())
    }

    /// Initialize real-local slots after fallibly reserving their backing array.
    pub(crate) fn try_resize(&mut self, len: usize, value: Value) -> Result<(), VmPanic> {
        self.0.try_resize(len, value)
    }

    /// Replace existing operands with one result without allocating.
    #[inline]
    pub(crate) fn replace_top_n_dynamic(&mut self, n: usize, value: Value) {
        assert!(n > 0 && n <= self.len());
        let index = self.len() - n;
        self.0[index] = value;
        self.0.truncate(index + 1);
    }

    /// Replace a fixed number of operands without growing storage.
    #[inline]
    pub(crate) fn replace_top_n<const N: usize>(&mut self, value: Value) {
        self.replace_top_n_dynamic(N, value);
    }

    /// Remove consumed operands without growing storage.
    #[inline]
    pub(crate) fn truncate_suffix(&mut self, n: usize) {
        self.0.truncate(self.len() - n);
    }

    /// Drop trailing live values, keeping their charged capacity for reuse.
    pub fn truncate(&mut self, len: usize) {
        self.0.truncate(len);
    }

    /// Remove a typed range of live slots, retaining backing capacity.
    pub(crate) fn drain(
        &mut self,
        range: impl RangeBounds<StackIndex>,
    ) -> std::vec::Drain<'_, Value> {
        self.0.drain(raw_bounds(range))
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
        &self.0
    }
}
impl DerefMut for EvalStack {
    /// Mutably borrow live slots without exposing Vec growth.
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Index<StackIndex> for EvalStack {
    type Output = Value;
    /// Read a typed stack slot.
    fn index(&self, index: StackIndex) -> &Self::Output {
        &self.0[index.raw()]
    }
}
impl IndexMut<StackIndex> for EvalStack {
    /// Mutate a typed stack slot.
    fn index_mut(&mut self, index: StackIndex) -> &mut Self::Output {
        &mut self.0[index.raw()]
    }
}
macro_rules! typed_range_index {
    ($($range:ty),* $(,)?) => {$(
        impl Index<$range> for EvalStack {
            type Output = [Value];
            /// Read a typed range of stack slots.
            fn index(&self, range: $range) -> &Self::Output { self.0.index(raw_bounds(range)) }
        }
        impl IndexMut<$range> for EvalStack {
            /// Mutate a typed range of stack slots.
            fn index_mut(&mut self, range: $range) -> &mut Self::Output { self.0.index_mut(raw_bounds(range)) }
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
        self.0.iter_mut()
    }
}

pub(crate) trait EvalStackTrait {
    /// Pop a compiler-validated nonempty evaluation stack.
    ///
    /// The compiler guarantees the stack is non-empty at every pop site —
    /// each instruction's stack effect is validated during bytecode emission.
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
        self.0
            .pop()
            .expect("compiler-validated nonempty evaluation stack")
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
            StackIndex::from_raw(self.0.len().unchecked_sub(index_from_top + 1))
        }
    }

    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    fn get_at(&self, idx: StackIndex) -> Value {
        // SAFETY: compiler-validated slot (see trait docs).
        unsafe { *self.0.get_unchecked(idx.raw()) }
    }

    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    fn set_at(&mut self, idx: StackIndex, v: Value) {
        // SAFETY: compiler-validated slot (see trait docs).
        unsafe {
            *self.0.get_unchecked_mut(idx.raw()) = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
        assert!(stack.is_empty());
        assert_eq!(stack.capacity(), capacity);
        assert_eq!(stack.as_ptr(), ptr);
    }
}
