use bex_vm_types::{
    StackIndex, Value,
    indexable::{Pool, StackKind},
};

// Type aliases for specific pools and indices

pub type EvalStack = Pool<Value, StackKind>;

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
}

impl EvalStackTrait for EvalStack {
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn ensure_pop(&mut self) -> Value {
        let new_len = self.0.len() - 1;
        #[allow(unsafe_code)]
        // SAFETY: The bytecode compiler tracks stack depth at every instruction
        // and rejects programs where a pop could underflow. At runtime the stack
        // is always at least as deep as the compiler computed, so this is safe.
        //
        // We bypass Vec::pop() because it returns Option<T> which compiles to a
        // branch even when the None path is unreachable. Instead we decrement
        // len and read the slot directly — zero branches, ~3 native instructions.
        unsafe {
            self.0.set_len(new_len);
            std::ptr::read(self.0.as_ptr().add(new_len))
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
            StackIndex::from_raw(self.0.len().unchecked_sub(index_from_top + 1))
        }
    }
}
