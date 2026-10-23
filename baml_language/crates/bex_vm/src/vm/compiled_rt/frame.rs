use crate::{
    compiled_rt::FrameAt,
    vm::{error_evidence::UnwindEvidence, *},
};

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

// Call-site type arguments follow those carried by the callee.
pub(in crate::vm) fn append_passed_type_args(
    frame: &mut BytecodeFrame,
    type_args: &[bex_vm_types::RealizedTy],
    type_values: &[Option<TypeValue>],
) {
    let initial_type_arg_count = frame.type_args.len();
    frame.type_args.extend_from_slice(type_args);
    if !type_values.is_empty() {
        let metadata = frame
            .type_metadata
            .get_or_insert_with(|| Box::new(FrameTypeMetadata::default()));
        metadata.values.resize(initial_type_arg_count, None);
        metadata
            .values
            .extend((0..type_args.len()).map(|slot| type_values.get(slot).cloned().flatten()));
    }
}

pub(in crate::vm) fn validate_compiled_program(
    program: &bex_vm_types::Program,
    compiled_bodies: Option<CompiledBodies>,
) -> Result<Vec<Option<String>>, VmInternalError> {
    if !super::STACKFUL_FIBERS_SUPPORTED
        && compiled_bodies.is_some_and(|bodies| !bodies.table.entries.is_empty())
    {
        return Err(VmInternalError::CompiledBody {
            message:
                "compiled bodies require stackful fibers, which are unsupported on this target"
                    .into(),
        });
    }
    if let Some(bodies) = compiled_bodies
        && bodies.table.fingerprint != bodies.artifact_fingerprint
    {
        return Err(VmInternalError::CompiledBody {
            message: "compiled bodies were compiled from another program: fingerprint mismatch"
                .into(),
        });
    }
    // Function names as the loader renders them; pooled functions have no owner yet.
    let body_names: Vec<Option<String>> = compiled_bodies.map_or_else(Vec::new, |bodies| {
        bodies
            .table
            .entries
            .iter()
            .map(|entry| {
                let index = entry.object as usize;
                (index < program.objects.len())
                    .then(|| program.function_name(bex_vm_types::ObjectIndex::from_raw(index)))
                    .flatten()
            })
            .collect()
    });
    Ok(body_names)
}

impl BytecodeFrame {
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
    /// Internal errors propagate without unwinding; thrown values unwind to `level`.
    pub(crate) fn land_compiled_catch(
        &mut self,
        error: Box<VmError>,
        level: usize,
        landing: Option<(usize, usize)>,
    ) -> Result<(Value, Value), Box<VmError>> {
        let VmError::Thrown(thrown) = *error else {
            return Err(error);
        };
        let value = thrown.value;
        let outcome = self.telemetry_outcome_for_exception(value);
        let (mut context, mut evidence) = self
            .exception_evidence(&thrown, RaiseEntry::Vm, None)
            .map_err(Box::new)?;
        while self.level_count() > level + 1 {
            let top = self.top_frame().expect("frames above the handler");
            // SAFETY: compiled frames hold functions from the compile-time heap;
            // see `load_function`.
            let function = unsafe { self.frame_function(top) }
                .map_err(|error| Box::new(VmError::InternalError(error)))?;
            let recorded = if self.frame_wants_error_capture(top, function) {
                self.recorded_context(&mut context, value)
            } else {
                value
            };
            self.complete_unwound_frame(top, function, outcome, recorded, evidence.as_mut());
            if let Some(evidence) = &mut evidence {
                self.error_frame_unwound(evidence, self.level_of(top));
            }
            match top {
                FrameAt::Vm(_) => {
                    if let Some(Frame::Bytecode(frame)) = self.frames.pop() {
                        self.stack.truncate(frame.locals_offset.raw());
                    }
                }
                FrameAt::Record(record) => {
                    debug_assert!(self.is_zombie(record), "only zombies stand above a handler");
                    self.pop_zombie(record);
                }
            }
        }
        let ctx = match context {
            InFlightContext::Carried(context) => context,
            InFlightContext::Fresh { trace, cause } => {
                self.alloc_error_context(value, &trace, cause)
            }
        };
        let frame = self.frame_at_level(level).expect("catching frame is live");
        if let Some((slot, _)) = landing {
            self.stack.0[slot] = ctx;
        }
        if let Some(evidence) = evidence {
            self.error_landed(
                evidence,
                frame,
                landing.map(|(slot, _)| (slot, true)),
                landing.map_or(0, |(_, pc)| pc),
            );
        }
        Ok((value, ctx))
    }

    #[inline(never)]
    pub(crate) fn resume_compiled(
        &mut self,
        frame_idx: &mut usize,
        function: &mut &'static Function,
    ) -> Result<Option<VmExecState>, VmError> {
        use crate::compiled_rt::{Request, stub};
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
        let Frame::Bytecode(frame) = &mut self.frames[*frame_idx] else {
            unreachable!("a compiled frame is a bytecode frame");
        };
        frame.instruction_ptr = crate::compiled_rt::RUNNING;
        let locals_offset = frame.locals_offset.raw();
        let request = match offset {
            stub::START => {
                let args = (0..function.arity)
                    .map(|i| self.stack.get_at(StackIndex::from_raw(locals_offset + i)))
                    .collect();
                self.fiber_resume(|vm| super::fiber::Message::Enter { vm, body, args })
            }
            stub::AFTER_YIELD => self.resume_body(Ok(Value::NULL)),
            stub::AFTER_LOG => {
                // The engine answers a log with a null, as it does for `SendEvent`.
                self.stack.ensure_pop();
                self.resume_body(Ok(Value::NULL))
            }
            // Results arrive on the stack; errors arrive through `delivery`.
            _ => match self.compiled.delivery.take() {
                Some(error) => self.resume_body(Err(error)),
                None => {
                    let value = self.stack.ensure_pop();
                    self.resume_body(Ok(value))
                }
            },
        };
        self.settle();
        // A suspension may have materialized frames above the entered frame.
        *frame_idx = self.frames.len() - 1;
        // SAFETY: see `load_function`.
        *function = unsafe { self.load_function(*frame_idx)? };
        let step = match request {
            Request::Done(outcome) => {
                let value = outcome.map_err(|error| *error)?;
                self.stack.push(value);
                return self.finish_bytecode_frame(frame_idx, function, value);
            }
            Request::Step(step) => step,
        };
        let start = crate::compiled_rt::stub_start(function)
            .expect("a suspending body has a compiled function");
        let offset = match &step {
            Step::Call { .. } => stub::AFTER_CALL,
            Step::Await(_) => stub::AWAIT,
            Step::AwaitAny(_) => stub::AWAIT_ANY,
            Step::Spawn(_) => stub::SPAWN,
            Step::Log { .. } => stub::AFTER_LOG,
            Step::Yield => stub::AFTER_YIELD,
        };
        if let Frame::Bytecode(frame) = &mut self.frames[*frame_idx] {
            frame.instruction_ptr = start + offset;
        }
        match step {
            Step::Call { callee, argc } => {
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
                self.stack.push(operand);
                Ok(None)
            }
            Step::Log {
                level,
                event_name,
                data,
                source_location,
            } => Ok(Some(VmExecState::Log {
                level,
                event_name,
                data,
                source_location,
            })),
            Step::Yield => Ok(Some(VmExecState::EarlyYield)),
        }
    }

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
        let pc = self.compiled_instruction(self.innermost_at(caller));
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

    pub(crate) fn compiled_at(&self, idx: usize) -> Option<crate::compiled_rt::CompiledAt> {
        let Frame::Bytecode(frame) = &self.frames[idx] else {
            return None;
        };
        if frame.instruction_ptr == crate::compiled_rt::RUNNING {
            return Some(crate::compiled_rt::CompiledAt::Running);
        }
        // Function values retain a GenericFunction wrapper in their VM frame.
        // Resolve it as dispatch does, or a parked fiber looks like bytecode
        // and the unwinder pops its frame before its live records can unwind.
        // SAFETY: the frame roots its callable under the heap permit.
        let function = unsafe { self.load_function(idx) }.ok()?;
        crate::compiled_rt::compiled_at(frame.instruction_ptr, function)
    }

    /// The innermost bytecode frame when it is executing bytecode: the one
    /// frame whose position is `cur_pc` rather than a recorded pc.
    #[must_use]
    pub fn live_bytecode_frame(&self) -> Option<usize> {
        self.frames
            .iter()
            .rposition(|frame| matches!(frame, Frame::Bytecode(_)))
            .filter(|&idx| self.compiled_at(idx).is_none())
    }

    /// The compact pc frame `idx` stands at: a compiled frame's recorded
    /// instruction, `cur_pc` for `live` (see [`Self::live_bytecode_frame`]),
    /// or the call site a bytecode frame recorded when it descended. `None`
    /// for a native frame or a compiled position with no compact offset.
    #[must_use]
    pub fn frame_pc(&self, idx: usize, live: Option<usize>) -> Option<usize> {
        let Frame::Bytecode(frame) = &self.frames[idx] else {
            return None;
        };
        if self.compiled_at(idx).is_some() {
            // SAFETY: the frame roots its callable under the heap permit.
            return unsafe { self.load_function(idx) }
                .ok()?
                .bytecode
                .compact
                .as_ref()?
                .offset_of(self.compiled_instruction(FrameAt::Vm(idx)));
        }
        Some(if Some(idx) == live {
            self.cur_pc
        } else {
            frame.faulting_pc
        })
    }

    pub(crate) fn caller_pc(&self, caller: FrameAt, frame_idx: usize, call_site: usize) -> u32 {
        if self.frame_runs_compiled(caller) {
            let pc = self.frame_at_pc(caller, None).unwrap_or(0);
            return u32::try_from(pc).unwrap_or(u32::MAX);
        }
        let FrameAt::Vm(caller_frame_idx) = caller else {
            unreachable!("a frame record runs compiled code");
        };
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

        // As `OpCode::Return` does: charged before the frame completes, so an
        // invoke that is a span carries its own usage.
        if matches!(
            function.body_meta,
            Some(bex_vm_types::FunctionMeta::ModelInvoke)
        ) {
            self.charge_model_usage(result);
        }
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

    pub(crate) fn bind_passed_type_args(&mut self, frame_idx: usize) {
        let Frame::Bytecode(frame) = &mut self.frames[frame_idx] else {
            unreachable!("a compiled frame is a bytecode frame");
        };
        append_passed_type_args(
            frame,
            &self.pending_call_type_args,
            &self.pending_call_type_values,
        );
    }
    pub(in crate::vm) fn unwind_suspended_compiled(
        &mut self,
        depth: usize,
        frame_locals_offset: StackIndex,
        exception_value: Value,
        context: &InFlightContext,
        evidence: &mut Option<UnwindEvidence>,
    ) -> Result<&'static Function, VmError> {
        // SAFETY: see `load_function`.
        let frame_function = unsafe { self.load_function(depth)? };
        let stub = crate::compiled_rt::stub_start(frame_function)
            .expect("a suspended compiled frame has a compiled body");
        // The body the error is delivered to is the innermost one parked at
        // this frame: a frame record above it, if any. Its slots, and those of
        // every record below it, stay.
        let kept = self.suspended_slots_end(depth, frame_locals_offset.raw())?;
        // A recoverable hook failure answers the hook call with null,
        // so the body lands at its decision boundary as bytecode does
        // below. Task cancellation keeps unwinding normally.
        if self
            .pending_trace_hooks
            .last()
            .is_some_and(|hook| hook.frame == depth && hook.evaluating)
            && !self.as_instance(&exception_value).is_ok_and(|instance| {
                instance.class == self.panic_class_ptrs[PanicClass::Cancelled as usize]
            })
        {
            self.stack.truncate(kept);
            self.stack.push(Value::NULL);
            if let Some(evidence) = evidence.take() {
                self.error_hook_fallback(evidence);
            }
            self.restore_caller_context(depth);
            return Ok(frame_function);
        }
        let context = match context {
            InFlightContext::Carried(context) => *context,
            InFlightContext::Fresh { trace, cause } => {
                self.alloc_error_context(exception_value, trace, *cause)
            }
        };
        self.compiled.delivery = Some(Box::new(VmError::Thrown(VmThrown::rethrow(
            exception_value,
            context,
        ))));
        if let Frame::Bytecode(frame) = &mut self.frames[depth] {
            frame.instruction_ptr = stub + crate::compiled_rt::stub::AFTER_CALL;
        }
        self.stack.truncate(kept);
        self.compiled.error_evidence = evidence.take();
        Ok(frame_function)
    }
}

#[cfg(all(target_pointer_width = "64", not(feature = "heap_debug")))]
const _: () = assert!(std::mem::size_of::<BytecodeFrame>() == 112);
