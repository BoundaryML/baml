use bex_vm_types::{
    StackIndex, Value,
    indexable::{Pool, StackKind},
};

use crate::errors::VmInternalError;

// Type aliases for specific pools and indices

pub type EvalStack = Pool<Value, StackKind>;

pub(crate) trait EvalStackTrait {
    fn ensure_pop(&mut self) -> Result<Value, VmInternalError>;
    fn ensure_stack_top(&self) -> Result<StackIndex, VmInternalError>;
    fn ensure_slot_from_top(&self, index_from_top: usize) -> Result<StackIndex, VmInternalError>;
}

impl EvalStackTrait for EvalStack {
    fn ensure_pop(&mut self) -> Result<Value, VmInternalError> {
        self.0.pop().ok_or(VmInternalError::UnexpectedEmptyStack)
    }

    fn ensure_stack_top(&self) -> Result<StackIndex, VmInternalError> {
        self.ensure_slot_from_top(0)
    }

    fn ensure_slot_from_top(&self, index_from_top: usize) -> Result<StackIndex, VmInternalError> {
        self.0
            .len()
            .checked_sub(index_from_top + 1)
            .ok_or(VmInternalError::NotEnoughItemsOnStack(index_from_top))
            .map(StackIndex::from_raw)
    }
}
