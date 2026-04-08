use bex_vm_types::{
    Object,
    types::{Function, FunctionType, ObjectType},
};

use crate::errors::VmInternalError;

pub trait ObjectTrait {
    fn as_function(&self) -> Result<&Function, VmInternalError>;
    fn as_string(&self) -> Result<&String, VmInternalError>;
    fn as_string_mut(&mut self) -> Result<&mut String, VmInternalError>;
}

impl ObjectTrait for Object {
    /// Helper to unwrap an [`Object::Function`].
    ///
    /// Used to deal with some borrow checker issues in the [`crate::vm::BexVm::exec`]
    /// function.
    #[inline]
    fn as_function(&self) -> Result<&Function, VmInternalError> {
        match self {
            Object::Function(function) => Ok(function),
            _ => Err(VmInternalError::TypeError {
                expected: FunctionType::Any.into(),
                got: ObjectType::of(self).into(),
            }),
        }
    }

    fn as_string(&self) -> Result<&String, VmInternalError> {
        let Self::String(str) = self else {
            return Err(VmInternalError::TypeError {
                expected: ObjectType::String.into(),
                got: ObjectType::of(self).into(),
            });
        };

        Ok(str)
    }

    fn as_string_mut(&mut self) -> Result<&mut String, VmInternalError> {
        let Self::String(str) = self else {
            return Err(VmInternalError::TypeError {
                expected: ObjectType::String.into(),
                got: ObjectType::of(self).into(),
            });
        };

        Ok(str)
    }
}
