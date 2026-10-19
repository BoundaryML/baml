//! Heap objects: containers, instances, closures, enums and classes, and the
//! allocations compiled code makes.

use bex_heap::TlabHolder;
use bex_vm_types::{HeapPtr, Object, ObjectType, RealizedTy, Value, types::Closure};

use crate::{
    errors::{VmError, VmInternalError, VmPanic},
    vm::BexVm,
};

impl BexVm {
    // ── Containers ────────────────────────────────────────────────────────────

    /// `.length()` of an array, byte array, map, or string (in bytes).
    pub fn container_len(&self, container: Value) -> Result<Value, Box<VmError>> {
        let Some(ptr) = container.as_object_ptr() else {
            return Err(VmInternalError::TypeError {
                expected: ObjectType::Array.into(),
                got: self.type_of(&container),
            }
            .into());
        };
        #[allow(clippy::cast_possible_wrap)]
        let len = match self.get_object(ptr) {
            Object::Array(arr) => arr.len() as i64,
            Object::Uint8Array(bytes) => bytes.len() as i64,
            Object::Map(map) => map.len() as i64,
            Object::String(s) => s.byte_len() as i64,
            other => {
                return Err(VmInternalError::TypeError {
                    expected: ObjectType::Array.into(),
                    got: ObjectType::of(other).into(),
                }
                .into());
            }
        };
        Ok(Value::int(len))
    }

    /// `array[index]` for arrays and byte arrays. A negative index counts from
    /// the end; one that still lands outside throws `IndexOutOfBounds` with
    /// the original index.
    pub fn load_array_element(
        &mut self,
        array: Value,
        index: Value,
    ) -> Result<Value, Box<VmError>> {
        let array_ptr = self.as_object_ptr(array, ObjectType::Array)?;
        let Some(i) = index.as_int() else {
            return Err(VmInternalError::TypeError {
                expected: bex_vm_types::types::Type::Int,
                got: self.type_of(&index),
            }
            .into());
        };
        // Acquire the array's read lock for the duration of the bounds-check
        // + element load so it stays atomic against a racing `push`/grow. The
        // guard drops at the end of the inner scope before any `&mut self`
        // call.
        let load_result: Result<Value, (i64, usize)> = {
            match self.get_object(array_ptr) {
                Object::Array(arr) => {
                    let guard = arr.lock();
                    let len = guard.len();
                    match crate::array_index::resolve_index(i, len) {
                        Some(idx) => Ok(guard[idx]),
                        None => Err((i, len)),
                    }
                }
                Object::Uint8Array(bytes) => {
                    let guard = bytes.lock();
                    let len = guard.len();
                    match crate::array_index::resolve_index(i, len) {
                        Some(idx) => Ok(Value::int(i64::from(guard[idx]))),
                        None => Err((i, len)),
                    }
                }
                other => {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Array.into(),
                        got: ObjectType::of(other).into(),
                    }
                    .into());
                }
            }
        };
        match load_result {
            Ok(v) => Ok(v),
            Err((index, length)) => Err(Box::new(self.index_out_of_bounds(index, length))),
        }
    }

    /// `array[index] = value` for arrays and byte arrays, with the same index
    /// semantics as [`Self::load_array_element`]. Applies the generational
    /// write barrier.
    pub fn store_array_element(
        &mut self,
        array: Value,
        index: Value,
        new_value: Value,
    ) -> Result<(), Box<VmError>> {
        let array_ptr = self.as_object_ptr(array, ObjectType::Array)?;
        let Some(i) = index.as_int() else {
            return Err(VmInternalError::TypeError {
                expected: bex_vm_types::types::Type::Int,
                got: self.type_of(&index),
            }
            .into());
        };
        let new_value_u8: Option<u8> = new_value.as_int().map(|v| (v.cast_unsigned() & 0xFF) as u8);
        let store_result: Result<(), (i64, usize)> = {
            match self.get_object(array_ptr) {
                Object::Array(arr) => {
                    let mut guard = arr.lock_mut(self.tlab.alloc_debt());
                    let len = guard.len();
                    match crate::array_index::resolve_index(i, len) {
                        Some(idx) => {
                            guard[idx] = new_value;
                            Ok(())
                        }
                        None => Err((i, len)),
                    }
                }
                Object::Uint8Array(bytes) => {
                    let Some(byte_v) = new_value_u8 else {
                        return Err(VmInternalError::TypeError {
                            expected: bex_vm_types::types::Type::Int,
                            got: self.type_of(&new_value),
                        }
                        .into());
                    };
                    let mut guard = bytes.lock_mut(self.tlab.alloc_debt());
                    let len = guard.len();
                    match crate::array_index::resolve_index(i, len) {
                        Some(idx) => {
                            guard[idx] = byte_v;
                            Ok(())
                        }
                        None => Err((i, len)),
                    }
                }
                other => {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Array.into(),
                        got: ObjectType::of(other).into(),
                    }
                    .into());
                }
            }
        };
        if let Err((index, length)) = store_result {
            return Err(Box::new(self.index_out_of_bounds(index, length)));
        }
        self.heap.write_barrier(array_ptr, new_value);
        Ok(())
    }

    /// Build a catchable `baml.panics.IndexOutOfBounds` throw.
    #[cold]
    #[inline(never)]
    pub fn index_out_of_bounds(&mut self, index: i64, length: usize) -> VmError {
        VmError::thrown_fresh(
            self.panic_to_exception_value(VmPanic::IndexOutOfBounds { index, length }),
        )
    }

    // ── Instances ─────────────────────────────────────────────────────────────

    /// Read field `idx` of a class instance.
    pub fn load_field(&mut self, instance: Value, idx: usize) -> Result<Value, Box<VmError>> {
        let obj_ptr = self.as_object_ptr(instance, ObjectType::Instance)?;
        let load_result = {
            let Object::Instance(instance) = self.get_object(obj_ptr) else {
                return Err(VmInternalError::TypeError {
                    expected: ObjectType::Instance.into(),
                    got: ObjectType::of(self.get_object(obj_ptr)).into(),
                }
                .into());
            };
            instance
                .try_load_field(idx)
                .ok_or_else(|| instance.field_len())
        };
        match load_result {
            Ok(value) => Ok(value),
            Err(length) => Err(Box::new(self.invalid_field_access_error(idx, length))),
        }
    }

    /// Write field `idx` of a class instance, applying the generational write
    /// barrier.
    pub fn store_field(
        &mut self,
        instance: Value,
        idx: usize,
        new_value: Value,
    ) -> Result<(), Box<VmError>> {
        let obj_ptr = self.as_object_ptr(instance, ObjectType::Instance)?;
        let store_error = {
            let Object::Instance(instance) = self.get_object(obj_ptr) else {
                return Err(VmInternalError::TypeError {
                    expected: ObjectType::Instance.into(),
                    got: ObjectType::of(self.get_object(obj_ptr)).into(),
                }
                .into());
            };
            (idx >= instance.field_len()).then_some(instance.field_len())
        };
        if let Some(length) = store_error {
            return Err(Box::new(self.invalid_field_access_error(idx, length)));
        }
        self.heap.write_barrier(obj_ptr, new_value);
        let Object::Instance(instance) = self.get_object(obj_ptr) else {
            unreachable!("already type-checked above");
        };
        instance.store_field(idx, new_value);
        Ok(())
    }

    // ── Closures ──────────────────────────────────────────────────────────────

    /// Allocate a closure over `function` with already-evaluated capture cells
    /// and already-resolved type arguments.
    pub fn make_closure(
        &mut self,
        function: HeapPtr,
        captures: Vec<Value>,
        captured_type_args: Box<[RealizedTy]>,
    ) -> Value {
        let closure = Object::Closure(Closure {
            function,
            captures: captures.into_boxed_slice(),
            captured_type_args,
        });
        Value::object(self.tlab.alloc(closure))
    }

    /// `receiver.method` as a value: a bound method over the function object
    /// `function`, with the receiver's class type arguments curried in, as
    /// bytecode's `MakeBoundMethod` builds it.
    pub fn make_bound_method(&mut self, function: HeapPtr, receiver: Value) -> Value {
        let type_args = self.bound_method_curried_type_args(receiver);
        let bound = Object::BoundMethod(bex_vm_types::BoundMethod {
            function,
            receiver,
            type_args,
        });
        Value::object(self.tlab.alloc(bound))
    }

    // ── Enums and classes ─────────────────────────────────────────────────────

    /// The variant index of an enum value.
    pub fn discriminant(&self, value: Value) -> Result<i64, Box<VmError>> {
        let Some(ptr) = value.as_object_ptr() else {
            return Err(VmInternalError::TypeError {
                expected: ObjectType::Variant.into(),
                got: self.type_of(&value),
            }
            .into());
        };
        match self.get_object(ptr) {
            #[allow(clippy::cast_possible_wrap)]
            Object::Variant(variant) => Ok(variant.index as i64),
            other => Err(VmInternalError::TypeError {
                expected: ObjectType::Variant.into(),
                got: ObjectType::of(other).into(),
            }
            .into()),
        }
    }

    /// `value is C` for a non-generic class `C`: class identity, which is
    /// exactly what the interpreter's type test decides for such a class.
    #[inline]
    #[must_use]
    pub fn is_instance_of(&self, value: Value, class: HeapPtr) -> bool {
        match value.as_object_ptr().map(|ptr| self.get_object(ptr)) {
            Some(Object::Instance(instance)) => instance.class == class,
            _ => false,
        }
    }

    /// `value is E` for an enum `E`.
    #[inline]
    #[must_use]
    pub fn is_variant_of(&self, value: Value, enm: HeapPtr) -> bool {
        match value.as_object_ptr().map(|ptr| self.get_object(ptr)) {
            Some(Object::Variant(variant)) => variant.enm == enm,
            _ => false,
        }
    }

    /// The type of instances of the non-generic class `class`, as a type
    /// argument or an array's element type.
    #[must_use]
    pub fn class_type(&self, class: HeapPtr) -> RealizedTy {
        let Object::Class(declaration) = self.get_object(class) else {
            unreachable!("compiled code names classes by their class object");
        };
        RealizedTy::Class(
            bex_vm_types::TypeHead::new(class, declaration.type_tag),
            Box::new([]),
        )
    }

    /// Allocate an instance of a non-generic class with its fields in slot
    /// order.
    #[inline]
    pub fn new_instance(&mut self, class: HeapPtr, fields: Vec<Value>) -> Value {
        Value::object(self.alloc_instance(class, fields))
    }

    /// Allocate an array of `element_ty`.
    #[inline]
    pub fn new_array(&mut self, element_ty: RealizedTy, values: Vec<Value>) -> Value {
        Value::object(self.alloc_array(element_ty, values))
    }

    /// Allocate an empty map of `key_ty` to `value_ty`: a map literal, whose
    /// entries follow as `baml.Map.set` calls, as bytecode's `AllocMap(0)`.
    #[inline]
    pub fn new_map(&mut self, key_ty: RealizedTy, value_ty: RealizedTy) -> Value {
        Value::object(
            self.tlab
                .alloc_map(key_ty, value_ty, indexmap::IndexMap::new()),
        )
    }

    /// Allocate variant `index` of the enum `enm`: an enum variant literal.
    /// The compiler checked `index` against the enum's declaration, so no
    /// range check runs here (bytecode's `AllocVariant` checks a stack
    /// operand it cannot trust the same way).
    #[inline]
    pub fn new_variant(&mut self, enm: HeapPtr, index: usize) -> Value {
        Value::object(self.tlab.alloc_variant(enm, index))
    }

    /// Allocate a string from a literal.
    #[inline]
    pub fn string_value(&mut self, text: &str) -> Value {
        Value::object(self.alloc_string(text))
    }
}

#[cfg(test)]
mod tests {
    use bex_vm_types::{
        Object, ObjectIndex, RealizedTy, Value,
        types::{Enum, EnumVariant, Owner},
    };
    use indexmap::IndexMap;

    use crate::vm::tests::test_vm;

    fn color() -> Object {
        let variant = |name: &str| EnumVariant {
            name: name.to_string(),
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::default(),
            skip: false,
        };
        Object::Enum(Box::new(Enum {
            path: bex_vm_types::types::LocalName::new(Vec::new(), baml_type::Name::new("Color")),
            type_tag: baml_type::typetag::TypeTag::of_static_index(0),
            variants: vec![variant("Red"), variant("Green")],
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::default(),
            owner: Owner::default(),
        }))
    }

    #[test]
    fn a_new_map_is_empty_and_typed() {
        let mut vm = test_vm(Vec::new());
        let map = vm.new_map(RealizedTy::String, RealizedTy::Int);
        assert_eq!(vm.container_len(map).unwrap(), Value::int(0));
        let Object::Map(object) = vm.get_object(map.as_object_ptr().unwrap()) else {
            panic!("a map object");
        };
        assert_eq!(*object.key_ty, RealizedTy::String);
        assert_eq!(*object.value_ty, RealizedTy::Int);
    }

    #[test]
    fn a_new_variant_is_of_its_enum_at_its_index() {
        let mut vm = test_vm(vec![color()]);
        let color = vm.idx_to_ptr(ObjectIndex::from_raw(0));
        let green = vm.new_variant(color, 1);
        assert!(vm.is_variant_of(green, color));
        assert_eq!(vm.discriminant(green).unwrap(), 1);
        let red = vm.new_variant(color, 0);
        assert_eq!(vm.discriminant(red).unwrap(), 0);
        assert_ne!(red, green, "each literal is a fresh object");
    }
}
