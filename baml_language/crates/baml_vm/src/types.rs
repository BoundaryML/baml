use baml_vm_types::types::{Function, FunctionType, Object, ObjectType};

use crate::{InternalError, NativeFunction, errors::VmError};

pub trait ObjectTrait {
    fn as_function(&self) -> Result<&Function<NativeFunction>, VmError>;
    fn as_string(&self) -> Result<&String, InternalError>;
    fn as_string_mut(&mut self) -> Result<&mut String, InternalError>;
}

impl ObjectTrait for Object<NativeFunction> {
    /// Helper to unwrap an [`Object::Function`].
    ///
    /// Used to deal with some borrow checker issues in the [`crate::vm::Vm::exec`]
    /// function.
    #[inline]
    fn as_function(&self) -> Result<&Function<NativeFunction>, VmError> {
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
