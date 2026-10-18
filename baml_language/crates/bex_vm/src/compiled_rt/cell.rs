//! Cells and captures: the storage of a local a closure captures, the
//! captures of the closure a compiled lambda body runs as, and a closure
//! call made from compiled code.
//!
//! A captured local holds an `Object::Cell`, allocated where the binding is
//! created (`MakeCell`) and shared with every closure that captures it; every
//! read or write of the binding goes through the cell (`LoadDeref` /
//! `StoreDeref` in the enclosing function, `LoadCapture` / `StoreCapture` in
//! a lambda, which finds the cell in its closure's capture array). Compiled
//! code keeps the cell pointer in a Rust local and composes the same
//! primitives.

use bex_vm_types::{HeapPtr, Object, ObjectType, StackIndex, Value, types::Cell};

use super::{CompiledBody, CompiledFn};
use crate::{
    errors::{VmError, VmInternalError, VmPanic},
    vm::{BexVm, Frame},
};

impl BexVm {
    /// A fresh cell holding `value`: bytecode's `MakeCell`.
    #[inline]
    pub fn new_cell(&mut self, value: Value) -> Value {
        Value::object(self.tlab.alloc(Object::Cell(Cell::new(value))))
    }

    /// The value in `cell`: `LoadDeref` / `LoadCapture`.
    #[inline]
    pub fn cell_load(&self, cell: Value) -> Result<Value, Box<VmError>> {
        let (_, cell) = self.cell_of(cell)?;
        Ok(cell.load())
    }

    /// Store `value` in `cell`: `StoreDeref` / `StoreCapture`, with the
    /// generational write barrier they apply.
    #[inline]
    pub fn cell_store(&mut self, cell: Value, value: Value) -> Result<(), Box<VmError>> {
        let (ptr, _) = self.cell_of(cell)?;
        self.heap.write_barrier(ptr, value);
        let (_, cell) = self.cell_of(cell)?;
        cell.store(value);
        Ok(())
    }

    /// The cell in capture `index` of the closure the compiled function whose
    /// frame count is `depth` runs: `CaptureRef`. Only a lambda body reads
    /// one, and a lambda is only ever entered through a closure over it,
    /// which its frame holds as its function.
    #[inline]
    pub fn capture_cell(&self, depth: usize, index: usize) -> Result<Value, Box<VmError>> {
        let Frame::Bytecode(frame) = &self.frames[depth - 1] else {
            unreachable!("a compiled body reads its own frame");
        };
        match self.get_object(frame.function) {
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

    /// `cell`'s pointer and object. A non-cell here is a checker hole, as
    /// the interpreter reports it.
    #[inline]
    fn cell_of(&self, cell: Value) -> Result<(HeapPtr, &Cell), Box<VmError>> {
        let Some(ptr) = cell.as_object_ptr() else {
            return Err(Box::new(
                VmInternalError::TypeError {
                    expected: ObjectType::Cell.into(),
                    got: self.type_of(&cell),
                }
                .into(),
            ));
        };
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

    // ── Closure calls ─────────────────────────────────────────────────────────

    /// The inline compiled body a call of the function value `callee` with
    /// `argc` arguments runs, when compiled code can make the call directly:
    /// a closure (or a plain function) whose function has an inline compiled
    /// body and takes exactly `argc` arguments. Any other callee — bytecode,
    /// a resumable body, a bound method, a generic function, a host closure,
    /// an arity the VM's call path reports — goes through [`Step::Call`].
    ///
    /// [`Step::Call`]: super::Step::Call
    #[inline]
    #[must_use]
    pub fn closure_inline_body(&self, callee: HeapPtr, argc: usize) -> Option<CompiledBody> {
        let function = match self.get_object(callee) {
            Object::Closure(closure) => closure.function,
            Object::Function(_) => callee,
            _ => return None,
        };
        let Object::Function(function) = self.get_object(function) else {
            return None;
        };
        if function.arity != argc {
            return None;
        }
        match super::native_body_fn(function)? {
            CompiledFn::Inline(body) => Some(body),
            CompiledFn::Resumable(_) => None,
        }
    }

    /// Call the function value `callee` (a closure, or a plain function)
    /// whose inline body [`Self::closure_inline_body`] found, from compiled
    /// code: [`Self::call_compiled`] with the frame a closure call gets, its
    /// function the closure itself (where the body's `capture_cell` finds the
    /// captures) and its type arguments the closure's.
    #[inline]
    pub fn call_closure_compiled(
        &mut self,
        callee: HeapPtr,
        body: CompiledBody,
        args: &[Value],
        pc: u32,
    ) -> Result<Value, Box<VmError>> {
        self.note_call_pc(pc);
        if self.frames.len() >= crate::vm::MAX_FRAMES {
            return Err(Box::new(VmError::thrown_fresh(
                self.panic_to_exception_value(VmPanic::StackOverflow),
            )));
        }
        self.with_stack_room(|vm| vm.call_closure_compiled_here(callee, body, args))
    }

    /// [`Self::call_closure_compiled`] on the Rust stack where it stands.
    #[inline]
    fn call_closure_compiled_here(
        &mut self,
        callee: HeapPtr,
        body: CompiledBody,
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        if self.telemetry.is_some() {
            return self.call_compiled_funnelled(callee, &[], args);
        }
        let context = self.current_context().clone();
        let locals_offset = StackIndex::from_raw(self.stack.len());
        let type_args = match self.get_object(callee) {
            Object::Closure(closure) => closure.captured_type_args.to_vec(),
            _ => Vec::new(),
        };
        let frame =
            crate::vm::BytecodeFrame::compiled(callee, locals_offset, type_args, context, None);
        self.frames.push(Frame::Bytecode(frame));
        let result = body(self, args);
        if result.is_ok() {
            self.frames.pop();
            self.stack.truncate(locals_offset.raw());
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use bex_vm_types::{RealizedTy, StackIndex, Value};

    use crate::{
        errors::VmError,
        vm::{BexVm, BytecodeFrame, Frame},
    };

    fn vm() -> BexVm {
        crate::vm::tests::test_vm(Vec::new())
    }

    #[test]
    fn a_cell_holds_what_was_last_stored() {
        let mut vm = vm();
        let cell = vm.new_cell(Value::int(1));
        assert_eq!(vm.cell_load(cell).unwrap(), Value::int(1));
        vm.cell_store(cell, Value::int(2)).unwrap();
        assert_eq!(vm.cell_load(cell).unwrap(), Value::int(2));
        // Two cells are two bindings: the enclosing scope and a closure share
        // one, a new iteration gets its own (`FreshCell` with `carry_value`).
        let carried = vm.cell_load(cell).unwrap();
        let fresh = vm.new_cell(carried);
        vm.cell_store(fresh, Value::int(3)).unwrap();
        assert_eq!(vm.cell_load(cell).unwrap(), Value::int(2));
        assert_eq!(vm.cell_load(fresh).unwrap(), Value::int(3));
    }

    #[test]
    fn a_cell_holds_heap_values() {
        let mut vm = vm();
        let text = vm.string_value("captured");
        let cell = vm.new_cell(Value::NULL);
        assert!(vm.cell_load(cell).unwrap().is_null());
        vm.cell_store(cell, text).unwrap();
        assert_eq!(vm.cell_load(cell).unwrap(), text);
    }

    /// A non-cell where the checker typed a cell is reported as the
    /// interpreter's internal type error, never read.
    #[test]
    fn a_non_cell_is_a_checker_hole() {
        let mut vm = vm();
        assert!(matches!(
            *vm.cell_load(Value::int(1)).unwrap_err(),
            VmError::InternalError(_)
        ));
        assert!(matches!(
            *vm.cell_store(Value::NULL, Value::int(1)).unwrap_err(),
            VmError::InternalError(_)
        ));
        let text = vm.string_value("not a cell");
        assert!(matches!(
            *vm.cell_load(text).unwrap_err(),
            VmError::InternalError(_)
        ));
    }

    /// A closure's captures are the cells it was made over, in order; a
    /// compiled lambda body reads them off its frame's function, which a
    /// closure call sets to the closure.
    #[test]
    fn a_closure_call_frame_exposes_its_captures() {
        let mut vm = vm();
        let a = vm.new_cell(Value::int(10));
        let b = vm.new_cell(Value::int(20));
        let function = vm.string_value("stands in for the lambda");
        let function = function.as_object_ptr().expect("an object");
        let closure = vm.make_closure(function, vec![a, b], Box::new([RealizedTy::Int]));
        let closure = closure.as_object_ptr().expect("a closure is an object");
        let context = vm.current_context().clone();
        let frame = BytecodeFrame::compiled(
            closure,
            StackIndex::from_raw(vm.stack.len()),
            Vec::new(),
            context,
            None,
        );
        vm.frames.push(Frame::Bytecode(frame));
        let depth = vm.frame_depth();
        assert_eq!(vm.capture_cell(depth, 0).unwrap(), a);
        assert_eq!(vm.capture_cell(depth, 1).unwrap(), b);
        let second = vm.capture_cell(depth, 1).unwrap();
        assert_eq!(vm.cell_load(second).unwrap(), Value::int(20));
        assert!(matches!(
            *vm.capture_cell(depth, 2).unwrap_err(),
            VmError::InternalError(_)
        ));
        // A frame running a plain function has no captures.
        let depth = depth - 1;
        vm.frames.pop();
        let frame = BytecodeFrame::compiled(
            function,
            StackIndex::from_raw(vm.stack.len()),
            Vec::new(),
            context_of(&vm),
            None,
        );
        vm.frames.push(Frame::Bytecode(frame));
        assert!(matches!(
            *vm.capture_cell(depth + 1, 0).unwrap_err(),
            VmError::InternalError(_)
        ));
        vm.frames.pop();
    }

    fn context_of(vm: &BexVm) -> btel_types::context::Context {
        vm.current_context().clone()
    }
}
