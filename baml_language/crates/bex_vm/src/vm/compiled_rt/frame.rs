use crate::vm::*;

/// A `log.*` compiled code emitted; see [`CompiledState::pending_logs`](super::CompiledState::pending_logs). The fields
/// are those of [`VmExecState::Log`].
#[derive(Debug, Clone)]
pub struct PendingLog {
    pub level: btel_records::LogLevel,
    pub event_name: Option<Arc<str>>,
    pub data: Value,
    pub source_location: Option<VmEventSourceLocation>,
}

/// Compiled bodies for one program, as the BAML-to-Rust compiler
/// emits them; see [`super::CompiledEntry`].
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

/// The exact values a bytecode call carries beside `type_args`
/// (`take_type_args`): none when every argument names only compiled
/// declarations, which every consumer finds from the program; each argument
/// as its own value when one names a runtime declaration. A compiled caller
/// has its type arguments as realized types, and a `type` value is exactly
/// its realized type, so the two lanes carry the same content.
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

/// Where a frame stands, for a reader outside the interpreter
/// (`BexVm::frame_position`): a bytecode position, or where its compiled
/// body is (see `crate::compiled_rt::stub`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramePosition {
    /// The next bytecode instruction.
    Bytecode(usize),
    /// A compiled body on the Rust stack, or not yet started.
    CompiledRunning,
    /// A compiled body suspended on a request, at this offset into its
    /// function's stub.
    CompiledStub(usize),
}

impl BytecodeFrame {
    /// The frame of a compiled-to-compiled call: the call's type arguments
    /// (a pooled vector, or an empty one for a non-generic call) and no
    /// locals region (compiled code keeps its locals in Rust), its body
    /// running. `telemetry` is `None` with telemetry off or for a hidden
    /// invocation, else what `enter_bytecode_with_trace` returned for it.
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
    /// Land a thrown error in a compiled function's `catch`: build its
    /// context exactly as the unwinder would (with every frame between the
    /// throw and the handler still on the stack, so the trace is complete),
    /// then pop the frames compiled callees left pushed above the handling
    /// function's own frame, completing their telemetry with the error
    /// outcome. `depth` is the frame count with the handling function's frame
    /// on top. Returns the caught value and its `baml.errors.Context`.
    ///
    /// Internal errors are not catchable and come back as `Err`.
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
        let (mut context, _) = self
            .in_flight_context(thrown.context, value)
            .map_err(Box::new)?;
        while self.frames.len() > depth {
            let top = self.frames.len() - 1;
            // SAFETY: see `load_function`; compiled frames hold functions from
            // the compile-time heap.
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

    /// `ResumeCompiled` (at `cur_pc`): hand the frame on top to its compiled
    /// body, as the stub offset it is parked at says (see
    /// `crate::compiled_rt::stub`), and carry out what the body returns,
    /// parking the frame again when it stays on top. On `Err` the frame stays
    /// pushed for the unwinder.
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
            // Only an entry point starts an inline body here; a call runs it
            // inside the call path.
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
                // A request leaves its result on the stack; the unwinder
                // leaves an error here instead.
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

    /// Carry out the request a resumable body returned, parking its frame at
    /// the stub offset that resumes it.
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
                // A bound method's receiver joins the pushed arguments as the
                // leading slot, as `CallIndirect` inserts it; the call path
                // resolves the inner function and seeds the receiver's type
                // arguments from the bound method itself.
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
            // The interpreter's own instruction, on this frame, then
            // `ResumeCompiled` with its result.
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

    /// Complete the compiled frame at `frame_idx` with a body's outcome, as
    /// `Return` does: the result goes on the stack first, where the epilogue
    /// expects it even for a function with no slots.
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

    /// A call made by the compiled frame at `*frame_idx` through the VM's
    /// call path, with its `argc` arguments at `locals_offset`. The path
    /// records the caller's call site from `cur_pc` in its `faulting_pc`,
    /// where a compiled frame keeps the index of its call instruction; this
    /// hands it that index and puts it back.
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
        // A compiled caller has no live `cur_pc` of its own (it holds
        // whatever bytecode last ran), so hand the call path the call
        // instruction the caller recorded as its call site.
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

    /// The compact PC the compiled frame at `idx` stands at: the instruction
    /// of its function's bytecode equivalent to where its body last reported
    /// (`BexVm::note_pc`), which the body names by index. A reader takes it
    /// as a bytecode frame's position: an outer frame's call instruction, or
    /// the instruction that raised. `None` for any other frame.
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

    /// Where the compiled body of the frame at `idx` stands; `None` for a
    /// frame whose bytecode runs, or a native continuation. Every reader of
    /// a frame's position asks this first: a compiled frame's position fields
    /// hold its stub offset and original bytecode PC, not a compact bytecode PC.
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

    /// Whether a compiled body runs the frame at `idx`: its
    /// position fields then hold no bytecode position (see
    /// `crate::compiled_rt::stub`).
    #[must_use]
    pub fn frame_is_compiled(&self, idx: usize) -> bool {
        self.compiled_at(idx).is_some()
    }

    /// Where the frame at `idx` stands; `None` for a native continuation.
    /// The one way out of the interpreter to a frame's position: it asks
    /// `Self::compiled_at` first, so a compiled frame's stub offset is
    /// never read as a bytecode position.
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

    /// The context of the error the compiled frame at `idx` is handling, from
    /// its cause slot: what a raise during that handling chains onto.
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

    /// The call-site offset a call path records for a call made from the
    /// frame at `caller_frame_idx`: the live `call_site` when that frame is
    /// the one calling, its recorded `faulting_pc` when a native continuation
    /// sits between, and the call instruction it recorded for a compiled
    /// caller ([`Self::compiled_pc`]).
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

    /// Complete the bytecode frame at `frame_idx` with `result`: the shared
    /// epilogue of `OpCode::Return` and of a compiled body.
    ///
    /// Finishes the frame's telemetry record, replaces the frame's stack region
    /// with `result`, pops the frame, restores the caller's context, and selects
    /// the caller into `frame_idx`/`function`. Never touches the run loop's
    /// `pc`/`code` locals; the loop-side `Return` does that itself, and the
    /// call funnel returns to a loop that still holds the caller's. Returns
    /// `Some(Complete)` when this was the last frame.
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

        // Completion reads the callee's `Function`; it must run before
        // `function` is reloaded with the caller's. An unobserved frame still
        // ends its held trace and span activation, as `Return` ends them.
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

        // SAFETY: the compiler guarantees a result at or above the callee's
        // locals base. That base is therefore an existing initialized slot,
        // even for a zero-argument function with no locals. Value is Copy, so
        // discarded slots need no drop. No allocation or GC point occurs
        // between writing the result and shortening the stack to the caller's
        // new top.
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
        // Select the restored caller. Native continuations and early yields
        // still hand this frame back to the outer loop.
        *frame_idx = self.frames.len() - 1;
        if matches!(self.frames[*frame_idx], Frame::Bytecode(_)) {
            // SAFETY: the caller's function object lives in the compile-time
            // heap, which is never collected; see `load_function`.
            *function = unsafe { self.load_function(*frame_idx)? };
        }
        Ok(None)
    }

    /// Bind the type arguments of the call being made (`pending_call_type_args`
    /// and the exact values beside them) onto the frame at `frame_idx`, as
    /// the writeback in `execute_call_from_locals_offset_with_type_args` does
    /// once the funnel returns: for an inline compiled body, which runs
    /// inside the funnel and reads its frame.
    #[cold]
    #[inline(never)]
    pub(crate) fn bind_passed_type_args(&mut self, frame_idx: usize) {
        let Frame::Bytecode(frame) = &mut self.frames[frame_idx] else {
            unreachable!("a compiled frame is a bytecode frame");
        };
        // A frame that carried nothing holds an empty, unallocated vector;
        // a pooled one takes its place and returns when the body completes.
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

    /// Return the type-argument vector of the compiled frame at `frame_idx`
    /// to the pool, before the frame pops.
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
