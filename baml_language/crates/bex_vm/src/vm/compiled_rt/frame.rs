use crate::vm::*;

/// A log event queued for the engine in [`super::CompiledState::pending_logs`].
#[derive(Debug, Clone)]
pub struct PendingLog {
    pub level: btel_records::LogLevel,
    pub event_name: Option<Arc<str>>,
    pub data: Value,
    pub source_location: Option<VmEventSourceLocation>,
}

/// Compiled entry points for one program.
pub struct CompiledBodyTable {
    pub fingerprint: [u8; 32],
    pub entries: &'static [crate::compiled_rt::CompiledEntry],
}

/// Compiled bodies paired with the digest verified while decoding their artifact.
#[derive(Clone, Copy)]
pub struct CompiledBodies {
    pub table: &'static CompiledBodyTable,
    pub artifact_fingerprint: [u8; 32],
}

pub(crate) fn attach_compiled_bodies(
    objects: &mut [Object],
    table: &CompiledBodyTable,
    names: &[Option<String>],
) -> Result<(), VmInternalError> {
    for (entry, name) in table.entries.iter().zip(names) {
        let mismatch = |why: &str| VmInternalError::CompiledBody {
            message: format!(
                "compiled body for `{}` (object {}) {why}",
                entry.name, entry.object
            ),
        };
        let Some(Object::Function(function)) = objects.get_mut(entry.object as usize) else {
            return Err(mismatch("does not name a function in this program"));
        };
        let name = name.as_deref().unwrap_or_default();
        if name != entry.name || function.arity != entry.arity {
            return Err(mismatch(&format!(
                "was compiled from another program: object is `{name}` with arity {}",
                function.arity
            )));
        }
        if !matches!(function.kind, FunctionKind::Bytecode) {
            return Err(mismatch("targets a function that is not bytecode"));
        }
        function.compiled_body = Some(entry.erase());
    }
    Ok(())
}

/// Type values required when any argument names a runtime declaration.
pub(crate) fn exact_type_values(type_args: &[bex_vm_types::RealizedTy]) -> Vec<Option<TypeValue>> {
    if type_args
        .iter()
        .all(crate::reachable::is_statically_declared)
    {
        return Vec::new();
    }
    type_args
        .iter()
        .map(|ty| Some(TypeValue::new(ty.clone())))
        .collect()
}

/// A frame position returned by [`BexVm::frame_position`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramePosition {
    /// The next bytecode instruction.
    Bytecode(usize),
    /// A compiled body on the Rust stack, or not yet started.
    CompiledRunning,
    /// A compiled body suspended at an offset into its function's stub.
    CompiledStub(usize),
}

impl BytecodeFrame {
    /// Create a running compiled frame whose locals live on the Rust stack.
    pub(crate) fn compiled(
        function: HeapPtr,
        locals_offset: StackIndex,
        type_args: Vec<bex_vm_types::RealizedTy>,
        context: Context,
        telemetry: Option<FrameTelemetry>,
    ) -> Self {
        Self {
            function,
            instruction_ptr: crate::compiled_rt::RUNNING,
            locals_offset,
            type_args,
            type_metadata: None,
            faulting_pc: 0,
            telemetry,
            context,
        }
    }
}

impl BexVm {
    /// Return the caught value and context after unwinding to `depth`.
    ///
    /// Internal errors propagate without unwinding.
    pub(crate) fn land_compiled_catch(
        &mut self,
        error: Box<VmError>,
        depth: usize,
    ) -> Result<(Value, Value), Box<VmError>> {
        let VmError::Thrown(thrown) = *error else {
            return Err(error);
        };
        let value = thrown.value;
        let outcome = self.telemetry_outcome_for_exception(value);
        // Capture the trace before removing any frames between the throw and handler.
        let (mut context, _) = self
            .in_flight_context(thrown.context, value)
            .map_err(Box::new)?;
        while self.frames.len() > depth {
            let top = self.frames.len() - 1;
            // SAFETY: compiled frames hold functions from the compile-time heap;
            // see `load_function`.
            let function = unsafe { self.load_function(top) }
                .map_err(|error| Box::new(VmError::InternalError(error)))?;
            let recorded = if self.frame_wants_error_capture(top, function) {
                self.recorded_context(&mut context, value)
            } else {
                value
            };
            self.complete_unwound_frame(top, function, outcome, recorded, None);
            if let Some(Frame::Bytecode(frame)) = self.frames.pop() {
                self.stack.truncate(frame.locals_offset.raw());
            }
        }
        let ctx = match context {
            InFlightContext::Carried(context) => context,
            InFlightContext::Fresh { trace, cause } => {
                self.alloc_error_context(value, &trace, cause)
            }
        };
        Ok((value, ctx))
    }

    /// Resume a compiled body at its stub offset, retaining its frame on error.
    #[cold]
    #[inline(never)]
    pub(crate) fn resume_compiled(
        &mut self,
        frame_idx: &mut usize,
        function: &mut &'static Function,
    ) -> Result<Option<VmExecState>, VmError> {
        use crate::compiled_rt::{Resume, stub};
        let (Some(start), Some(body)) = (
            crate::compiled_rt::stub_start(function),
            crate::compiled_rt::compiled_fn(function),
        ) else {
            return Err(VmInternalError::CompiledBody {
                message: format!("`{}` resumed without a compiled body", function.name()),
            }
            .into());
        };
        let offset = self.cur_pc - start;
        if let Frame::Bytecode(frame) = &mut self.frames[*frame_idx] {
            frame.instruction_ptr = crate::compiled_rt::RUNNING;
        }
        match body {
            // Inline calls execute in the call path; only entry points reach here.
            CompiledFn::Inline(body) => {
                let Frame::Bytecode(frame) = &self.frames[*frame_idx] else {
                    unreachable!("a compiled frame is a bytecode frame");
                };
                let locals_offset = frame.locals_offset.raw();
                let args: SmallVec<[Value; 4]> = (0..function.arity)
                    .map(|i| self.stack.get_at(StackIndex::from_raw(locals_offset + i)))
                    .collect();
                let outcome = self.with_stack_room(|vm| body(vm, &args));
                self.settle();
                self.complete_compiled(frame_idx, function, outcome.map_err(|error| *error))
            }
            CompiledFn::Resumable(body) => {
                // Results arrive on the stack; errors arrive through `delivery`.
                let resume = match offset {
                    stub::START => Resume::Start,
                    stub::AFTER_YIELD => Resume::Yielded,
                    _ => match self.compiled.delivery.take() {
                        Some(error) => Resume::Thrown(error),
                        None => Resume::Value(self.stack.ensure_pop()),
                    },
                };
                let step = body(self, resume);
                self.settle();
                self.carry_out(step.map_err(|error| *error)?, start, frame_idx, function)
            }
        }
    }

    /// Handle a resumable body's request and park its frame at the resume offset.
    pub(crate) fn carry_out(
        &mut self,
        step: Step,
        start: usize,
        frame_idx: &mut usize,
        function: &mut &'static Function,
    ) -> Result<Option<VmExecState>, VmError> {
        use crate::compiled_rt::stub;
        let park = |vm: &mut Self, frame_idx: usize, offset: usize| {
            if let Frame::Bytecode(frame) = &mut vm.frames[frame_idx] {
                frame.instruction_ptr = start + offset;
            }
        };
        match step {
            Step::Return(value) => self.complete_compiled(frame_idx, function, Ok(value)),
            Step::Call { callee, argc } => {
                park(self, *frame_idx, stub::AFTER_CALL);
                let mut argc = argc as usize;
                let mut args_offset = self.stack.len() - argc;
                // The call path expects the receiver before the explicit arguments.
                // It takes the receiver's type arguments from the bound method.
                if let Object::BoundMethod(method) = self.get_object(callee) {
                    let receiver = method.receiver;
                    self.stack.insert(args_offset, receiver);
                    argc += 1;
                    args_offset = self.stack.len() - argc;
                }
                let locals_offset = StackIndex::from_raw(args_offset);
                let mut type_args = std::mem::take(&mut self.compiled.call_type_args);
                let outcome = self.compiled_caller_call(
                    callee,
                    locals_offset,
                    argc,
                    &type_args,
                    frame_idx,
                    function,
                );
                type_args.clear();
                self.compiled.call_type_args = type_args;
                outcome
            }
            // The stub executes the operation before resuming the compiled body.
            Step::Await(operand) | Step::AwaitAny(operand) | Step::Spawn(operand) => {
                let offset = match step {
                    Step::Await(_) => stub::AWAIT,
                    Step::AwaitAny(_) => stub::AWAIT_ANY,
                    _ => stub::SPAWN,
                };
                self.stack.push(operand);
                park(self, *frame_idx, offset);
                Ok(None)
            }
            Step::Yield => {
                park(self, *frame_idx, stub::AFTER_YIELD);
                Ok(Some(VmExecState::EarlyYield))
            }
        }
    }

    /// Complete a compiled frame with its result on the stack.
    pub(crate) fn complete_compiled(
        &mut self,
        frame_idx: &mut usize,
        function: &mut &'static Function,
        outcome: Result<Value, VmError>,
    ) -> Result<Option<VmExecState>, VmError> {
        let value = outcome?;
        self.stack.push(value);
        self.finish_bytecode_frame(frame_idx, function, value)
    }

    /// Call through the VM with the compiled caller's recorded call site.
    pub(crate) fn compiled_caller_call(
        &mut self,
        callee: HeapPtr,
        locals_offset: StackIndex,
        argc: usize,
        type_args: &[bex_vm_types::RealizedTy],
        frame_idx: &mut usize,
        function: &mut &'static Function,
    ) -> Result<Option<VmExecState>, VmError> {
        let caller = *frame_idx;
        let pc = match &self.frames[caller] {
            Frame::Bytecode(frame) => frame.faulting_pc,
            Frame::Native(_) => unreachable!("a compiled caller has a bytecode frame"),
        };
        // `cur_pc` belongs to the last bytecode execution. The call path needs
        // the compiled caller's recorded instruction index instead.
        let saved_pc = std::mem::replace(&mut self.cur_pc, pc);
        let outcome = if type_args.is_empty() {
            self.execute_call_from_locals_offset(callee, locals_offset, argc, frame_idx, function)
        } else {
            let type_values = exact_type_values(type_args);
            self.execute_call_from_locals_offset_with_type_args(
                callee,
                locals_offset,
                argc,
                CallOptions {
                    type_args,
                    type_values: &type_values,
                },
                frame_idx,
                function,
            )
        };
        self.cur_pc = saved_pc;
        if let Some(Frame::Bytecode(frame)) = self.frames.get_mut(caller) {
            frame.faulting_pc = pc;
        }
        outcome
    }

    /// Return the compact bytecode offset corresponding to the compiled frame's recorded PC.
    #[must_use]
    pub fn compiled_pc(&self, idx: usize) -> Option<usize> {
        self.compiled_at(idx)?;
        let Frame::Bytecode(frame) = &self.frames[idx] else {
            return None;
        };
        let function = self.get_object(frame.function).as_callable().ok()?;
        function
            .bytecode
            .compact
            .as_ref()?
            .offset_of(frame.faulting_pc)
    }

    /// Identify compiled frames before interpreting their position fields as bytecode offsets.
    pub(crate) fn compiled_at(&self, idx: usize) -> Option<crate::compiled_rt::CompiledAt> {
        let Frame::Bytecode(frame) = &self.frames[idx] else {
            return None;
        };
        if frame.instruction_ptr == crate::compiled_rt::RUNNING {
            return Some(crate::compiled_rt::CompiledAt::Running);
        }
        let function = self.get_object(frame.function).as_callable().ok()?;
        crate::compiled_rt::compiled_at(frame.instruction_ptr, function)
    }

    /// Return whether the frame at `idx` runs a compiled body.
    #[must_use]
    pub fn frame_is_compiled(&self, idx: usize) -> bool {
        self.compiled_at(idx).is_some()
    }

    /// Return the frame's bytecode or compiled position, or `None` for a native continuation.
    #[must_use]
    pub fn frame_position(&self, idx: usize) -> Option<FramePosition> {
        let Frame::Bytecode(frame) = &self.frames[idx] else {
            return None;
        };
        Some(match self.compiled_at(idx) {
            None => FramePosition::Bytecode(frame.instruction_ptr),
            Some(crate::compiled_rt::CompiledAt::Running) => FramePosition::CompiledRunning,
            Some(crate::compiled_rt::CompiledAt::Suspended) => {
                let function = self.get_object(frame.function).as_callable().ok()?;
                let stub = crate::compiled_rt::stub_start(function)?;
                FramePosition::CompiledStub(frame.instruction_ptr - stub)
            }
        })
    }

    /// Return the error context used to chain errors raised in the current handler.
    pub(crate) fn compiled_cause(&self, idx: usize) -> Option<Value> {
        let Frame::Bytecode(frame) = &self.frames[idx] else {
            return None;
        };
        let function = self.get_object(frame.function).as_callable().ok()?;
        let slot = function.compiled_body?.cause_slot()?;
        let context = *self
            .stack
            .0
            .get(frame.locals_offset.raw() + slot as usize)?;
        (!context.is_null()).then_some(context)
    }

    /// Resolve a caller's call site across compiled frames and native continuations.
    pub(crate) fn caller_pc(
        &self,
        caller_frame_idx: usize,
        frame_idx: usize,
        call_site: usize,
    ) -> u32 {
        if self.compiled_at(caller_frame_idx).is_some() {
            let pc = self.compiled_pc(caller_frame_idx).unwrap_or(0);
            return u32::try_from(pc).unwrap_or(u32::MAX);
        }
        let Frame::Bytecode(caller) = &self.frames[caller_frame_idx] else {
            unreachable!("a native continuation retains its bytecode caller");
        };
        let pc = if caller_frame_idx == frame_idx {
            call_site
        } else {
            caller.faulting_pc
        };
        u32::try_from(pc).unwrap_or(u32::MAX)
    }

    /// Complete a frame and restore its caller without updating the run loop's `pc` or `code`.
    pub(crate) fn finish_bytecode_frame(
        &mut self,
        frame_idx: &mut usize,
        function: &mut &'static Function,
        result: Value,
    ) -> Result<Option<VmExecState>, VmError> {
        let Frame::Bytecode(bf) = &mut self.frames[*frame_idx] else {
            unreachable!("finish_bytecode_frame on a native frame")
        };
        let locals_offset = bf.locals_offset;
        let frame_telemetry = bf.telemetry.take();

        // Complete telemetry before replacing the callee's `Function` with the caller's.
        if let Some(frame_telemetry) = frame_telemetry {
            self.complete_bytecode_invocation_with_function(
                *frame_idx,
                function,
                frame_telemetry,
                InvocationOutcome::Ok,
                Some(result),
            );
        } else {
            self.end_manual_activation(*frame_idx);
        }

        // SAFETY: the compiler guarantees a result at or above the locals base,
        // so the base slot is initialized even when there are no arguments or locals.
        // Value is Copy. No allocation or GC occurs before the stack is shortened.
        let consumed = self.stack.len() - locals_offset.raw();
        unsafe { self.stack.replace_top_n_dynamic(consumed, result) };
        // SAFETY: the frame accessed above is the one being completed; stack
        // operations cannot remove it.
        unsafe { self.frames.no_return_pop() };
        let context = self.current_context().clone();
        if let Some(telemetry) = &mut self.telemetry {
            telemetry.set_context(context);
        }
        if self.frames.is_empty() {
            return Ok(Some(VmExecState::Complete(self.stack.ensure_pop())));
        }
        *frame_idx = self.frames.len() - 1;
        if matches!(self.frames[*frame_idx], Frame::Bytecode(_)) {
            // SAFETY: the caller's function object lives in the compile-time
            // heap, which is never collected; see `load_function`.
            *function = unsafe { self.load_function(*frame_idx)? };
        }
        Ok(None)
    }

    /// Bind pending type arguments before an inline body runs inside the call path.
    #[cold]
    #[inline(never)]
    pub(crate) fn bind_passed_type_args(&mut self, frame_idx: usize) {
        let Frame::Bytecode(frame) = &mut self.frames[frame_idx] else {
            unreachable!("a compiled frame is a bytecode frame");
        };
        if frame.type_args.capacity() == 0
            && let Some(pooled) = self.compiled.type_arg_pool.pop()
        {
            frame.type_args = pooled;
        }
        append_passed_type_args(
            frame,
            &self.pending_call_type_args,
            &self.pending_call_type_values,
        );
    }

    /// Return a compiled frame's type-argument vector to the pool before popping it.
    #[cold]
    #[inline(never)]
    pub(crate) fn recycle_frame_type_args(&mut self, frame_idx: usize) {
        let Frame::Bytecode(frame) = &mut self.frames[frame_idx] else {
            unreachable!("a compiled frame is a bytecode frame");
        };
        let type_args = std::mem::take(&mut frame.type_args);
        self.recycle_type_args(type_args);
    }
}
