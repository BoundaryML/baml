//! Native functions and methods.
//!
//! We need to find a better pattern for this, but this works for now.

use std::collections::HashMap;

use crate::{
    vm::{InternalError, Object, Type, Vm, VmError},
    Value,
};

impl Vm {
    /// Array length.
    pub fn len(&mut self, args: &[Value]) -> Result<Value, VmError> {
        if args.len() != 1 {
            return Err(VmError::from(InternalError::InvalidArgumentCount {
                expected: 1,
                got: args.len(),
            }));
        }

        let Value::Object(array) = args[0] else {
            return Err(VmError::from(InternalError::TypeError {
                expected: Type::Object,
                got: Type::of(&args[0]),
            }));
        };

        let Object::Array(array) = &self.objects[array] else {
            return Err(VmError::from(InternalError::TypeError {
                expected: Type::Object,
                got: Type::Object,
            }));
        };

        Ok(Value::Int(array.len() as i64))
    }
}

pub type NativeFunction = fn(&mut Vm, &[Value]) -> Result<Value, VmError>;

pub fn functions() -> HashMap<String, (NativeFunction, usize)> {
    let native_fn: NativeFunction = Vm::len;
    let fns = [("len", (native_fn, 1))];

    HashMap::from_iter(fns.into_iter().map(|(name, func)| (name.to_string(), func)))
}
