//! Captured locals share cells between the enclosing scope and its closures.

use bex_vm_types::{HeapPtr, Object, ObjectType, Value, types::Cell};

use super::{CompiledFn, FrameRecord};
use crate::{
    errors::{VmError, VmInternalError},
    vm::BexVm,
};

impl BexVm {
    #[inline]
    pub fn new_cell(&mut self, value: Value) -> Value {
        Value::object(self.tlab.alloc(Object::Cell(Cell::new(value))))
    }

    #[inline]
    pub fn cell_load(&self, cell: Value) -> Result<Value, Box<VmError>> {
        let (_, cell) = self.cell_of(cell)?;
        Ok(cell.load())
    }

    #[inline]
    pub fn cell_store(&mut self, cell: Value, value: Value) -> Result<(), Box<VmError>> {
        let (ptr, _) = self.cell_of(cell)?;
        self.heap.write_barrier(ptr, value);
        let (_, cell) = self.cell_of(cell)?;
        cell.store(value);
        Ok(())
    }

    #[inline]
    pub fn capture_cell(&self, frame: &FrameRecord, index: usize) -> Result<Value, Box<VmError>> {
        match self.get_object(frame.function()) {
            Object::Closure(closure) => closure.captures.get(index).copied().ok_or_else(|| {
                Box::new(
                    VmInternalError::CompiledBody {
                        message: format!(
                            "compiled lambda read capture {index} of a closure with {}",
                            closure.captures.len()
                        ),
                    }
                    .into(),
                )
            }),
            other => Err(Box::new(
                VmInternalError::TypeError {
                    expected: ObjectType::Closure.into(),
                    got: ObjectType::of(other).into(),
                }
                .into(),
            )),
        }
    }

    #[inline]
    fn cell_of(&self, cell: Value) -> Result<(HeapPtr, &Cell), Box<VmError>> {
        let ptr = self.as_object_ptr(cell, ObjectType::Cell)?;
        match self.get_object(ptr) {
            Object::Cell(cell) => Ok((ptr, cell)),
            other => Err(Box::new(
                VmInternalError::TypeError {
                    expected: ObjectType::Cell.into(),
                    got: ObjectType::of(other).into(),
                }
                .into(),
            )),
        }
    }

    #[inline]
    #[must_use]
    pub fn closure_compiled_body(&self, callee: HeapPtr, argc: usize) -> Option<CompiledFn> {
        let function = match self.get_object(callee) {
            Object::Closure(closure) => closure.function,
            Object::Function(_) => callee,
            _ => return None,
        };
        let Object::Function(function) = self.get_object(function) else {
            return None;
        };
        if function.arity != argc || function.trace_passthrough {
            return None;
        }
        super::compiled_fn(function)
    }
}

#[cfg(test)]
mod tests {
    use bex_vm_types::{RealizedTy, StackIndex, Value};

    use crate::{
        errors::VmError,
        vm::{BytecodeFrame, Frame},
    };

    #[test]
    fn a_cell_holds_what_was_last_stored() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let cell = vm.new_cell(Value::int(1));
        assert_eq!(vm.cell_load(cell).unwrap(), Value::int(1));
        vm.cell_store(cell, Value::int(2)).unwrap();
        assert_eq!(vm.cell_load(cell).unwrap(), Value::int(2));
        // A new loop iteration gets a separate binding with the carried value.
        let carried = vm.cell_load(cell).unwrap();
        let fresh = vm.new_cell(carried);
        vm.cell_store(fresh, Value::int(3)).unwrap();
        assert_eq!(vm.cell_load(cell).unwrap(), Value::int(2));
        assert_eq!(vm.cell_load(fresh).unwrap(), Value::int(3));
    }

    #[test]
    fn a_cell_holds_heap_values() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let text = vm.string_value("captured");
        let cell = vm.new_cell(Value::NULL);
        assert!(vm.cell_load(cell).unwrap().is_null());
        vm.cell_store(cell, text).unwrap();
        assert_eq!(vm.cell_load(cell).unwrap(), text);
    }

    #[test]
    fn a_non_cell_is_a_checker_hole() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let text = vm.string_value("not a cell");
        for value in [Value::int(1), Value::NULL, text] {
            assert!(matches!(
                *vm.cell_load(value).unwrap_err(),
                VmError::InternalError(_)
            ));
            assert!(matches!(
                *vm.cell_store(value, Value::int(1)).unwrap_err(),
                VmError::InternalError(_)
            ));
        }
    }

    #[test]
    fn a_closure_call_frame_exposes_its_captures() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let a = vm.new_cell(Value::int(10));
        let b = vm.new_cell(Value::int(20));
        let function = vm.string_value("stands in for the lambda");
        let function = function.as_object_ptr().expect("an object");
        let closure = vm.make_closure(function, vec![a, b], Box::new([RealizedTy::Int]));
        let closure = closure.as_object_ptr().expect("a closure is an object");
        for (callable, captures) in [(closure, &[a, b][..]), (function, &[][..])] {
            let frame = BytecodeFrame::compiled(
                callable,
                StackIndex::from_raw(vm.stack.len()),
                Vec::new(),
                vm.current_context().clone(),
                None,
            );
            vm.frames.push(Frame::Bytecode(frame));
            vm.run_entry_with(|vm, fr| -> Result<(), Box<VmError>> {
                for (index, expected) in captures.iter().enumerate() {
                    assert_eq!(vm.capture_cell(fr, index).unwrap(), *expected);
                }
                if !captures.is_empty() {
                    let second = vm.capture_cell(fr, 1).unwrap();
                    assert_eq!(vm.cell_load(second).unwrap(), Value::int(20));
                }
                assert!(matches!(
                    *vm.capture_cell(fr, captures.len()).unwrap_err(),
                    VmError::InternalError(_)
                ));
                Ok(())
            })
            .unwrap();
            vm.frames.pop();
        }
    }
}
