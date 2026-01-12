use baml_vm_types::{
    Object, ObjectIndex, ObjectPool, ObjectType, StackIndex, Value,
    indexable::{Pool, StackKind},
    types::Type,
};

use crate::{InternalError, NativeFunction, types::ObjectTrait};

pub(crate) trait ObjectPoolTrait {
    fn as_object(
        &self,
        value: &Value,
        object_type: ObjectType,
    ) -> Result<ObjectIndex, InternalError>;
    fn as_string(&self, value: &Value) -> Result<&String, InternalError>;
    #[allow(unused)]
    fn as_string_mut(&mut self, value: &Value) -> Result<&mut String, InternalError>;
    fn as_media(
        &self,
        value: &Value,
        media_kind: baml_vm_types::types::MediaKind,
    ) -> Result<&baml_vm_types::types::MediaValue, InternalError>;
    fn as_array(&self, value: &Value) -> Result<&[Value], InternalError>;
    fn as_array_mut(&mut self, value: &Value) -> Result<&mut Vec<Value>, InternalError>;
    fn as_map(&self, value: &Value) -> Result<&indexmap::IndexMap<String, Value>, InternalError>;
    #[allow(unused)]
    fn as_map_mut(
        &mut self,
        value: &Value,
    ) -> Result<&mut indexmap::IndexMap<String, Value>, InternalError>;
    fn type_of(&self, value: &Value) -> Type;
    fn insert(&mut self, value: Object<NativeFunction>) -> ObjectIndex;
}

impl ObjectPoolTrait for ObjectPool<NativeFunction> {
    /// If `value` is an object, returns a reference to the object.
    /// - If `value` is not an object, throws [`InternalError::TypeError`].
    /// - If `value` is an object but reference is not accessible, throws
    ///   [`InternalError::InvalidObjectRef`].
    fn as_object(
        &self,
        value: &Value,
        object_type: ObjectType,
    ) -> Result<ObjectIndex, InternalError> {
        let Value::Object(index) = value else {
            return Err(InternalError::TypeError {
                expected: object_type.into(),
                got: self.type_of(value),
            });
        };

        Ok(*index)
    }

    fn as_string(&self, value: &Value) -> Result<&String, InternalError> {
        let index = self.as_object(value, ObjectType::String)?;
        self[index].as_string()
    }

    fn as_string_mut(&mut self, value: &Value) -> Result<&mut String, InternalError> {
        let index = self.as_object(value, ObjectType::String)?;
        self[index].as_string_mut()
    }

    fn as_media(
        &self,
        value: &Value,
        media_kind: baml_vm_types::types::MediaKind,
    ) -> Result<&baml_vm_types::types::MediaValue, InternalError> {
        let object_index = self.as_object(value, ObjectType::Media(media_kind))?;

        let Object::Media(media) = &self[object_index] else {
            return Err(InternalError::TypeError {
                expected: ObjectType::Media(media_kind).into(),
                got: ObjectType::of(&self[object_index]).into(),
            });
        };

        Ok(media)
    }

    /// Get an array reference from a Value.
    fn as_array(&self, value: &Value) -> Result<&[Value], InternalError> {
        let object_index = self.as_object(value, ObjectType::Array)?;

        let Object::Array(array) = &self[object_index] else {
            return Err(InternalError::TypeError {
                expected: ObjectType::Array.into(),
                got: ObjectType::of(&self[object_index]).into(),
            });
        };

        Ok(array.as_slice())
    }

    /// Get a mutable array reference from a Value.
    fn as_array_mut(&mut self, value: &Value) -> Result<&mut Vec<Value>, InternalError> {
        let object_index = self.as_object(value, ObjectType::Array)?;

        // Check type first to avoid borrow issues
        if !matches!(&self[object_index], Object::Array(_)) {
            return Err(InternalError::TypeError {
                expected: ObjectType::Array.into(),
                got: ObjectType::of(&self[object_index]).into(),
            });
        }

        let Object::Array(array) = &mut self[object_index] else {
            unreachable!("type was just checked")
        };

        Ok(array)
    }

    /// Get a map reference from a Value.
    fn as_map(&self, value: &Value) -> Result<&indexmap::IndexMap<String, Value>, InternalError> {
        let object_index = self.as_object(value, ObjectType::Map)?;

        let Object::Map(map) = &self[object_index] else {
            return Err(InternalError::TypeError {
                expected: ObjectType::Map.into(),
                got: ObjectType::of(&self[object_index]).into(),
            });
        };

        Ok(map)
    }

    /// Get a mutable map reference from a Value.
    fn as_map_mut(
        &mut self,
        value: &Value,
    ) -> Result<&mut indexmap::IndexMap<String, Value>, InternalError> {
        let object_index = self.as_object(value, ObjectType::Map)?;

        // Check type first to avoid borrow issues
        if !matches!(&self[object_index], Object::Map(_)) {
            return Err(InternalError::TypeError {
                expected: ObjectType::Map.into(),
                got: ObjectType::of(&self[object_index]).into(),
            });
        }

        let Object::Map(map) = &mut self[object_index] else {
            unreachable!("type was just checked")
        };

        Ok(map)
    }

    /// Inspects the type of a value, including the [`ObjectType`] if the object
    /// reference is valid.
    fn type_of(&self, value: &Value) -> Type {
        Type::of(value, |index| ObjectType::of(&self[index]))
    }

    fn insert(&mut self, value: Object<NativeFunction>) -> ObjectIndex {
        self.push(value);
        ObjectIndex::from_raw(self.0.len() - 1)
    }
}

// Type aliases for specific pools and indices

pub type EvalStack = Pool<Value, StackKind>;

pub(crate) trait EvalStackTrait {
    fn ensure_pop(&mut self) -> Result<Value, InternalError>;
    fn ensure_stack_top(&self) -> Result<StackIndex, InternalError>;
    fn ensure_slot_from_top(&self, index_from_top: usize) -> Result<StackIndex, InternalError>;
}

impl EvalStackTrait for EvalStack {
    fn ensure_pop(&mut self) -> Result<Value, InternalError> {
        self.0.pop().ok_or(InternalError::UnexpectedEmptyStack)
    }

    fn ensure_stack_top(&self) -> Result<StackIndex, InternalError> {
        self.ensure_slot_from_top(0)
    }

    fn ensure_slot_from_top(&self, index_from_top: usize) -> Result<StackIndex, InternalError> {
        self.0
            .len()
            .checked_sub(index_from_top + 1)
            .ok_or(InternalError::NotEnoughItemsOnStack(index_from_top))
            .map(StackIndex::from_raw)
    }
}
