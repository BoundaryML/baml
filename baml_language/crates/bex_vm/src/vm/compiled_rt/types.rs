//! Runtime type arguments, type values, structural tests, and generic callables.

use bex_vm_types::{
    GlobalIndex, HeapPtr, Object, ObjectType, RealizedTy, TypeHead, Value,
    types::{FunctionType, GenericFunction, TypeValue},
};

use super::FrameRecord;
use crate::{
    errors::{VmError, VmInternalError},
    vm::{BexVm, Frame},
};

impl BexVm {
    /// Return frame type arguments in De Bruijn order, with owner arguments first.
    #[inline]
    pub(crate) fn frame_type_args<'a>(&'a self, frame: &'a FrameRecord) -> &'a [RealizedTy] {
        match Self::record_vm_frame(frame) {
            Some(idx) => match &self.frames[idx] {
                Frame::Bytecode(frame) => &frame.type_args,
                Frame::Native(_) => &[],
            },
            None => frame.own_type_args(),
        }
    }

    fn entry_frame(&self, frame: &FrameRecord) -> Option<&crate::vm::BytecodeFrame> {
        match &self.frames[Self::record_vm_frame(frame)?] {
            Frame::Bytecode(frame) => Some(frame),
            Frame::Native(_) => None,
        }
    }

    #[inline]
    pub fn frame_type_arg(
        &self,
        frame: &FrameRecord,
        slot: u32,
    ) -> Result<RealizedTy, Box<VmError>> {
        let type_args = self.frame_type_args(frame);
        match type_args.get(slot as usize) {
            Some(ty) => Ok(ty.clone()),
            None => Err(Box::new(substitution_error(
                &baml_type::SubstituteError::TypeArgRefOutOfRange {
                    index: slot,
                    frame_len: type_args.len(),
                },
            ))),
        }
    }

    /// Create a type value, preserving the identity of an explicitly bound `slot`.
    pub fn type_value(&mut self, frame: &FrameRecord, slot: Option<u32>, ty: RealizedTy) -> Value {
        // Only a VM frame carries exact type values; see `direct_calls_typed`.
        let exact = slot.and_then(|slot| {
            self.entry_frame(frame)?
                .type_metadata
                .as_ref()
                .and_then(|metadata| metadata.values.get(slot as usize))
                .and_then(Clone::clone)
        });
        let type_value = exact.unwrap_or_else(|| TypeValue::new(ty));
        Value::object(self.tlab.alloc_type(type_value))
    }

    /// Cache a static type value by site ordinal unless the frame binds a runtime declaration.
    pub fn static_type_value(
        &mut self,
        frame: &FrameRecord,
        ordinal: usize,
        ty: RealizedTy,
    ) -> Value {
        let cacheable = self
            .entry_frame(frame)
            .and_then(|frame| frame.type_metadata.as_ref())
            .is_none_or(|metadata| !metadata.binds_runtime_declaration(&self.heap));
        let key = match self.get_object(frame.function()) {
            Object::Function(function)
                if cacheable && self.runtime_package_of(function).is_null() =>
            {
                Some((std::ptr::from_ref(function.as_ref()) as usize, ordinal))
            }
            _ => None,
        };
        if let Some(cached) = key.and_then(|key| self.compiled.static_type_cache.get(&key).copied())
        {
            return Value::object(cached);
        }
        let ptr = self.tlab.alloc_type(TypeValue::new(ty));
        if let Some(key) = key {
            self.compiled.static_type_cache.insert(key, ptr);
        }
        Value::object(ptr)
    }

    pub fn bound_satisfied(&mut self, value: Value, bound: Value) -> Result<bool, Box<VmError>> {
        self.satisfies_bound(value, bound).map_err(Box::new)
    }

    pub fn type_operand(&mut self, value: Value) -> Result<RealizedTy, Box<VmError>> {
        Ok(self.type_operand_value(value).map_err(Box::new)?.ty)
    }

    pub fn bind_type(
        &mut self,
        frame: &FrameRecord,
        slot: u32,
        value: Value,
    ) -> Result<(), Box<VmError>> {
        let type_value = self.type_operand_value(value).map_err(Box::new)?;
        // A body that binds types runs with a VM frame; see `CompiledEntry::framed`.
        let Some(Frame::Bytecode(frame)) =
            Self::record_vm_frame(frame).map(|idx| &mut self.frames[idx])
        else {
            return Err(Box::new(
                VmInternalError::CompiledBody {
                    message: "a body that binds types ran without a VM frame".to_string(),
                }
                .into(),
            ));
        };
        let slot = slot as usize;
        frame.type_args.resize(slot + 1, RealizedTy::unknown());
        frame.type_args[slot] = type_value.ty.clone();
        let metadata = frame
            .type_metadata
            .get_or_insert_with(|| Box::new(crate::vm::FrameTypeMetadata::default()));
        metadata.values.resize(slot + 1, None);
        metadata.values[slot] = Some(type_value);
        Ok(())
    }

    #[must_use]
    pub fn value_is_member_of(&self, value: Value, ty: &RealizedTy) -> bool {
        crate::type_match::value_is_member(self, value, ty.as_ty()).unwrap_or(false)
    }

    #[must_use]
    pub fn is_instance_of_at(
        &self,
        value: Value,
        class: HeapPtr,
        type_args: &[RealizedTy],
    ) -> bool {
        match value.as_object_ptr().map(|ptr| self.get_object(ptr)) {
            Some(Object::Instance(instance)) if instance.class == class => {
                instance.class_type_args.len() == type_args.len()
                    && instance
                        .class_type_args
                        .iter()
                        .zip(type_args)
                        .all(|(actual, expected)| {
                            baml_type::normalize::equivalent(actual.as_ty(), expected.as_ty(), self)
                        })
            }
            _ => false,
        }
    }

    #[inline]
    #[must_use]
    pub fn class_type_at(&self, class: HeapPtr, type_args: Box<[RealizedTy]>) -> RealizedTy {
        let Object::Class(declaration) = self.get_object(class) else {
            unreachable!("compiled code names classes by their class object");
        };
        RealizedTy::Class(TypeHead::new(class, declaration.type_tag), type_args)
    }

    #[must_use]
    pub fn interface_type_at(
        &self,
        iface: HeapPtr,
        type_args: Box<[RealizedTy]>,
        assoc: &[(&str, RealizedTy)],
    ) -> RealizedTy {
        let Object::Interface(declaration) = self.get_object(iface) else {
            unreachable!("compiled code names interfaces by their interface object");
        };
        let mut assoc: Vec<(baml_type::Name, RealizedTy)> = assoc
            .iter()
            .map(|(name, ty)| (baml_type::Name::from(*name), ty.clone()))
            .collect();
        assoc.sort_by(|(left, _), (right, _)| left.cmp(right));
        RealizedTy::Interface(
            TypeHead::new(iface, declaration.type_tag),
            type_args,
            assoc.into_boxed_slice(),
        )
    }

    #[must_use]
    pub fn enum_type(&self, enm: HeapPtr) -> RealizedTy {
        let Object::Enum(declaration) = self.get_object(enm) else {
            unreachable!("compiled code names enums by their enum object");
        };
        RealizedTy::Enum(TypeHead::new(enm, declaration.type_tag))
    }

    #[inline]
    pub fn new_generic_instance(
        &mut self,
        class: HeapPtr,
        type_args: Box<[RealizedTy]>,
        fields: Vec<Value>,
    ) -> Value {
        Value::object(
            self.tlab
                .alloc_instance_with_type_args(class, type_args, fields),
        )
    }

    pub fn make_generic_function(
        &mut self,
        frame: &FrameRecord,
        function: GlobalIndex,
        type_args: Box<[RealizedTy]>,
    ) -> Value {
        let runtime_package = match self.get_object(frame.function()) {
            Object::Function(running) => self.runtime_package_of(running),
            _ => HeapPtr::null(),
        };
        let generic = Object::GenericFunction(GenericFunction {
            function,
            type_args,
            runtime_package,
        });
        Value::object(self.tlab.alloc(generic))
    }

    /// Append type arguments to a callable while preserving captures and bound methods.
    pub fn specialize_function_value(
        &mut self,
        callable: Value,
        type_args: &[RealizedTy],
    ) -> Result<Value, Box<VmError>> {
        let callable_ptr =
            self.as_object_ptr(callable, ObjectType::from(FunctionType::Callable))?;
        let wrapped = match self.get_object(callable_ptr) {
            Object::Function(_) => Some((callable_ptr, Vec::new(), Vec::new())),
            Object::Closure(closure) => Some((
                closure.function,
                closure.captures.to_vec(),
                closure.captured_type_args.to_vec(),
            )),
            _ => None,
        };
        Ok(match wrapped {
            Some((function, captures, mut captured_type_args)) => {
                captured_type_args.extend_from_slice(type_args);
                self.make_closure(function, captures, captured_type_args.into_boxed_slice())
            }
            None => callable,
        })
    }
}

#[cold]
#[inline(never)]
fn substitution_error(error: &baml_type::SubstituteError) -> VmError {
    VmInternalError::TypeSubstitution {
        message: error.to_string(),
    }
    .into()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::AtomicBool};

    use bex_vm_types::{RealizedTy, StackIndex, Value};

    use crate::vm::{BexVm, BytecodeFrame, Frame};

    fn vm_with_frame(type_args: &[RealizedTy]) -> BexVm {
        let program = baml_test_support::compile_source("function Main() -> int { 1 }");
        let entry = program.rendered_callables()["user.Main"].object.raw();
        let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
        let function = vm.heap.compile_time_ptr(entry);
        let context = vm.current_context().clone();
        vm.frames.push(Frame::Bytecode(BytecodeFrame::compiled(
            function,
            StackIndex::from_raw(0),
            type_args.to_vec(),
            context,
            None,
        )));
        vm
    }

    fn with_frame(type_args: &[RealizedTy], test: impl FnOnce(&mut BexVm, &super::FrameRecord)) {
        vm_with_frame(type_args)
            .run_entry_with(|vm, frame| -> Result<(), Box<crate::errors::VmError>> {
                test(vm, frame);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn templates_substitute_against_the_frame() {
        with_frame(&[RealizedTy::Int, RealizedTy::String], |vm, fr| {
            assert_eq!(vm.frame_type_arg(fr, 1).unwrap(), RealizedTy::String);
            let out_of_range = vm.frame_type_arg(fr, 2).unwrap_err();
            assert!(
                matches!(
                    *out_of_range,
                    crate::errors::VmError::InternalError(
                        crate::errors::VmInternalError::TypeSubstitution { .. }
                    )
                ),
                "{out_of_range:?}"
            );
        });
    }

    #[test]
    fn values_are_tested_against_substituted_types() {
        with_frame(&[RealizedTy::Int], |vm, fr| {
            let t = vm.frame_type_arg(fr, 0).unwrap();
            assert!(vm.value_is_member_of(Value::int(3), &t));
            assert!(!vm.value_is_member_of(Value::bool(true), &t));
            assert!(!vm.value_is_member_of(Value::NULL, &t));
        });
    }

    #[test]
    fn static_type_values_are_one_object_per_site() {
        with_frame(&[], |vm, fr| {
            let first = vm.static_type_value(fr, 0, RealizedTy::Int);
            let again = vm.static_type_value(fr, 0, RealizedTy::Int);
            let other = vm.static_type_value(fr, 1, RealizedTy::Int);
            assert_eq!(first.as_object_ptr(), again.as_object_ptr());
            assert_ne!(first.as_object_ptr(), other.as_object_ptr());
        });
    }

    #[test]
    fn type_operands_realize_type_values_only() {
        with_frame(&[], |vm, fr| {
            let value = vm.type_value(fr, None, RealizedTy::String);
            assert_eq!(vm.type_operand(value).unwrap(), RealizedTy::String);
            assert!(vm.type_operand(Value::int(1)).is_err());
        });
    }

    #[test]
    fn interface_types_are_built_from_their_declaration() {
        let program = baml_test_support::compile_source(
            "interface Named { name: string }\nfunction Main() -> int { 1 }",
        );
        let iface = (&program.objects)
            .into_iter()
            .position(|object| matches!(object, bex_vm_types::Object::Interface(_)))
            .expect("the program declares an interface");
        let vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
        let ptr = vm.heap.compile_time_ptr(iface);
        let bex_vm_types::Object::Interface(declaration) = vm.get_object(ptr) else {
            unreachable!()
        };
        let tag = declaration.type_tag;
        let ty = vm.interface_type_at(
            ptr,
            Box::new([RealizedTy::Int]),
            &[("Second", RealizedTy::String), ("First", RealizedTy::Bool)],
        );
        let RealizedTy::Interface(head, args, assoc) = ty else {
            panic!("an interface type: {ty:?}")
        };
        assert_eq!(head.tag(), tag);
        assert_eq!(&*args, &[RealizedTy::Int]);
        let names: Vec<&str> = assoc.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["First", "Second"], "bound in name order");
    }

    #[test]
    fn bound_types_widen_the_frame() {
        with_frame(&[RealizedTy::Int], |vm, fr| {
            let bound = vm.type_value(fr, None, RealizedTy::String);
            vm.bind_type(fr, 2, bound).unwrap();
            assert_eq!(vm.frame_type_args(fr).len(), 3);
            assert_eq!(vm.frame_type_arg(fr, 1).unwrap(), RealizedTy::unknown());
            assert_eq!(vm.frame_type_arg(fr, 2).unwrap(), RealizedTy::String);
            let read = vm.type_value(fr, Some(2), RealizedTy::Int);
            let read = vm
                .as_object_ptr(read, bex_vm_types::ObjectType::Type)
                .unwrap();
            let bex_vm_types::Object::Type(type_value) = vm.get_object(read) else {
                unreachable!()
            };
            assert_eq!(
                type_value.ty,
                RealizedTy::String,
                "the exact bound value wins"
            );
        });
    }
}
