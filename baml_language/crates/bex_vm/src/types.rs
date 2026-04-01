use bex_vm_types::{
    Object,
    types::{Function, FunctionType, ObjectType},
};

use crate::errors::VmError;

pub trait ObjectTrait {
    fn as_function(&self) -> Result<&Function, VmError>;
    fn as_string(&self) -> Result<&String, VmError>;
    fn as_string_mut(&mut self) -> Result<&mut String, VmError>;
}

impl ObjectTrait for Object {
    /// Helper to unwrap an [`Object::Function`].
    ///
    /// Used to deal with some borrow checker issues in the [`crate::vm::BexVm::exec`]
    /// function.
    #[inline]
    fn as_function(&self) -> Result<&Function, VmError> {
        match self {
            Object::Function(function) => Ok(function),
            _ => Err(VmError::TypeError {
                expected: FunctionType::Any.into(),
                got: ObjectType::of(self).into(),
            }),
        }
    }

    fn as_string(&self) -> Result<&String, VmError> {
        let Self::String(str) = self else {
            return Err(VmError::TypeError {
                expected: ObjectType::String.into(),
                got: ObjectType::of(self).into(),
            });
        };

        Ok(str)
    }

    fn as_string_mut(&mut self) -> Result<&mut String, VmError> {
        let Self::String(str) = self else {
            return Err(VmError::TypeError {
                expected: ObjectType::String.into(),
                got: ObjectType::of(self).into(),
            });
        };

        Ok(str)
    }
}
