//! The program around a compiled body: its function's constant pool, the
//! globals a top-level `let` lives in, and the package that owns it.
//!
//! A compiled body replaces a bytecode function but keeps that function's
//! object: its constant pool is still in the program, so a type template the
//! body tests against or allocates with is read from there by index, exactly
//! the constant the bytecode's `IsType`/`LoadType` would read.

use bex_vm_types::{GlobalIndex, HeapPtr, Object, RealizedTy, Value, types::ConstValue};

use crate::{
    errors::{VmError, VmInternalError},
    vm::BexVm,
};

impl BexVm {
    /// The value of global slot `index` as the function `function` reads it
    /// (bytecode's `LoadGlobal`): a top-level `let`, initialized by its
    /// package's `$init` before any body runs.
    #[inline]
    #[must_use]
    pub fn load_global(&self, function: HeapPtr, index: usize) -> Value {
        let Object::Function(function) = self.get_object(function) else {
            unreachable!("compiled code names its own function object");
        };
        self.load_global_in(
            self.runtime_package_of(function),
            GlobalIndex::from_raw(index),
        )
    }

    /// `Package.current` in the package at `ordinal` (bytecode's
    /// `LoadCurrentPackage`, whose operand the linker wrote).
    #[inline]
    pub fn current_package(&mut self, ordinal: usize) -> Value {
        crate::package_reflect::reflect::current_package_value(self, ordinal)
    }

    /// The type a `ConstValue::Type` constant of `function` names, for a
    /// function with no frame type arguments (bytecode's `LoadType`, less the
    /// `type` object it allocates: compiled code wants the type itself).
    pub fn type_constant(
        &self,
        function: HeapPtr,
        index: usize,
    ) -> Result<RealizedTy, Box<VmError>> {
        let Object::Function(function) = self.get_object(function) else {
            unreachable!("compiled code names its own function object");
        };
        let ConstValue::Type(template) = &function.bytecode.constants[index] else {
            return Err(Box::new(VmInternalError::UnexpectedConstantKind.into()));
        };
        template.substitute(&[], self).map_err(|error| {
            Box::new(
                VmInternalError::TypeSubstitution {
                    message: error.to_string(),
                }
                .into(),
            )
        })
    }

    /// `value is T` for the type constant `index` of `function` (a structural
    /// template, a literal type, or a type tag), in a function with no frame
    /// type arguments: bytecode's `IsType`.
    pub fn is_type_constant(
        &self,
        function: HeapPtr,
        index: usize,
        value: Value,
    ) -> Result<bool, Box<VmError>> {
        let Object::Function(function) = self.get_object(function) else {
            unreachable!("compiled code names its own function object");
        };
        let raw = &function.bytecode.constants[index];
        let resolved = function.bytecode.resolved_constants[index];
        self.value_matches_type_constant_with_type_args(&[], value, raw, resolved)
            .map_err(|error| Box::new(error.into()))
    }
}

#[cfg(test)]
mod tests {
    use baml_type::typetag;
    use bex_vm_types::{
        FunctionKind, GlobalPool, Object, ObjectIndex, RealizedTy, TyTemplate, Value, VmGlobals,
        bytecode::Bytecode, types::ConstValue,
    };

    use crate::vm::tests::{native_function_object, test_vm};

    /// A bytecode function whose pool holds `constants`, resolved as
    /// `resolved` (the loader resolves an `Int` to itself and leaves a type
    /// or literal constant `NULL`).
    fn function_with(constants: Vec<ConstValue>) -> Object {
        let Object::Function(mut function) = native_function_object() else {
            unreachable!()
        };
        let resolved = constants
            .iter()
            .map(|constant| match constant {
                ConstValue::Int(n) => Value::int(*n),
                _ => Value::NULL,
            })
            .collect();
        function.kind = FunctionKind::Bytecode;
        function.bytecode = Bytecode {
            constants,
            resolved_constants: resolved,
            ..Bytecode::default()
        };
        Object::Function(function)
    }

    #[test]
    fn a_type_constant_is_read_from_the_functions_pool() {
        let vm = test_vm(vec![function_with(vec![
            ConstValue::Int(1),
            ConstValue::Type(TyTemplate::List(Box::new(TyTemplate::String))),
        ])]);
        let function = vm.idx_to_ptr(ObjectIndex::from_raw(0));
        assert_eq!(
            vm.type_constant(function, 1).unwrap(),
            RealizedTy::List(Box::new(RealizedTy::String))
        );
        assert!(
            vm.type_constant(function, 0).is_err(),
            "not a type constant"
        );
    }

    #[test]
    fn a_type_test_decides_as_the_interpreters_is_type_does() {
        let mut vm = test_vm(vec![function_with(vec![
            ConstValue::Type(TyTemplate::List(Box::new(TyTemplate::Int))),
            ConstValue::Int(typetag::STRING),
            ConstValue::Literal(baml_base::Literal::Int(7)),
        ])]);
        let function = vm.idx_to_ptr(ObjectIndex::from_raw(0));
        let ints = vm.new_array(RealizedTy::Int, vec![Value::int(1)]);
        let strings = vm.new_array(RealizedTy::String, Vec::new());
        let text = vm.string_value("x");
        assert!(vm.is_type_constant(function, 0, ints).unwrap());
        assert!(!vm.is_type_constant(function, 0, strings).unwrap());
        assert!(vm.is_type_constant(function, 1, text).unwrap());
        assert!(!vm.is_type_constant(function, 1, ints).unwrap());
        assert!(vm.is_type_constant(function, 2, Value::int(7)).unwrap());
        assert!(!vm.is_type_constant(function, 2, Value::int(8)).unwrap());
    }

    #[test]
    fn a_global_is_read_from_the_programs_pool() {
        let mut vm = test_vm(vec![function_with(Vec::new())]);
        vm.globals = VmGlobals::Owned(GlobalPool::from_vec(vec![Value::int(3), Value::int(9)]));
        let function = vm.idx_to_ptr(ObjectIndex::from_raw(0));
        assert_eq!(vm.load_global(function, 1), Value::int(9));
    }
}
