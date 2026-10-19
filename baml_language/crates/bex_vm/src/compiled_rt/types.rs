//! Types at run time: a generic body's type arguments and what it derives
//! from them, type values, structural type tests, and generic callables.
//!
//! A compiled generic function is one body for every instantiation, as its
//! bytecode is: it reads `T` off its frame's type arguments (seeded by the
//! call, in the callee's De Bruijn order) and builds every type it needs
//! from them, exactly as `LoadType` substitutes a template against the
//! frame. The compiler renders each template as the `RealizedTy` it builds
//! from these leaves, so a body allocates a derived type once per call.

use bex_vm_types::{
    GlobalIndex, HeapPtr, Object, ObjectType, RealizedTy, TyTemplate, TypeHead, Value,
    types::{FunctionType, GenericFunction, TypeValue},
};

use crate::{
    errors::{VmError, VmInternalError},
    vm::{BexVm, Frame},
};

impl BexVm {
    // ── The frame's type arguments ────────────────────────────────────────────

    /// The type arguments of the compiled function whose frame count is
    /// `depth` (see [`Self::frame_depth`]), in its De Bruijn order: the
    /// owner's (a class's parameters; for an interface body `Self`, then
    /// the interface's), then the function's own.
    #[inline]
    #[must_use]
    pub fn frame_type_args(&self, depth: usize) -> &[RealizedTy] {
        let frame = self.compiled_frame(depth);
        &frame.type_args
    }

    /// Type argument `slot` of the compiled function whose frame count is
    /// `depth`: what `TyTemplate::TypeArgRef(slot)` substitutes to. A slot
    /// the frame does not supply is a frame-layout bug, reported as the
    /// substitution error bytecode raises.
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

    /// `template` realized against the frame's type arguments: what a
    /// `LoadType` of it evaluates to. Compiled code builds the common shapes
    /// directly from [`Self::frame_type_arg`]; this is the general path, for
    /// a template with a projection.
    pub fn substitute(
        &self,
        depth: usize,
        template: &TyTemplate,
    ) -> Result<RealizedTy, Box<VmError>> {
        template
            .substitute(self.frame_type_args(depth), self)
            .map_err(|error| Box::new(substitution_error(&error)))
    }

    /// `ty` as a `type` value. `slot` names the frame slot a bare
    /// `TypeArgRef` read, whose exact value (an explicitly passed `type`,
    /// kept beside the frame's type arguments) keeps its identity, as
    /// bytecode's `LoadType` prefers it.
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

    /// `ty`, a statically known type, as a `type` value: the one object the
    /// running function creates for it, as bytecode's `LoadType` of a fully
    /// realized constant in a static function creates once per site
    /// (`static_load_type_cache`), so two reads of the site are one value.
    /// `ordinal` numbers the site within the compiled body, in a distinct
    /// cache namespace from bytecode constants. A frame binding a runtime
    /// declaration takes a fresh value, as there.
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

    /// A `let type T extends I = …` bound check, as bytecode's
    /// `SatisfiesBound`: whether the type `value` holds satisfies `bound`,
    /// the constraint as a `type` value.
    pub fn bound_satisfied(&mut self, value: Value, bound: Value) -> Result<bool, Box<VmError>> {
        self.satisfies_bound(value, bound).map_err(Box::new)
    }

    /// `type T = value` in a block: bind the `type` value `value` into slot
    /// `slot` of the frame, as `BindType` does, so later templates reading
    /// the slot see it. Every derived type the body builds after this reads
    /// the frame again.
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

    // ── Type tests ────────────────────────────────────────────────────────────

    /// `value is T` for a realized `T`: membership in the type, by the
    /// canonical algebra, as bytecode's structural type test decides it.
    #[must_use]
    pub fn value_is_member_of(&self, value: Value, ty: &RealizedTy) -> bool {
        crate::type_match::value_is_member(self, value, ty.as_ty()).unwrap_or(false)
    }

    /// `value is C<..>` for a generic class `C` at `type_args`: class
    /// identity, then each argument invariantly equivalent to the instance's,
    /// as the interpreter's test against a class with type arguments.
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

    // ── Declared types ────────────────────────────────────────────────────────

    /// The type of instances of `class` at `type_args`; see
    /// [`Self::class_type`].
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

    /// Allocate an instance of the generic class `class` at `type_args`
    /// with its fields in slot order.
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

    // ── Generic callables ─────────────────────────────────────────────────────

    /// `foo<T>` as a value, in the compiled function whose frame count is
    /// `depth`: a `GenericFunction` over the function in global slot
    /// `function` at `type_args`, as `MakeGenericFunction` builds it. (A
    /// fully concrete `foo<int>` is the program's pooled object instead.)
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

    /// `g<T>` on a callable value `callable`: a closure over its function
    /// carrying its captures and type arguments, then `type_args`, as
    /// `MakeGenericFunctionFromValue` builds it. A bound method passes
    /// through unchanged, as there.
    pub fn specialize_function_value(
        &mut self,
        callable: Value,
        type_args: &[RealizedTy],
    ) -> Result<Value, Box<VmError>> {
        let callable_ptr = self
            .as_object_ptr(callable, ObjectType::from(FunctionType::Callable))
            .map_err(|error| Box::new(VmError::InternalError(error)))?;
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

    /// A VM with one compiled frame on top whose type arguments are
    /// `type_args`, as `call_compiled_with_types` pushes it.
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

    /// A template over the frame substitutes to the realized type bytecode's
    /// `LoadType` would build; a slot the frame does not supply is an
    /// internal error, never an erasure.
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

    /// A value is a member of the type its frame argument names, as `is T`
    /// decides it.
    #[test]
    fn values_are_tested_against_substituted_types() {
        let vm = vm_with_frame(&[RealizedTy::Int]);
        let depth = vm.frame_depth();
        let t = vm.frame_type_arg(depth, 0).unwrap();
        assert!(vm.value_is_member_of(Value::int(3), &t));
        assert!(!vm.value_is_member_of(Value::bool(true), &t));
        assert!(!vm.value_is_member_of(Value::NULL, &t));
    }

    /// A statically known type read as a value is one object per site, as
    /// bytecode's `LoadType` cache makes it; sites differ.
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

    /// `type T = ..` widens the frame and every later read sees the bound
    /// type; a `type` value of a bare slot carries the exact value bound.
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
