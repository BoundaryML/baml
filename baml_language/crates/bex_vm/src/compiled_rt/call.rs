//! Calls and the frame: builtins, compiled callees, the slots a resumable body
//! keeps, and what a body reports to the VM.

use bex_vm_types::{
    HeapPtr, Object, RealizedTy, StackIndex, Value,
    types::{ConstValue, Function},
};

use super::{CompiledBody, stack_address};
use crate::{
    errors::{VmError, VmInternalError, VmPanic},
    indexable::EvalStackTrait,
    package_baml::{NativeCallResult, NativeFunction},
    telemetry::{CallTypeArgs, InvocationOutcome},
    vm::{BexVm, BytecodeFrame, COMPILED_CALLER_PC, Frame, VmExecState},
};

#[cfg(test)]
thread_local! {
    /// Send every observed compiled-to-compiled call through the VM's call
    /// path instead of [`BexVm::call_compiled_observed`]'s inline entry: the
    /// records the two produce must not differ.
    pub(crate) static FORCE_CALL_FUNNEL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl BexVm {
    // ── Calls ─────────────────────────────────────────────────────────────────

    /// Call a VM-owned builtin synchronously. A builtin that yields (to run a
    /// callback) cannot be called from compiled code; the compiler admits only
    /// builtins that never do.
    pub fn call_builtin(
        &mut self,
        builtin: NativeFunction,
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        let outcome = builtin(self, args);
        self.settle();
        match outcome {
            NativeCallResult::Done(value) => Ok(value),
            NativeCallResult::Error(error) => Err(Box::new(self.native_error_to_vm_error(error))),
            NativeCallResult::YieldToCall { .. } => Err(Box::new(
                VmInternalError::CompiledBody {
                    message: "a builtin yielded inside compiled code".to_string(),
                }
                .into(),
            )),
        }
    }

    /// [`Self::call_builtin`] for a generic builtin (`Array.push`,
    /// `json.to_string`, …), with its type arguments where the interpreter
    /// puts them for a native callee: `current_call_type_args()`.
    pub fn call_builtin_typed(
        &mut self,
        builtin: NativeFunction,
        type_args: &[RealizedTy],
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        // Reuse the buffer's capacity: a `push` in a loop allocates nothing
        // here after the first call.
        let previous = std::mem::take(&mut self.pending_call_type_args);
        let mut buffer = std::mem::take(&mut self.compiled_type_args);
        buffer.clear();
        buffer.extend_from_slice(type_args);
        self.pending_call_type_args = buffer;
        let result = self.call_builtin(builtin, args);
        self.compiled_type_args = std::mem::replace(&mut self.pending_call_type_args, previous);
        result
    }

    /// Size the slot region of the compiled function whose frame count is
    /// `depth` to `slots` (a resumable body's arguments arrive in the first
    /// ones). Called once, on start; a resumable body's region then sits at
    /// the top of the stack whenever it suspends.
    pub fn reserve_slots(&mut self, depth: usize, slots: usize) {
        let crate::vm::Frame::Bytecode(frame) = &self.frames[depth - 1] else {
            unreachable!("a compiled body reserves slots in its own frame");
        };
        let end = frame.locals_offset.raw() + slots;
        self.stack.resize(end, Value::NULL);
    }

    /// Slot `index` of the compiled function whose frame count is `depth`.
    #[inline]
    #[must_use]
    pub fn slot(&self, depth: usize, index: usize) -> Value {
        let crate::vm::Frame::Bytecode(frame) = &self.frames[depth - 1] else {
            unreachable!("a compiled body reads its own frame");
        };
        self.stack
            .get_at(StackIndex::from_raw(frame.locals_offset.raw() + index))
    }

    /// Write slot `index` of the compiled function whose frame count is
    /// `depth`; see [`Self::slot`].
    #[inline]
    pub fn set_slot(&mut self, depth: usize, index: usize, value: Value) {
        let crate::vm::Frame::Bytecode(frame) = &self.frames[depth - 1] else {
            unreachable!("a compiled body writes its own frame");
        };
        let at = frame.locals_offset.raw() + index;
        self.stack.0[at] = value;
    }

    /// Push an argument of the call a resumable body is about to request
    /// ([`Step::Call`]); arguments sit above the body's slots, rooted.
    #[inline]
    pub fn push_arg(&mut self, value: Value) {
        self.stack.push(value);
    }

    /// Push a type argument of the call a resumable body is about to request
    /// ([`Step::Call`]), into a buffer the VM reuses: a generic call
    /// allocates nothing for them.
    #[inline]
    pub fn push_type_arg(&mut self, ty: RealizedTy) {
        self.call_type_args.push(ty);
    }

    /// The value of `future` when it has settled with one and awaiting it
    /// would not poll for a yield: `await`'s fast path, with no suspension.
    /// `None` sends the body through the interpreter's own `Await`
    /// ([`Step::Await`]), which raises, parks or yields exactly as bytecode
    /// does. Counts the `Await`'s control-flow check, as bytecode does.
    #[inline]
    pub fn ready(&mut self, future: Value) -> Option<Value> {
        if self.early_yield.is_due() {
            return None;
        }
        let value = match self.get_object(future.as_object_ptr()?) {
            Object::Future(future) => match future.read() {
                bex_vm_types::FutureRead::Ready(value) => value,
                _ => return None,
            },
            _ => return None,
        };
        self.early_yield.tick();
        Some(value)
    }

    /// Whether a resumable body should yield ([`Step::Yield`]) at a loop back
    /// edge: the interpreter's control-flow check.
    #[inline]
    pub fn should_yield(&mut self) -> bool {
        self.should_early_yield()
    }

    /// The frame count with the running compiled function's frame on top.
    /// A compiled body reads it on entry and hands it to [`Self::catch`].
    #[inline]
    #[must_use]
    pub fn frame_depth(&self) -> usize {
        self.frames.len()
    }

    /// Record `line` as the source position of the compiled function whose
    /// frame count is `depth` (see [`Self::frame_depth`]). Compiled code calls
    /// this only when an error leaves the function, lands in its `catch`, or
    /// it suspends, so stack traces name the statement that raised, at no
    /// cost otherwise. A compiled frame keeps its line in `faulting_pc`; see
    /// `stub`.
    /// Record `line` as the running compiled function's call site, in the
    /// top frame (the caller's, before it pushes its callee); see
    /// [`Self::call_compiled`].
    #[inline(always)]
    pub(crate) fn note_call_line(&mut self, line: u32) {
        if let Some(crate::vm::Frame::Bytecode(frame)) = self.frames.last_mut() {
            frame.faulting_pc = line as usize;
        }
    }

    #[cold]
    pub fn note_line(&mut self, depth: usize, line: u32) {
        if let Some(crate::vm::Frame::Bytecode(frame)) = depth
            .checked_sub(1)
            .and_then(|top| self.frames.get_mut(top))
        {
            frame.faulting_pc = line as usize;
        }
    }

    /// Land a thrown error in a compiled `catch`; see
    /// `BexVm::land_compiled_catch`. Returns the caught value and its
    /// `baml.errors.Context`; an internal error keeps propagating.
    #[inline]
    pub fn catch(
        &mut self,
        error: Box<VmError>,
        depth: usize,
    ) -> Result<(Value, Value), Box<VmError>> {
        self.land_compiled_catch(error, depth)
    }

    /// `unreachable` reached in compiled code: the catchable
    /// `baml.panics.Unreachable` bytecode's `Unreachable` throws.
    #[cold]
    #[inline(never)]
    pub fn unreachable_panic(&mut self) -> Box<VmError> {
        Box::new(VmError::thrown_fresh(
            self.panic_to_exception_value(VmPanic::Unreachable),
        ))
    }

    /// Call a compiled function from compiled code.
    ///
    /// Every call gets a frame, as in bytecode: it counts toward the depth
    /// limit (a catchable `StackOverflow`, never a native stack overflow),
    /// names the callee in stack traces, and roots the callee. With telemetry
    /// off that frame is all the call costs: a struct push, then a direct
    /// Rust call LLVM can inline. With telemetry on the call goes through the
    /// VM's call path, so records, call paths and `$trace` behave exactly as
    /// for a bytecode call.
    ///
    /// The frame limit is the only limit a program sees: the Rust stack the
    /// callee runs on is the VM's concern ([`Self::with_stack_room`]).
    ///
    /// `line` is the call's source line. It goes into the caller's frame
    /// before the callee runs, as the interpreter records a caller's call
    /// site on every call: a stack trace taken while the callee runs (an
    /// error caught below, a `StackOverflow`) names the line of this call.
    ///
    /// On `Err` the callee's frame stays pushed, as the VM's call path leaves
    /// it: the unwinder pops it if the error escapes, [`Self::catch`] if a
    /// compiled handler lands it.
    #[inline]
    pub fn call_compiled(
        &mut self,
        callee: HeapPtr,
        body: CompiledBody,
        args: &[Value],
        line: u32,
    ) -> Result<Value, Box<VmError>> {
        self.note_call_line(line);
        if self.frames.len() >= crate::vm::MAX_FRAMES {
            return Err(Box::new(VmError::thrown_fresh(
                self.panic_to_exception_value(VmPanic::StackOverflow),
            )));
        }
        self.with_stack_room(|vm| vm.call_compiled_here(callee, body, args))
    }

    /// [`Self::call_compiled`] on the Rust stack where it stands.
    #[inline]
    fn call_compiled_here(
        &mut self,
        callee: HeapPtr,
        body: CompiledBody,
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        if self.telemetry.is_some() {
            return self.call_compiled_cold(callee, body, args);
        }
        let context = self.current_context().clone();
        let locals_offset = StackIndex::from_raw(self.stack.len());
        self.frames.push(crate::vm::Frame::Bytecode(
            crate::vm::BytecodeFrame::compiled(callee, locals_offset, Vec::new(), context, None),
        ));
        let result = body(self, args);
        if result.is_ok() {
            self.frames.pop();
            self.stack.truncate(locals_offset.raw());
        }
        result
    }

    /// [`Self::call_compiled`] for a generic callee: its frame starts with
    /// `type_args`, the call's type arguments in the callee's De Bruijn
    /// order (a plain function carries none of its own, so these are all of
    /// them). The frame's vector comes from the pool and returns to it when
    /// the frame pops, so the call allocates nothing in the steady state.
    #[inline]
    pub fn call_compiled_with_types(
        &mut self,
        callee: HeapPtr,
        body: CompiledBody,
        type_args: &[RealizedTy],
        args: &[Value],
        line: u32,
    ) -> Result<Value, Box<VmError>> {
        self.note_call_line(line);
        if self.frames.len() >= crate::vm::MAX_FRAMES {
            return Err(Box::new(VmError::thrown_fresh(
                self.panic_to_exception_value(VmPanic::StackOverflow),
            )));
        }
        self.with_stack_room(|vm| vm.call_compiled_with_types_here(callee, body, type_args, args))
    }

    /// [`Self::call_compiled_with_types`] on the Rust stack where it stands.
    #[inline]
    fn call_compiled_with_types_here(
        &mut self,
        callee: HeapPtr,
        body: CompiledBody,
        type_args: &[RealizedTy],
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        if self.telemetry.is_some() {
            return self.call_compiled_funnelled(callee, type_args, args);
        }
        let context = self.current_context().clone();
        let locals_offset = StackIndex::from_raw(self.stack.len());
        let mut frame_type_args = self.pooled_type_args();
        frame_type_args.extend_from_slice(type_args);
        let mut frame =
            crate::vm::BytecodeFrame::compiled(callee, locals_offset, frame_type_args, context, None);
        // A type naming a runtime declaration rides beside the frame's
        // arguments as an exact value, as a bytecode call carries it.
        let type_values = crate::vm::exact_type_values(type_args);
        if !type_values.is_empty() {
            frame.type_metadata = Some(Box::new(crate::vm::FrameTypeMetadata {
                values: type_values,
            }));
        }
        self.frames.push(crate::vm::Frame::Bytecode(frame));
        let result = body(self, args);
        if result.is_ok() {
            if let Some(crate::vm::Frame::Bytecode(frame)) = self.frames.pop() {
                self.recycle_type_args(frame.type_args);
            }
            self.stack.truncate(locals_offset.raw());
        }
        result
    }

    /// A spare type-argument vector: one the pool holds, or a new one.
    #[inline]
    pub(crate) fn pooled_type_args(&mut self) -> Vec<RealizedTy> {
        self.type_arg_pool.pop().unwrap_or_default()
    }

    /// Give a frame's type-argument vector back to the pool, cleared. The
    /// pool keeps a few: as many as calls nest in practice, and an
    /// unallocated vector is not worth keeping.
    #[inline]
    pub(crate) fn recycle_type_args(&mut self, mut type_args: Vec<RealizedTy>) {
        const POOL: usize = 16;
        if type_args.capacity() != 0 && self.type_arg_pool.len() < POOL {
            type_args.clear();
            self.type_arg_pool.push(type_args);
        }
    }

    /// [`Self::call_compiled`] through the VM's call path, for telemetry,
    /// with the call's type arguments recorded as a bytecode call's.
    /// Run `f`, which runs a compiled body, where the Rust stack has room for
    /// it: here when the stack is above the floor (one compare), else on a
    /// fresh segment. Every path that runs a compiled body on the Rust stack
    /// goes through this, so deep compiled recursion reaches the frame limit
    /// on any thread, never the thread's stack limit.
    #[inline]
    pub(crate) fn with_stack_room<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        if stack_address() < self.stack_floor {
            return self.on_fresh_segment(f);
        }
        f(self)
    }

    /// [`Self::with_stack_room`] below the floor: `f` runs on a fresh
    /// [`STACK_SEGMENT`](super::STACK_SEGMENT), with the floor set to that
    /// segment's while it does. The segment is mapped on entry and unmapped
    /// on return; the frames that fit in one are what make the cost cold.
    /// On wasm32 the floor is unknown (0), so this is never reached.
    #[cold]
    #[inline(never)]
    fn on_fresh_segment<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let floor = self.stack_floor;
            let result = stacker::grow(super::STACK_SEGMENT, || {
                self.stack_floor = super::stack_floor();
                f(self)
            });
            self.stack_floor = floor;
            result
        }
        #[cfg(target_arch = "wasm32")]
        {
            f(self)
        }
    }

    /// [`Self::call_compiled`] with telemetry on, as the branch
    /// [`Self::call_compiled`] does not expect: `cold` lays the direct path
    /// out as the fall-through without sizing the observed path's own code
    /// for coldness.
    #[cold]
    #[inline(never)]
    fn call_compiled_cold(
        &mut self,
        callee: HeapPtr,
        body: CompiledBody,
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        self.call_compiled_observed(callee, body, args)
    }

    /// [`Self::call_compiled`] with telemetry on.
    ///
    /// A plain compiled callee (a `Function`, not `$trace`-hooked, called
    /// without a `$trace` attachment or type arguments) is entered here with
    /// what the VM's call path would establish for it, but known statically:
    /// the call site is [`COMPILED_CALLER_PC`], the visible caller is the
    /// running compiled frame's function, the callee's context is inherited.
    /// The frame is pushed with the invocation `enter_bytecode_with_trace`
    /// opened, the body runs, and the frame completes as
    /// `finish_bytecode_frame` completes one: the records are those the call
    /// path would write, without its argument copy, callee dispatch and
    /// call-site bookkeeping. The context the call path selects on entry and
    /// restores on completion is the one already selected, the caller's, so
    /// it is reselected only when it is not. Every other callee goes through
    /// the call path ([`Self::call_compiled_funnelled`]).
    #[inline(never)]
    #[allow(unsafe_code)]
    pub(super) fn call_compiled_observed(
        &mut self,
        callee: HeapPtr,
        body: CompiledBody,
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        // SAFETY: a compiled callee is a function object in the compile-time
        // heap, which is never collected; see `load_function`.
        let function: &'static Function = match unsafe { callee.get() } {
            Object::Function(function) => function,
            _ => return self.call_compiled_funnelled(callee, &[], args),
        };
        let hooked = function
            .bytecode
            .compact
            .as_ref()
            .is_some_and(|code| code.trace_hook_finish_pc.is_some());
        #[cfg(test)]
        let forced = FORCE_CALL_FUNNEL.with(std::cell::Cell::get);
        #[cfg(not(test))]
        let forced = false;
        if hooked
            || forced
            || self.pending_call_trace.is_some()
            || self.pending_call_passes_type_args
        {
            return self.call_compiled_funnelled(callee, &[], args);
        }
        debug_assert_eq!(args.len(), function.arity);

        let top = self.frames.len() - 1;
        let caller_frame_idx = self.bytecode_caller_index(top);
        let visible_caller = self
            .visible_bytecode_caller_index(top)
            .and_then(|index| self.frame_function_identity(index));
        let caller_is_observed = matches!(
            self.frames.get(caller_frame_idx),
            Some(Frame::Bytecode(BytecodeFrame {
                telemetry: Some(_),
                ..
            }))
        );
        let context = self.current_context().clone();
        let telemetry = self
            .telemetry
            .as_mut()
            .expect("observed compiled call has telemetry");
        if !telemetry.has_context(&context) {
            telemetry.set_context(context.clone());
        }
        // SAFETY: the arguments and the callee stay live under the heap
        // permit: the caller holds them in its locals.
        let frame_telemetry = unsafe {
            telemetry.enter_bytecode_with_trace(
                function,
                callee,
                visible_caller,
                COMPILED_CALLER_PC,
                caller_is_observed,
                args,
                None,
                CallTypeArgs::default(),
                |caller, callee| Self::register_call_path_functions(&self.heap, caller, callee),
            )
        };
        let locals_offset = StackIndex::from_raw(self.stack.len());
        self.frames.push(Frame::Bytecode(BytecodeFrame::compiled(
            callee,
            locals_offset,
            Vec::new(),
            context,
            frame_telemetry,
        )));
        let result = body(self, args);
        if let Ok(value) = result {
            let Some(Frame::Bytecode(frame)) = self.frames.pop() else {
                unreachable!("a compiled callee has a bytecode frame");
            };
            self.stack.truncate(locals_offset.raw());
            if let Some(frame_telemetry) = frame.telemetry {
                let telemetry = self
                    .telemetry
                    .as_mut()
                    .expect("observed compiled call has telemetry");
                // The callee's context is the caller's: selected for the
                // completion and left selected for the caller.
                if !telemetry.has_context(&frame.context) {
                    telemetry.set_context(frame.context);
                }
                // SAFETY: completion holds the heap permit and the result
                // is live.
                unsafe {
                    telemetry.complete_invocation(
                        frame_telemetry,
                        function,
                        InvocationOutcome::Ok,
                        Some(value),
                    );
                }
            }
        }
        result
    }

    /// [`Self::call_compiled`] through the VM's call path, for a callee the
    /// inline observed entry does not cover (a generic, `$trace`-hooked or
    /// `$trace`-attached callee, a non-`Function` callee), with the call's
    /// type arguments recorded as a bytecode call's.
    #[cold]
    #[inline(never)]
    #[allow(unsafe_code)]
    fn call_compiled_funnelled(
        &mut self,
        callee: HeapPtr,
        type_args: &[RealizedTy],
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        let top = self.frames.len() - 1;
        let locals_offset = StackIndex::from_raw(self.stack.len());
        self.stack.extend(args.iter().copied());
        let mut frame_idx = top;
        // SAFETY: the running compiled frame's function object lives in the
        // compile-time heap, which is never collected; see `load_function`.
        let mut function = unsafe {
            self.load_function(top)
                .map_err(|error| Box::new(VmError::InternalError(error)))?
        };
        let outcome = self.compiled_caller_call(
            callee,
            locals_offset,
            args.len(),
            type_args,
            &mut frame_idx,
            &mut function,
        );
        match outcome {
            // The callee ran to completion inside the call path (its body is
            // compiled), so its result is on the stack. An `EarlyYield`
            // request is left armed for the enclosing loop.
            Ok(None | Some(VmExecState::EarlyYield)) if frame_idx == top => {
                Ok(self.stack.ensure_pop())
            }
            Ok(_) => Err(Box::new(
                VmInternalError::CompiledBody {
                    message: "a compiled call reached a callee without a compiled body".to_string(),
                }
                .into(),
            )),
            Err(error) => Err(Box::new(error)),
        }
    }

    // ── Type tests ────────────────────────────────────────────────────────────

    /// `value is T` against a type constant from the function's constant pool,
    /// with the caller's resolved generic type arguments (empty when the
    /// enclosing function is not generic).
    pub fn value_is_type(
        &self,
        type_args: &[RealizedTy],
        value: Value,
        raw_const: &ConstValue,
        resolved_const: Value,
    ) -> Result<bool, VmInternalError> {
        self.value_matches_type_constant_with_type_args(type_args, value, raw_const, resolved_const)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::AtomicBool};

    use bex_vm_types::RealizedTy;

    use crate::vm::BexVm;

    fn vm() -> BexVm {
        let program = baml_test_support::compile_source("function Main() -> int { 1 }");
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap()
    }

    /// A recycled vector comes back cleared with its capacity; an
    /// unallocated one is not kept, and the pool is bounded.
    #[test]
    fn type_arg_vectors_are_pooled_by_capacity() {
        let mut vm = vm();
        let mut first = vm.pooled_type_args();
        assert_eq!(first.capacity(), 0, "an empty pool hands out a new vector");
        first.extend_from_slice(&[RealizedTy::Int, RealizedTy::String]);
        let capacity = first.capacity();
        vm.recycle_type_args(first);
        vm.recycle_type_args(Vec::new());
        assert_eq!(vm.type_arg_pool.len(), 1);
        let again = vm.pooled_type_args();
        assert!(again.is_empty());
        assert_eq!(again.capacity(), capacity);
        assert!(vm.type_arg_pool.is_empty());
        vm.recycle_type_args(again);
        for _ in 0..40 {
            vm.recycle_type_args(Vec::with_capacity(4));
        }
        assert_eq!(vm.type_arg_pool.len(), 16);
    }
}
