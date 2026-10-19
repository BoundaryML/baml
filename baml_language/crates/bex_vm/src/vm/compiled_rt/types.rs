//! Runtime type arguments, type values, structural tests, and generic callables.

use bex_vm_types::{
    GlobalIndex, HeapPtr, Object, ObjectType, RealizedTy, TyTemplate, TypeHead, Value,
    types::{FunctionType, GenericFunction, TypeValue},
};

use crate::{
    errors::{VmError, VmInternalError},
    vm::{BexVm, Frame},
};

impl BexVm {
    /// Return frame type arguments in De Bruijn order, with owner arguments first.
    #[inline]
    #[must_use]
    pub fn frame_type_args(&self, depth: usize) -> &[RealizedTy] {
        let frame = self.compiled_frame(depth);
        &frame.type_args
    }

    /// Read a frame type argument, reporting a missing slot as a substitution error.
    #[inline]
    pub fn frame_type_arg(&self, depth: usize, slot: u32) -> Result<RealizedTy, Box<VmError>> {
        let type_args = self.frame_type_args(depth);
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

    /// Resolve a template against the frame's type arguments.
    pub fn substitute(
        &self,
        depth: usize,
        template: &TyTemplate,
    ) -> Result<RealizedTy, Box<VmError>> {
        template
            .substitute(self.frame_type_args(depth), self)
            .map_err(|error| Box::new(substitution_error(&error)))
    }

    /// Create a type value, preserving the identity of an explicitly bound `slot`.
    pub fn type_value(&mut self, depth: usize, slot: Option<u32>, ty: RealizedTy) -> Value {
        let exact = slot.and_then(|slot| match &self.frames[depth - 1] {
            Frame::Bytecode(frame) => frame
                .type_metadata
                .as_ref()
                .and_then(|metadata| metadata.values.get(slot as usize))
                .and_then(Clone::clone),
            Frame::Native(_) => None,
        });
        let type_value = exact.unwrap_or_else(|| TypeValue::new(ty));
        Value::object(self.tlab.alloc_type(type_value))
    }

    /// Cache a static type value by site ordinal unless the frame binds a runtime declaration.
    pub fn static_type_value(&mut self, depth: usize, ordinal: usize, ty: RealizedTy) -> Value {
        let frame = self.compiled_frame(depth);
        let cacheable = frame
            .type_metadata
            .as_ref()
            .is_none_or(|metadata| !metadata.binds_runtime_declaration(&self.heap));
        let key = match self.get_object(frame.function) {
            Object::Function(function)
                if cacheable && self.runtime_package_of(function).is_null() =>
            {
                Some((
                    std::ptr::from_ref(function.as_ref()) as usize,
                    crate::vm::TypeValueSite::Compiled(ordinal),
                ))
            }
            _ => None,
        };
        if let Some(cached) = key.and_then(|key| self.static_load_type_cache.get(&key).copied()) {
            return Value::object(cached);
        }
        let ptr = self.tlab.alloc_type(TypeValue::new(ty));
        if let Some(key) = key {
            self.static_load_type_cache.insert(key, ptr);
        }
        Value::object(ptr)
    }

    /// Check whether the type held by `value` satisfies `bound`.
    pub fn bound_satisfied(&mut self, value: Value, bound: Value) -> Result<bool, Box<VmError>> {
        self.satisfies_bound(value, bound).map_err(Box::new)
    }

    /// Bind a type value to a frame slot for subsequent template substitution.
    pub fn bind_type(&mut self, depth: usize, slot: u32, value: Value) -> Result<(), Box<VmError>> {
        let type_value = self.type_operand_value(value).map_err(Box::new)?;
        let frame = self.compiled_frame_mut(depth);
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

    /// Test membership in a realized type.
    #[must_use]
    pub fn value_is_member_of(&self, value: Value, ty: &RealizedTy) -> bool {
        crate::type_match::value_is_member(self, value, ty.as_ty()).unwrap_or(false)
    }

    /// Test class identity and invariant equivalence of its type arguments.
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

    /// Return the instance type of `class` at `type_args`.
    #[must_use]
    pub fn class_type_at(&self, class: HeapPtr, type_args: Box<[RealizedTy]>) -> RealizedTy {
        let Object::Class(declaration) = self.get_object(class) else {
            unreachable!("compiled code names classes by their class object");
        };
        RealizedTy::Class(TypeHead::new(class, declaration.type_tag), type_args)
    }

    /// The type of the enum `enm`'s values.
    #[must_use]
    pub fn enum_type(&self, enm: HeapPtr) -> RealizedTy {
        let Object::Enum(declaration) = self.get_object(enm) else {
            unreachable!("compiled code names enums by their enum object");
        };
        RealizedTy::Enum(TypeHead::new(enm, declaration.type_tag))
    }

    /// Allocate a generic class instance with fields in slot order.
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

    /// Create a generic function value in the running frame's package.
    pub fn make_generic_function(
        &mut self,
        depth: usize,
        function: GlobalIndex,
        type_args: Box<[RealizedTy]>,
    ) -> Value {
        let runtime_package = match self.get_object(self.frames[depth - 1].function()) {
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

    use bex_vm_types::{RealizedTy, StackIndex, TyTemplate, Value};

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

    #[test]
    fn templates_substitute_against_the_frame() {
        let vm = vm_with_frame(&[RealizedTy::Int, RealizedTy::String]);
        let depth = vm.frame_depth();
        assert_eq!(vm.frame_type_arg(depth, 1).unwrap(), RealizedTy::String);
        let list_of_t = TyTemplate::List(Box::new(TyTemplate::TypeArgRef(0)));
        assert_eq!(
            vm.substitute(depth, &list_of_t).unwrap(),
            RealizedTy::List(Box::new(RealizedTy::Int))
        );
        let out_of_range = vm.frame_type_arg(depth, 2).unwrap_err();
        assert!(
            matches!(
                *out_of_range,
                crate::errors::VmError::InternalError(
                    crate::errors::VmInternalError::TypeSubstitution { .. }
                )
            ),
            "{out_of_range:?}"
        );
        assert!(vm.substitute(depth, &TyTemplate::TypeArgRef(7)).is_err());
    }

    #[test]
    fn values_are_tested_against_substituted_types() {
        let vm = vm_with_frame(&[RealizedTy::Int]);
        let depth = vm.frame_depth();
        let t = vm.frame_type_arg(depth, 0).unwrap();
        assert!(vm.value_is_member_of(Value::int(3), &t));
        assert!(!vm.value_is_member_of(Value::bool(true), &t));
        assert!(!vm.value_is_member_of(Value::NULL, &t));
    }

    #[test]
    fn static_type_values_are_one_object_per_site() {
        let mut vm = vm_with_frame(&[]);
        let depth = vm.frame_depth();
        let first = vm.static_type_value(depth, 0, RealizedTy::Int);
        let again = vm.static_type_value(depth, 0, RealizedTy::Int);
        let other = vm.static_type_value(depth, 1, RealizedTy::Int);
        assert_eq!(first.as_object_ptr(), again.as_object_ptr());
        assert_ne!(first.as_object_ptr(), other.as_object_ptr());
    }

    #[test]
    fn bound_types_widen_the_frame() {
        let mut vm = vm_with_frame(&[RealizedTy::Int]);
        let depth = vm.frame_depth();
        let bound = vm.type_value(depth, None, RealizedTy::String);
        vm.bind_type(depth, 2, bound).unwrap();
        assert_eq!(vm.frame_type_args(depth).len(), 3);
        assert_eq!(vm.frame_type_arg(depth, 1).unwrap(), RealizedTy::unknown());
        assert_eq!(vm.frame_type_arg(depth, 2).unwrap(), RealizedTy::String);
        let read = vm.type_value(depth, Some(2), RealizedTy::Int);
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
    }
}
