use bex_vm_types::{
    Object,
    types::{Function, FunctionType, ObjectType},
};

use crate::{InternalError, errors::VmError};

pub trait ObjectTrait {
    fn as_function(&self) -> Result<&Function, VmError>;
    fn as_string(&self) -> Result<&String, InternalError>;
    fn as_string_mut(&mut self) -> Result<&mut String, InternalError>;
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
            _ => Err(InternalError::TypeError {
                expected: FunctionType::Any.into(),
                got: ObjectType::of(self).into(),
            }
            .into()),
        }
    }

    fn as_string(&self) -> Result<&String, InternalError> {
        let Self::String(str) = self else {
            return Err(InternalError::TypeError {
                expected: ObjectType::String.into(),
                got: ObjectType::of(self).into(),
            });
        };

        Ok(str)
    }

    fn as_string_mut(&mut self) -> Result<&mut String, InternalError> {
        let Self::String(str) = self else {
            return Err(InternalError::TypeError {
                expected: ObjectType::String.into(),
                got: ObjectType::of(self).into(),
            });
        };

        Ok(str)
    }
}
