//! Access to the compiled function's constant pool, globals, and package.

use bex_vm_types::{GlobalIndex, HeapPtr, Object, RealizedTy, Value, types::ConstValue};

use crate::{
    errors::{VmError, VmInternalError},
    vm::BexVm,
};

impl BexVm {
    /// Read a global, preserving the interpreter's initialization-order check.
    pub fn load_global_checked(
        &self,
        function: HeapPtr,
        index: usize,
    ) -> Result<Value, Box<VmError>> {
        let Object::Function(body) = self.get_object(function) else {
            unreachable!("compiled code names its own function object");
        };
        let slot = GlobalIndex::from_raw(index);
        let package = self.runtime_package_of(body);
        if package.as_ptr().is_null() && self.global_is_pending(slot) {
            return Err(Box::new(
                VmInternalError::GlobalReadBeforeInit { slot }.into(),
            ));
        }
        Ok(self.load_global_in(package, slot))
    }

    /// Store an initializer's result in its owning package's global cell.
    pub fn store_global(
        &mut self,
        function: HeapPtr,
        index: usize,
        value: Value,
    ) -> Result<(), Box<VmError>> {
        let Object::Function(function) = self.get_object(function) else {
            unreachable!("compiled code names its own function object");
        };
        let package = self.runtime_package_of(function);
        self.store_global_in(package, GlobalIndex::from_raw(index), value)
            .map_err(|error| Box::new(error.into()))?;
        if package.as_ptr().is_null()
            && let Some(pending) = self.pending_globals.as_mut()
            && let Some(slot) = pending.get_mut(index)
        {
            *slot = false;
        }
        Ok(())
    }

    /// Read a global initialized by the owning package's `$init`.
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

    /// Return `Package.current` for the linked package ordinal.
    #[inline]
    pub fn current_package(&mut self, ordinal: usize) -> Value {
        crate::package_reflect::reflect::current_package_value(self, ordinal)
    }

    /// Resolve a type constant without frame type arguments.
    pub fn type_constant(
        &self,
        function: HeapPtr,
        index: usize,
    ) -> Result<RealizedTy, Box<VmError>> {
        self.substitute_type_constant(&[], function, index)
    }

    /// Resolve a type constant using the type arguments of `frame`.
    pub fn frame_type_constant(
        &self,
        frame: &super::FrameRecord,
        function: HeapPtr,
        index: usize,
    ) -> Result<RealizedTy, Box<VmError>> {
        self.substitute_type_constant(self.frame_type_args(frame), function, index)
    }

    fn substitute_type_constant(
        &self,
        type_args: &[RealizedTy],
        function: HeapPtr,
        index: usize,
    ) -> Result<RealizedTy, Box<VmError>> {
        let Object::Function(function) = self.get_object(function) else {
            unreachable!("compiled code names its own function object");
        };
        let ConstValue::Type(template) = &function.bytecode.constants[index] else {
            return Err(Box::new(VmInternalError::UnexpectedConstantKind.into()));
        };
        template.substitute(type_args, self).map_err(|error| {
            Box::new(
                VmInternalError::TypeSubstitution {
                    message: error.to_string(),
                }
                .into(),
            )
        })
    }

    /// Test `value` against a type constant without frame type arguments.
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
        FunctionKind, GlobalIndex, GlobalPool, Object, ObjectIndex, RealizedTy, TyTemplate, Value,
        VmGlobals, bytecode::Bytecode, types::ConstValue,
    };

    use crate::{
        errors::{VmError, VmInternalError},
        vm::tests::{native_function_object, test_vm},
    };

    // The loader leaves type and literal constants unresolved.
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

    #[test]
    fn storing_an_initializer_clears_its_pending_global() {
        let mut vm = test_vm(vec![function_with(Vec::new())]);
        vm.globals = VmGlobals::Owned(GlobalPool::from_vec(vec![Value::NULL]));
        vm.pending_globals = Some(vec![true].into_boxed_slice());
        let function = vm.idx_to_ptr(ObjectIndex::from_raw(0));
        assert!(matches!(
            *vm.load_global_checked(function, 0).unwrap_err(),
            VmError::InternalError(VmInternalError::GlobalReadBeforeInit { .. })
        ));
        vm.store_global(function, 0, Value::int(42)).unwrap();
        assert_eq!(vm.load_global_checked(function, 0).unwrap(), Value::int(42));
        assert!(!vm.global_is_pending(GlobalIndex::from_raw(0)));
    }
}
