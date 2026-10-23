//! Heap access and allocation for compiled code.

use bex_heap::TlabHolder;
use bex_vm_types::{HeapPtr, Object, ObjectType, RealizedTy, Value, types::Closure};

use crate::{
    errors::{VmError, VmInternalError, VmPanic},
    vm::BexVm,
};

impl BexVm {
    /// Return the length of a container, measuring strings in bytes.
    pub fn container_len(&self, container: Value) -> Result<Value, Box<VmError>> {
        self.container_len_in::<false>(container)
    }

    /// [`Self::container_len`] on a container escape analysis proved
    /// thread-local, without its lock.
    pub fn container_len_local(&self, container: Value) -> Result<Value, Box<VmError>> {
        self.container_len_in::<true>(container)
    }

    #[inline]
    fn container_len_in<const THREAD_LOCAL: bool>(
        &self,
        container: Value,
    ) -> Result<Value, Box<VmError>> {
        let ptr = self.as_object_ptr(container, ObjectType::Array)?;
        // SAFETY (`THREAD_LOCAL`): codegen calls the `_local` entry points
        // only where escape analysis proved no other thread can reach the
        // container.
        #[allow(clippy::cast_possible_wrap, unsafe_code)]
        let len = match self.get_object(ptr) {
            Object::Array(arr) if THREAD_LOCAL => unsafe { arr.lock_thread_local() }.len() as i64,
            Object::Uint8Array(bytes) if THREAD_LOCAL => {
                unsafe { bytes.lock_thread_local() }.len() as i64
            }
            Object::Map(map) if THREAD_LOCAL => unsafe { map.lock_thread_local() }.len() as i64,
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

    /// Load an array or byte array element, counting negative indices from the end.
    pub fn load_array_element(
        &mut self,
        array: Value,
        index: Value,
    ) -> Result<Value, Box<VmError>> {
        self.load_array_element_in::<false>(array, index)
    }

    /// [`Self::load_array_element`] on a thread-local array, without its lock.
    pub fn load_array_element_local(
        &mut self,
        array: Value,
        index: Value,
    ) -> Result<Value, Box<VmError>> {
        self.load_array_element_in::<true>(array, index)
    }

    #[inline]
    fn load_array_element_in<const THREAD_LOCAL: bool>(
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
        // Hold the lock across bounds checking and loading to prevent races with growth.
        let load_result: Result<Value, (i64, usize)> = {
            match self.get_object(array_ptr) {
                Object::Array(arr) => {
                    // SAFETY (`THREAD_LOCAL`): see `container_len_in`.
                    #[allow(unsafe_code)]
                    let guard = if THREAD_LOCAL {
                        unsafe { arr.lock_thread_local() }
                    } else {
                        arr.lock()
                    };
                    crate::array_index::resolve_index(i, guard.len())
                        .map(|idx| guard[idx])
                        .ok_or((i, guard.len()))
                }
                Object::Uint8Array(bytes) => {
                    // SAFETY (`THREAD_LOCAL`): see `container_len_in`.
                    #[allow(unsafe_code)]
                    let guard = if THREAD_LOCAL {
                        unsafe { bytes.lock_thread_local() }
                    } else {
                        bytes.lock()
                    };
                    crate::array_index::resolve_index(i, guard.len())
                        .map(|idx| Value::int(i64::from(guard[idx])))
                        .ok_or((i, guard.len()))
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

    pub fn store_array_element(
        &mut self,
        array: Value,
        index: Value,
        new_value: Value,
    ) -> Result<(), Box<VmError>> {
        self.store_array_element_in::<false>(array, index, new_value)
    }

    /// [`Self::store_array_element`] on a thread-local array, without its lock.
    pub fn store_array_element_local(
        &mut self,
        array: Value,
        index: Value,
        new_value: Value,
    ) -> Result<(), Box<VmError>> {
        self.store_array_element_in::<true>(array, index, new_value)
    }

    #[inline]
    fn store_array_element_in<const THREAD_LOCAL: bool>(
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
                    let debt = self.tlab.alloc_debt();
                    // SAFETY (`THREAD_LOCAL`): see `container_len_in`.
                    #[allow(unsafe_code)]
                    let mut guard = if THREAD_LOCAL {
                        unsafe { arr.lock_mut_thread_local(debt) }
                    } else {
                        arr.lock_mut(debt)
                    };
                    crate::array_index::resolve_index(i, guard.len())
                        .ok_or((i, guard.len()))
                        .map(|idx| guard[idx] = new_value)
                }
                Object::Uint8Array(bytes) => {
                    let Some(byte_v) = new_value_u8 else {
                        return Err(VmInternalError::TypeError {
                            expected: bex_vm_types::types::Type::Int,
                            got: self.type_of(&new_value),
                        }
                        .into());
                    };
                    let debt = self.tlab.alloc_debt();
                    // SAFETY (`THREAD_LOCAL`): see `container_len_in`.
                    #[allow(unsafe_code)]
                    let mut guard = if THREAD_LOCAL {
                        unsafe { bytes.lock_mut_thread_local(debt) }
                    } else {
                        bytes.lock_mut(debt)
                    };
                    crate::array_index::resolve_index(i, guard.len())
                        .ok_or((i, guard.len()))
                        .map(|idx| guard[idx] = byte_v)
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

    #[cold]
    #[inline(never)]
    pub(crate) fn index_out_of_bounds(&mut self, index: i64, length: usize) -> VmError {
        VmError::thrown_fresh(
            self.panic_to_exception_value(VmPanic::IndexOutOfBounds { index, length }),
        )
    }

    pub fn load_field(&mut self, instance: Value, idx: usize) -> Result<Value, Box<VmError>> {
        self.load_field_in::<false>(instance, idx)
    }

    /// [`Self::load_field`] on an instance escape analysis proved
    /// thread-local, with no ordering on the load.
    pub fn load_field_local(&mut self, instance: Value, idx: usize) -> Result<Value, Box<VmError>> {
        self.load_field_in::<true>(instance, idx)
    }

    #[inline]
    fn load_field_in<const THREAD_LOCAL: bool>(
        &mut self,
        instance: Value,
        idx: usize,
    ) -> Result<Value, Box<VmError>> {
        let load_result = {
            let instance = self.as_instance(&instance)?;
            if THREAD_LOCAL {
                instance.try_load_field_relaxed(idx)
            } else {
                instance.try_load_field(idx)
            }
            .ok_or_else(|| instance.field_len())
        };
        match load_result {
            Ok(value) => Ok(value),
            Err(length) => Err(Box::new(self.invalid_field_access_error(idx, length))),
        }
    }

    pub fn store_field(
        &mut self,
        instance: Value,
        idx: usize,
        new_value: Value,
    ) -> Result<(), Box<VmError>> {
        self.store_field_in::<false>(instance, idx, new_value)
    }

    /// [`Self::store_field`] on a thread-local instance, with no ordering on
    /// the store. The write barrier still runs: generations are not threads.
    pub fn store_field_local(
        &mut self,
        instance: Value,
        idx: usize,
        new_value: Value,
    ) -> Result<(), Box<VmError>> {
        self.store_field_in::<true>(instance, idx, new_value)
    }

    #[inline]
    fn store_field_in<const THREAD_LOCAL: bool>(
        &mut self,
        instance: Value,
        idx: usize,
        new_value: Value,
    ) -> Result<(), Box<VmError>> {
        let obj_ptr = self.as_object_ptr(instance, ObjectType::Instance)?;
        let instance_ref = self.as_instance(&instance)?;
        let store_error = (idx >= instance_ref.field_len()).then_some(instance_ref.field_len());
        if let Some(length) = store_error {
            return Err(Box::new(self.invalid_field_access_error(idx, length)));
        }
        self.heap.write_barrier(obj_ptr, new_value);
        let instance = self.as_instance(&instance)?;
        if THREAD_LOCAL {
            instance.store_field_relaxed(idx, new_value);
        } else {
            instance.store_field(idx, new_value);
        }
        Ok(())
    }

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

    pub fn make_bound_method(&mut self, function: HeapPtr, receiver: Value) -> Value {
        let type_args = self.bound_method_curried_type_args(receiver);
        let bound = Object::BoundMethod(bex_vm_types::BoundMethod {
            function,
            receiver,
            type_args,
        });
        Value::object(self.tlab.alloc(bound))
    }

    pub fn discriminant(&self, value: Value) -> Result<i64, Box<VmError>> {
        let ptr = self.as_object_ptr(value, ObjectType::Variant)?;
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

    #[inline]
    #[must_use]
    pub fn is_instance_of(&self, value: Value, class: HeapPtr) -> bool {
        match value.as_object_ptr().map(|ptr| self.get_object(ptr)) {
            Some(Object::Instance(instance)) => instance.class == class,
            _ => false,
        }
    }

    #[inline]
    #[must_use]
    pub fn is_variant_of(&self, value: Value, enm: HeapPtr) -> bool {
        match value.as_object_ptr().map(|ptr| self.get_object(ptr)) {
            Some(Object::Variant(variant)) => variant.enm == enm,
            _ => false,
        }
    }

    #[must_use]
    pub fn class_type(&self, class: HeapPtr) -> RealizedTy {
        self.class_type_at(class, Box::new([]))
    }

    #[inline]
    pub fn new_instance(&mut self, class: HeapPtr, fields: Vec<Value>) -> Value {
        Value::object(self.alloc_instance(class, fields))
    }

    #[inline]
    pub fn new_array(&mut self, element_ty: RealizedTy, values: Vec<Value>) -> Value {
        Value::object(self.alloc_array(element_ty, values))
    }

    #[inline]
    pub fn new_map(&mut self, key_ty: RealizedTy, value_ty: RealizedTy) -> Value {
        Value::object(
            self.tlab
                .alloc_map(key_ty, value_ty, indexmap::IndexMap::new()),
        )
    }

    #[inline]
    pub fn new_variant(&mut self, enm: HeapPtr, index: usize) -> Value {
        Value::object(self.tlab.alloc_variant(enm, index))
    }

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
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
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
