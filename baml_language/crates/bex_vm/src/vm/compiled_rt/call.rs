//! Calls, frame slots, and control-flow checks for compiled bodies.
//!
//! The entry points here are safe although they store and later dereference
//! caller-supplied [`HeapPtr`]s: they rely on the contract of
//! [`BexVm::get_object`], which every safe VM API shares. A `HeapPtr` can only
//! be built unsafely from a live heap object, and generated code passes only
//! pointers from `idx_to_ptr` or from values it has just loaded or reloaded
//! from its frame slots after a suspension.

use bex_vm_types::{
    HeapPtr, Object, RealizedTy, StackIndex, Value,
    types::{Function, FunctionType},
};

use super::{CompiledFn, FrameRecord, Step};
use crate::{
    errors::{VmError, VmInternalError, VmPanic},
    indexable::EvalStackTrait,
    package_baml::{NativeCallResult, NativeFunction},
    types::ObjectTrait as _,
    vm::{BexVm, Frame, VmExecState},
};

#[cfg(test)]
thread_local! {
    /// Force calls through the VM call path to test telemetry equivalence.
    pub(crate) static FORCE_CALL_FUNNEL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Run direct compiled calls with VM frames, as `BAML_COMPILED_FRAMES=1` does.
    pub(crate) static FORCE_VM_FRAMES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl BexVm {
    /// Convert an indirect callee as `CallIndirect` does, including scalar type errors.
    pub fn indirect_callee(&self, value: Value) -> Result<HeapPtr, Box<VmError>> {
        self.as_object_ptr(value, FunctionType::Callable.into())
            .map_err(|error| Box::new(error.into()))
    }

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

    /// Call a generic builtin with type arguments available through `current_call_type_args()`.
    pub fn call_builtin_typed(
        &mut self,
        builtin: NativeFunction,
        type_args: &[RealizedTy],
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        self.save_call_types();
        let mut buffer = std::mem::take(&mut self.compiled.builtin_type_args);
        buffer.clear();
        buffer.extend_from_slice(type_args);
        self.pending_call_type_args = buffer;
        let result = self.call_builtin(builtin, args);
        self.compiled.builtin_type_args = self.restore_call_types();
        self.compiled.builtin_type_args.clear();
        result
    }

    pub fn reserve_slots(&mut self, frame: &FrameRecord, slots: usize) -> Result<(), Box<VmError>> {
        let end = frame.base() + slots;
        self.resize_stack(end, Value::NULL).map_err(Box::new)
    }

    #[inline]
    #[must_use]
    pub fn slot(&self, frame: &FrameRecord, index: usize) -> Value {
        self.stack
            .get_at(StackIndex::from_raw(frame.base() + index))
    }

    #[inline]
    pub fn set_slot(&mut self, frame: &FrameRecord, index: usize, value: Value) {
        self.stack[StackIndex::from_raw(frame.base() + index)] = value;
    }

    #[inline]
    pub fn push_arg(&mut self, value: Value) -> Result<(), Box<VmError>> {
        self.push_stack(value).map_err(Box::new)
    }

    #[inline]
    pub fn push_type_arg(&mut self, ty: RealizedTy) {
        self.compiled.call_type_args.push(ty);
    }

    /// Whether a loop should suspend with [`Step::Yield`] at this back edge: the
    /// interpreter's own poll, forced under `BAML_GC_STRESS=1`.
    #[inline]
    pub fn should_yield(&mut self) -> bool {
        super::gc_stress() || self.should_early_yield()
    }

    /// Tick the interpreter's call countdown; GC stress yields whenever it expires.
    #[inline]
    pub fn should_yield_at_call(&mut self) -> bool {
        self.early_yield.tick() && (super::gc_stress() || self.poll_for_yield())
    }

    /// Return the caught value and its `baml.errors.Context`, propagating internal errors.
    #[inline]
    pub fn catch(
        &mut self,
        error: Box<VmError>,
        frame: &FrameRecord,
    ) -> Result<(Value, Value), Box<VmError>> {
        let level = self.level_of(Self::record_frame(frame));
        self.land_compiled_catch(error, level, None)
    }

    /// Catch at a generated handler, rooting its context for rethrow provenance.
    #[allow(unsafe_code)]
    pub fn catch_at(
        &mut self,
        error: Box<VmError>,
        frame: &FrameRecord,
        context_slot: usize,
        handler_pc: usize,
    ) -> Result<(Value, Value), Box<VmError>> {
        let at = Self::record_frame(frame);
        // SAFETY: the live frame roots its function under the heap permit.
        let function = unsafe { self.frame_function(at) }?;
        let pc = function
            .bytecode
            .compact
            .as_ref()
            .and_then(|code| code.offset_of(handler_pc))
            .unwrap_or(handler_pc);
        self.land_compiled_catch(
            error,
            self.level_of(at),
            Some((frame.base() + context_slot, pc)),
        )
    }

    #[cold]
    #[inline(never)]
    pub fn unreachable_panic(&mut self) -> Box<VmError> {
        Box::new(VmError::thrown_fresh(
            self.panic_to_exception_value(VmPanic::Unreachable),
        ))
    }

    pub fn set_call_trace(&mut self, value: Value) -> Result<(), Box<VmError>> {
        match self.trace_config(value) {
            Ok(trace) => {
                self.pending_call_trace = Some(trace);
                Ok(())
            }
            Err(error) => {
                // Discard type arguments for the failed call.
                self.compiled.call_type_args.clear();
                Err(Box::new(error))
            }
        }
    }

    pub fn log_event(
        &mut self,
        frame: &FrameRecord,
        level: super::LogLevel,
        data: Value,
        event_name: Value,
        pc: u32,
    ) -> Result<(), Box<VmError>> {
        let name = if event_name.is_null() || event_name.is_omitted() {
            None
        } else {
            let name = self.as_string(&event_name)?;
            Some(std::sync::Arc::<str>::from(name.as_str()))
        };
        let at = Self::record_frame(frame);
        self.record_log_at(at, level, name.clone(), data);
        let source_location = self
            .get_object(frame.function())
            .as_callable()
            .ok()
            .and_then(|function| function.bytecode.line_entry_for_pc(pc as usize))
            .map(Self::event_source_location_for_line_entry);
        frame.note_pc(pc);
        self.suspend(Step::Log {
            level,
            event_name: name,
            data,
            source_location,
        })
        .map(drop)
    }

    /// VM-frame fallback: `type_args` use De Bruijn order; errors retain the frame.
    #[inline]
    pub fn call_compiled(
        &mut self,
        caller: &FrameRecord,
        callee: HeapPtr,
        body: CompiledFn,
        type_args: &[RealizedTy],
        args: &[Value],
        pc: u32,
    ) -> Result<Value, Box<VmError>> {
        caller.note_pc(pc);
        self.check_compiled_call()?;
        if self.telemetry.is_some() {
            if type_args.is_empty() {
                return self.call_compiled_observed(caller, callee, body, args);
            }
            return self.call_compiled_funnelled(callee, type_args, args);
        }
        // Runtime declarations need exact type values in addition to realized types.
        let type_values = super::frame::exact_type_values(type_args);
        let metadata = (!type_values.is_empty()).then(|| {
            Box::new(crate::vm::FrameTypeMetadata {
                values: type_values,
            })
        });
        self.run_compiled_frame(callee, body, type_args.to_vec(), metadata, args)
    }

    /// Closures carry captured type arguments; framed or runtime declarations require a VM frame.
    #[inline]
    pub fn call_closure_compiled(
        &mut self,
        caller: &FrameRecord,
        callee: HeapPtr,
        body: CompiledFn,
        args: &[Value],
        pc: u32,
    ) -> Result<Value, Box<VmError>> {
        if self.direct_calls() {
            let (function, type_args): (HeapPtr, &[RealizedTy]) = match self.get_object(callee) {
                Object::Closure(closure) => (closure.function, &closure.captured_type_args),
                Object::Function(_) => (callee, &[]),
                _ => (HeapPtr::null(), &[]),
            };
            if !function.as_ptr().is_null()
                && type_args
                    .iter()
                    .all(crate::reachable::is_statically_declared)
                && !Self::body_is_framed(function)
            {
                let type_args = type_args.to_vec();
                return self.call_record_dispatched(caller, callee, body, type_args, args, pc);
            }
        }
        caller.note_pc(pc);
        self.check_compiled_call()?;
        if self.telemetry.is_some() {
            return self.call_compiled_funnelled(callee, &[], args);
        }
        let type_args = match self.get_object(callee) {
            Object::Closure(closure) => closure.captured_type_args.to_vec(),
            _ => Vec::new(),
        };
        self.run_compiled_frame(callee, body, type_args, None, args)
    }

    /// Whether the compiled body of the function at `function` needs a VM
    /// frame; see [`bex_vm_types::types::CompiledBody::framed`].
    pub(crate) fn body_is_framed(function: HeapPtr) -> bool {
        // SAFETY: a closure roots its function.
        #[allow(unsafe_code)]
        match unsafe { function.get() } {
            Object::Function(function) => {
                function.trace_passthrough
                    || function
                        .compiled_body
                        .is_some_and(bex_vm_types::types::CompiledBody::framed)
            }
            _ => true,
        }
    }

    /// Reserves the callee's VM frame before telemetry or other call-setup side effects.
    #[inline]
    fn check_compiled_call(&mut self) -> Result<(), Box<VmError>> {
        self.reserve_call_frame().map_err(Box::new)?;
        self.stack_room()
    }

    #[inline]
    fn run_compiled_frame(
        &mut self,
        callee: HeapPtr,
        body: CompiledFn,
        type_args: Vec<RealizedTy>,
        type_metadata: Option<Box<crate::vm::FrameTypeMetadata>>,
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        let locals_offset = self.push_compiled_frame(callee, type_args, type_metadata, None);
        let result = self.run_entry(body, args);
        if result.is_ok() {
            // As a bytecode return does: a span the body activated ends.
            self.end_manual_activation(self.frames.len() - 1);
            self.frames.pop();
            self.stack.truncate(locals_offset.raw());
        }
        result
    }

    // Plain calls inherit context and use their recorded call site; errors retain the receipt.
    #[cold]
    #[inline(never)]
    #[allow(unsafe_code)]
    fn call_compiled_observed(
        &mut self,
        caller: &FrameRecord,
        callee: HeapPtr,
        body: CompiledFn,
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        // SAFETY: the callee lives in the compile-time heap, which is never
        // collected; see `load_function`.
        let function: &'static Function = match unsafe { callee.get() } {
            Object::Function(function) => function,
            _ => return self.call_compiled_funnelled(callee, &[], args),
        };
        if self.must_funnel(function) {
            return self.call_compiled_funnelled(callee, &[], args);
        }
        debug_assert_eq!(args.len(), function.arity);
        let frame_telemetry = self.enter_record_receipt(caller, function, callee, &[], &[], args);
        let locals_offset = self.push_compiled_frame(callee, Vec::new(), None, frame_telemetry);
        let result = self.run_entry(body, args);
        if let Ok(value) = result {
            if matches!(
                function.body_meta,
                Some(bex_vm_types::FunctionMeta::ModelInvoke)
            ) {
                self.charge_model_usage(value);
            }
            self.end_manual_activation(self.frames.len() - 1);
            let Some(Frame::Bytecode(frame)) = self.frames.pop() else {
                unreachable!("a compiled callee has a bytecode frame");
            };
            self.stack.truncate(locals_offset.raw());
            if let Some(frame_telemetry) = frame.telemetry {
                self.complete_record_telemetry(frame_telemetry, function, value);
            }
        }
        result
    }

    /// Whether a plain compiled callee must take the VM's call path with
    /// telemetry on: a trace hook, a pending trace attachment, or passed type
    /// arguments need it.
    #[inline]
    pub(crate) fn must_funnel(&self, function: &Function) -> bool {
        let hooked = function
            .bytecode
            .compact
            .as_ref()
            .is_some_and(|code| code.trace_hook_finish_pc.is_some());
        #[cfg(test)]
        let forced = FORCE_CALL_FUNNEL.with(std::cell::Cell::get);
        #[cfg(not(test))]
        let forced = false;
        hooked || forced || self.pending_call_trace.is_some() || self.pending_call_passes_type_args
    }

    /// Call through the VM for generic callees, trace hooks, trace attachments, or indirect callees.
    #[cold]
    #[inline(never)]
    #[allow(unsafe_code)]
    pub(super) fn call_compiled_funnelled(
        &mut self,
        callee: HeapPtr,
        type_args: &[RealizedTy],
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        let top = self.frames.len() - 1;
        let locals_offset = StackIndex::from_raw(self.stack.len());
        self.extend_stack(args.iter().copied()).map_err(Box::new)?;
        let mut frame_idx = top;
        // SAFETY: the frame's function lives in the compile-time heap, which
        // is never collected; see `load_function`.
        let mut function = unsafe { self.load_function(top)? };
        let outcome = self.compiled_caller_call(
            callee,
            locals_offset,
            args.len(),
            type_args,
            &mut frame_idx,
            &mut function,
        );
        match outcome {
            // The call path leaves the result on the stack. An early yield is
            // delayed until the compiled caller next yields.
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
}
