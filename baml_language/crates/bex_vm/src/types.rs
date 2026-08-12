use bex_vm_types::{
    Object,
    types::{Function, FunctionType, ObjectType},
};

use crate::errors::VmInternalError;

pub trait ObjectTrait {
    fn as_function(&self) -> Result<&Function, VmInternalError>;
    /// Like `as_function`, but also handles `Object::Closure` by returning the
    /// inner `Function`. This fixes the silent-empty-trace bug in `stack_trace()`
    /// where closure frames were causing an early `Err` that got swallowed.
    fn as_callable(&self) -> Result<&Function, VmInternalError>;
    fn as_string(&self) -> Result<&bex_vm_types::BexStr, VmInternalError>;
    fn as_string_mut(&mut self) -> Result<&mut bex_vm_types::BexStr, VmInternalError>;
}

#[allow(unsafe_code)]
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

    /// Unwrap either an [`Object::Function`], the inner function of an
    /// [`Object::Closure`], or the inner function of an [`Object::BoundMethod`],
    /// returning a reference to the underlying `Function`.
    ///
    /// This mirrors the dual-dispatch pattern in `load_function()` in `vm.rs`.
    #[inline]
    fn as_callable(&self) -> Result<&Function, VmInternalError> {
        match self {
            Object::Function(f) => Ok(f),
            Object::Closure(closure) => {
                // SAFETY: closure.function points to a Function object that lives
                // for the lifetime of the program (stored in the object pool).
                // Same guarantee as in load_function() in vm.rs.
                let func_obj: &Object = unsafe { closure.function.get() };
                func_obj.as_function()
            }
            Object::BoundMethod(bm) => {
                // SAFETY: bm.function points to a Function object that lives for
                // the lifetime of the program. Same guarantee as closures.
                let func_obj: &Object = unsafe { bm.function.get() };
                func_obj.as_function()
            }
            _ => Err(VmInternalError::TypeError {
                expected: FunctionType::Any.into(),
                got: ObjectType::of(self).into(),
            }),
        }
    }

    fn as_string(&self) -> Result<&bex_vm_types::BexStr, VmInternalError> {
        let Self::String(str) = self else {
            return Err(VmInternalError::TypeError {
                expected: ObjectType::String.into(),
                got: ObjectType::of(self).into(),
            });
        };

        Ok(str)
    }

    fn as_string_mut(&mut self) -> Result<&mut bex_vm_types::BexStr, VmInternalError> {
        let Self::String(str) = self else {
            return Err(VmInternalError::TypeError {
                expected: ObjectType::String.into(),
                got: ObjectType::of(self).into(),
            });
        };

        Ok(str)
    }
}
