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
        let ob_index = self.as_object(&args[0], expected)?;

        let Object::Array(array) = self.objects.get(ob_index)? else {
            return Err(InternalError::TypeError {
                expected: expected.into(),
                got: ObjectType::of(self.objects.get(ob_index).unwrap_or(&Object::Null)).into(),
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
        let ob_index = self.as_object(&args[0], expected)?;

        let Object::Map(map) = self.objects.get(ob_index)? else {
            return Err(InternalError::TypeError {
                expected: expected.into(),
                got: ObjectType::of(self.objects.get(ob_index).unwrap_or(&Object::Null)).into(),
            }
            .into());
        };

        Ok(Value::Int(map.len() as i64))
    }
    /// Map `contains`
    pub fn map_contains(&mut self, args: &[Value]) -> Result<Value, VmError> {
        // Arity is already checked by the VM.

        let expected = ObjectType::Map;
        let ob_index = self.as_object(&args[0], expected)?;

        let Object::Map(map) = self.objects.get(ob_index)? else {
            return Err(InternalError::TypeError {
                expected: expected.into(),
                got: ObjectType::of(self.objects.get(ob_index).unwrap_or(&Object::Null)).into(),
            }
            .into());
        };

        let key_idx = self.as_object(&args[1], ObjectType::String)?;
        let key = self.objects.get(key_idx)?.as_string()?;

        Ok(Value::Bool(map.contains_key(key)))
    }
}

pub type NativeFunction = fn(&mut Vm, &[Value]) -> Result<Value, VmError>;

pub fn functions() -> BamlMap<String, (NativeFunction, usize)> {
    let native_fn: NativeFunction = Vm::array_len;
    let fns = [
        ("std.Array.len".to_string(), (native_fn, 1)),
        ("std.Map.len".to_string(), (Vm::map_len, 1)),
        ("std.Map.contains".to_string(), (Vm::map_contains, 2)),
    ];

    BamlMap::from_iter(fns)
}
