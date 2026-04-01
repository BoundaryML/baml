use bex_vm_types::{
    StackIndex, Value,
    indexable::{Pool, StackKind},
};

use crate::errors::VmError;

// Type aliases for specific pools and indices

pub type EvalStack = Pool<Value, StackKind>;

pub(crate) trait EvalStackTrait {
    fn ensure_pop(&mut self) -> Result<Value, VmError>;
    fn ensure_stack_top(&self) -> Result<StackIndex, VmError>;
    fn ensure_slot_from_top(&self, index_from_top: usize) -> Result<StackIndex, VmError>;
}

impl EvalStackTrait for EvalStack {
    fn ensure_pop(&mut self) -> Result<Value, VmError> {
        self.0.pop().ok_or(VmError::UnexpectedEmptyStack)
    }

    fn ensure_stack_top(&self) -> Result<StackIndex, VmError> {
        self.ensure_slot_from_top(0)
    }

    fn ensure_slot_from_top(&self, index_from_top: usize) -> Result<StackIndex, VmError> {
        self.0
            .len()
            .checked_sub(index_from_top + 1)
            .ok_or(VmError::NotEnoughItemsOnStack(index_from_top))
            .map(StackIndex::from_raw)
    }
}
