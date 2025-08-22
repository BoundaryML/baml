//! Native functions and methods.
//!
//! We need to find a better pattern for this, but this works for now.

use std::collections::HashMap;

use crate::{
    vm::{InternalError, Object, ObjectType, Vm, VmError},
    Value,
};

impl Vm {
    /// Array length.
    pub fn len(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.

        let expected = ObjectType::Array;
        let ob_index = self.objects.as_object(&args[0], expected)?;

        let Object::Array(array) = &self.objects[ob_index] else {
            return Err(InternalError::TypeError {
                expected: expected.into(),
                got: ObjectType::of(&self.objects[ob_index]).into(),
            }
            .into());
        };

        Ok(Value::Int(array.len() as i64))
    }
}

pub type NativeFunction = fn(&mut Vm, &[Value]) -> Result<Value, VmError>;

pub fn functions() -> HashMap<String, (NativeFunction, usize)> {
    let native_fn: NativeFunction = Vm::len;
    let fns = [("std.Array.len", (native_fn, 1))];

    HashMap::from_iter(fns.into_iter().map(|(name, func)| (name.to_string(), func)))
}
