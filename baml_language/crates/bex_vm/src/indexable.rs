use bex_vm_types::{
    StackIndex, Value,
    indexable::{Pool, StackKind},
};

use crate::InternalError;

// Type aliases for specific pools and indices

pub type EvalStack = Pool<Value, StackKind>;

pub(crate) trait EvalStackTrait {
    fn ensure_pop(&mut self) -> Result<Value, InternalError>;
    fn ensure_stack_top(&self) -> Result<StackIndex, InternalError>;
    fn ensure_slot_from_top(&self, index_from_top: usize) -> Result<StackIndex, InternalError>;
}

impl EvalStackTrait for EvalStack {
    fn ensure_pop(&mut self) -> Result<Value, InternalError> {
        self.0.pop().ok_or(InternalError::UnexpectedEmptyStack)
    }

    fn ensure_stack_top(&self) -> Result<StackIndex, InternalError> {
        self.ensure_slot_from_top(0)
    }

    fn ensure_slot_from_top(&self, index_from_top: usize) -> Result<StackIndex, InternalError> {
        self.0
            .len()
            .checked_sub(index_from_top + 1)
            .ok_or(InternalError::NotEnoughItemsOnStack(index_from_top))
            .map(StackIndex::from_raw)
    }
}
