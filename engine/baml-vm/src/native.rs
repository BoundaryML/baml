//! Native functions and methods.
//!
//! We need to find a better pattern for this, but this works for now.

use baml_types::BamlMap;

use crate::{
    vm::{InternalError, Object, ObjectType, Vm, VmError},
    Value,
};

impl Vm {
    /// Array length.
    pub fn array_len(&mut self, args: &[Value]) -> Result<Value, VmError> {
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

impl Vm {
    /// Length of map
    pub fn map_len(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.

        let expected = ObjectType::Map;
        let ob_index = self.objects.as_object(&args[0], expected)?;

        let Object::Map(map) = &self.objects[ob_index] else {
            return Err(InternalError::TypeError {
                expected: expected.into(),
                got: ObjectType::of(&self.objects[ob_index]).into(),
            }
            .into());
        };

        Ok(Value::Int(map.len() as i64))
    }
    /// Map `contains`
    pub fn map_contains(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.

        let expected = ObjectType::Map;
        let ob_index = self.objects.as_object(&args[0], expected)?;

        let Object::Map(map) = &self.objects[ob_index] else {
            return Err(InternalError::TypeError {
                expected: expected.into(),
                got: ObjectType::of(&self.objects[ob_index]).into(),
            }
            .into());
        };

        let key = self.objects.as_string(&args[1])?;

        Ok(Value::Bool(map.contains_key(key)))
    }
}

pub type NativeFunction = fn(&mut Vm, &[Value]) -> Result<Value, VmError>;

pub fn functions() -> BamlMap<String, (NativeFunction, usize)> {
    let fns: &[(&str, (NativeFunction, usize))] = &[
        ("std.Array.len", (Vm::array_len, 1)),
        ("std.Map.len", (Vm::map_len, 1)),
        ("std.Map.contains", (Vm::map_contains, 2)),
    ];

    BamlMap::from_iter(
        fns.iter()
            .map(|(name, (func, arity))| (name.to_string(), (*func, *arity))),
    )
}
