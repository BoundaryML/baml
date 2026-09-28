//! Exception-path telemetry: raises, unwind ends, landing notes and future
//! links. Everything here runs only while unwinding or failing an await. The
//! successful call, return and await paths never reach it, and it builds
//! nothing unless a recording asked for it.
//!
//! Identity rules: every unwind gets a fresh raise ID. A raise that passes an
//! earlier error along names that error's origin only through a landing note
//! (rethrow) or a future link (await). Equal values alone never link two
//! raises. An `UnknownError` conversion never establishes a proven link:
//! the VM does not record where the converted value came from.
use std::sync::Arc;

use bex_vm_types::{
    Function, Object,
    bytecode::OpCode,
    errors::{StackFrame, ThrowKind, VmThrown},
};
use btel_records::{
    ErrorFrame, FutureErrorLink, FutureErrorLookup, InheritedFrame, MAX_ERROR_FRAMES,
    MAX_INHERITED_FRAMES, MAX_INHERITED_TEXT_BYTES, NO_PC, OriginEvidence, OriginVia, RaiseKind,
    RaiseOrigin, RaiseStack, SpanRecord, UnresolvedOrigin, UnwindResult,
};
use btel_types::{
    CallPathId, ClockInstant, FunctionId, TelemetryId, TelemetryPolicyId, allocate_telemetry_id,
};

use super::{BexVm, Frame, ThrowContext};
use crate::telemetry::{ActiveHandler, FrameScope, LandingMatch};

/// How unwinding was entered, when the faulting opcode alone cannot say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RaiseEntry {
    /// From VM execution: the faulting opcode decides the kind.
    Vm,
    /// The engine injected a sys-op or host failure.
    Host,
}

/// One recorded unwind while it runs.
#[derive(Clone, Copy)]
pub(super) struct UnwindEvidence {
    raise: TelemetryId,
    /// Also the exit instant of every frame this unwind pops.
    raised_at: ClockInstant,
    origin: OriginEvidence,
    raise_frame: Option<usize>,
    /// Every bytecode frame was observed, so defining their call paths
    /// registered their functions.
    on_path: bool,
    may_promote: bool,
    unwound: u32,
    /// The raise's record, not written yet. Until a retained call completes,
    /// nothing has to sit between the raise and its end, so both can share
    /// one record.
    held: Option<HeldRaise>,
}

/// An `ErrorRaised` whose raise frame was neither a span nor promotable.
#[derive(Clone, Copy)]
struct HeldRaise {
    kind: RaiseKind,
    function: Option<FunctionId>,
    pc: Option<u32>,
    call_path: CallPathId,
    frame_count: u32,
}

impl UnwindEvidence {
    pub(super) fn raised_at(&self) -> ClockInstant {
        self.raised_at
    }

    pub(super) fn holds_raise(&self) -> bool {
        self.held.is_some()
    }
}

fn clip(text: &str) -> String {
    if text.len() <= MAX_INHERITED_TEXT_BYTES {
        return text.to_owned();
    }
    let mut end = MAX_INHERITED_TEXT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

fn raised(
    raise_id: TelemetryId,
    raised_at: ClockInstant,
    held: HeldRaise,
    raise_call: Option<TelemetryId>,
    raise_frame_may_promote: bool,
) -> SpanRecord<btel_snapshot::Snapshot, btel_snapshot::Snapshot> {
    SpanRecord::ErrorRaised {
        raise_id,
        raised_at,
        kind: held.kind,
        function: held.function,
        pc: held.pc,
        raise_call,
        raise_frame_may_promote,
        call_path: held.call_path,
        frame_count: held.frame_count,
    }
}

fn origin_evidence(raise: TelemetryId, origin: RaiseOrigin) -> OriginEvidence {
    match origin {
        RaiseOrigin::Fresh => OriginEvidence::Known(raise),
        RaiseOrigin::Proven { origin, .. } => OriginEvidence::Known(origin),
        RaiseOrigin::Ambiguous { candidates, .. } => OriginEvidence::Ambiguous(candidates),
        RaiseOrigin::Unresolved { reason, .. } => OriginEvidence::Unresolved(reason),
    }
}

fn carried(evidence: OriginEvidence, previous: Option<TelemetryId>, via: OriginVia) -> RaiseOrigin {
    match evidence {
        OriginEvidence::Known(origin) => RaiseOrigin::Proven {
            origin,
            previous,
            via,
        },
        OriginEvidence::Ambiguous(candidates) => RaiseOrigin::Ambiguous { candidates, via },
        OriginEvidence::Unresolved(reason) => RaiseOrigin::Unresolved { reason, via },
    }
}

fn from_match(found: LandingMatch, via: OriginVia) -> RaiseOrigin {
    match found {
        LandingMatch::Origin(evidence, previous) => carried(evidence, previous, via),
        LandingMatch::Ambiguous(candidates) => RaiseOrigin::Ambiguous { candidates, via },
        LandingMatch::Unresolved(reason) => RaiseOrigin::Unresolved { reason, via },
    }
}

/// One live bytecode frame, for landing-note lookups.
struct HandlerFrame {
    depth: usize,
    function: Option<FunctionId>,
    pc: usize,
    locals_offset: bex_vm_types::StackIndex,
    code: &'static bex_vm_types::bytecode::CompactCode,
}

/// Whether `pc` lies in the body of the catch or defer pad entered at
/// `handler_pc`: its dispatch block, arm blocks or pad body.
fn in_handler_body(
    code: &bex_vm_types::bytecode::CompactCode,
    handler_pc: usize,
    pc: usize,
) -> bool {
    code.handler_context_table
        .iter()
        .any(|entry| entry.handler_pc == handler_pc && (entry.start_pc..entry.end_pc).contains(&pc))
}

/// Whether any instruction in the handler's body stores into frame-local
/// `local`. Conservative: undecodable code counts as a store.
fn handler_writes_local(
    code: &bex_vm_types::bytecode::CompactCode,
    handler_pc: usize,
    local: usize,
) -> bool {
    let operand = |at: usize| -> Option<usize> {
        let bytes = code.code.get(at..at + 4)?;
        usize::try_from(u32::from_le_bytes(bytes.try_into().ok()?)).ok()
    };
    code.handler_context_table
        .iter()
        .filter(|entry| entry.handler_pc == handler_pc)
        .any(|entry| {
            let mut pc = entry.start_pc;
            while pc < entry.end_pc {
                let Some(op) = code
                    .code
                    .get(pc)
                    .and_then(|byte| OpCode::try_from(*byte).ok())
                else {
                    return true;
                };
                let stored = match op {
                    OpCode::StoreVar | OpCode::StoreVarLoadVar => vec![operand(pc + 1)],
                    OpCode::StoreVar2 => vec![operand(pc + 1), operand(pc + 5)],
                    OpCode::NarrowBind => vec![operand(pc + 5)],
                    _ => Vec::new(),
                };
                if stored
                    .iter()
                    .any(|slot| slot.is_none_or(|slot| slot == local))
                {
                    return true;
                }
                pc += op.encoded_size();
            }
            false
        })
}

impl BexVm {
    /// The underlying function's telemetry ID, without registering it.
    fn frame_telemetry_function(&self, frame_idx: usize) -> Option<FunctionId> {
        let ptr = self.frame_function_identity(frame_idx)?;
        match self.get_object(ptr) {
            Object::Function(function) => function.telemetry_function_id,
            _ => None,
        }
    }

    /// Register and return the frame's function ID before a record names it.
    fn frame_registered_function(&self, frame_idx: usize) -> Option<FunctionId> {
        let ptr = self.frame_function_identity(frame_idx)?;
        // SAFETY: unwinding holds the heap permit and the frame roots `ptr`.
        unsafe { self.heap.register_telemetry_function(ptr) }
    }

    fn raise_kind(
        &self,
        entry: RaiseEntry,
        function: Option<&Function>,
        thrown: &VmThrown,
    ) -> RaiseKind {
        if entry == RaiseEntry::Host {
            return RaiseKind::HostBoundary;
        }
        let opcode = function
            .and_then(|function| function.bytecode.compact.as_ref()?.code.get(self.cur_pc))
            .and_then(|byte| OpCode::try_from(*byte).ok());
        match opcode {
            Some(OpCode::Throw) => RaiseKind::Throw,
            Some(OpCode::Rethrow) => RaiseKind::Rethrow,
            Some(OpCode::ThrowIfPanic) => RaiseKind::PanicRethrow,
            Some(OpCode::Await) if thrown.throw_kind == ThrowKind::Rethrow => RaiseKind::Await,
            Some(OpCode::Await) => RaiseKind::AwaitCancelled,
            Some(
                OpCode::Call
                | OpCode::CallExactArgs
                | OpCode::CallIndirect
                | OpCode::VirtualCall
                | OpCode::SysOp,
            ) => RaiseKind::NativeBoundary,
            _ => RaiseKind::Runtime,
        }
    }

    /// Landings of handlers running in the rethrowing frame whose slot still
    /// holds the value. See `ErrorBook::lookup` for when a note counts.
    fn rethrow_origin(&self, frame: Option<usize>, value: bex_vm_types::Value) -> RaiseOrigin {
        let Some(frame) = frame else {
            return RaiseOrigin::Unresolved {
                reason: UnresolvedOrigin::NoLanding,
                via: OriginVia::Rethrow,
            };
        };
        let frames: Vec<HandlerFrame> = self.handler_frame(frame).into_iter().collect();
        from_match(self.landing_origin(&frames, value), OriginVia::Rethrow)
    }

    /// A live bytecode frame's compiled handler tables and current PC: the
    /// live PC for the innermost bytecode frame, the call site for the others,
    /// as the unwinder uses them.
    fn handler_frame(&self, idx: usize) -> Option<HandlerFrame> {
        let Some(Frame::Bytecode(frame)) = self.frames.get(idx) else {
            return None;
        };
        let innermost = self
            .frames
            .iter()
            .rposition(|frame| matches!(frame, Frame::Bytecode(_)));
        // SAFETY: the bytecode frame roots its function under the permit.
        let function = unsafe { self.load_function(idx) }.ok()?;
        Some(HandlerFrame {
            depth: idx,
            function: self.frame_telemetry_function(idx),
            pc: if innermost == Some(idx) {
                self.cur_pc
            } else {
                frame.faulting_pc
            },
            locals_offset: frame.locals_offset,
            code: function.bytecode.compact.as_ref()?,
        })
    }

    fn landing_origin(&self, frames: &[HandlerFrame], value: bex_vm_types::Value) -> LandingMatch {
        let Some(book) = self
            .telemetry
            .as_ref()
            .and_then(|telemetry| telemetry.error_book_ref())
        else {
            return LandingMatch::Unresolved(UnresolvedOrigin::NoLanding);
        };
        let in_body: Vec<Box<dyn Fn(usize, usize) -> bool>> = frames
            .iter()
            .map(|frame| {
                let code = frame.code;
                Box::new(move |handler_pc, pc| in_handler_body(code, handler_pc, pc))
                    as Box<dyn Fn(usize, usize) -> bool>
            })
            .collect();
        let scopes: Vec<FrameScope<'_>> = frames
            .iter()
            .zip(&in_body)
            .map(|(frame, in_body)| FrameScope {
                depth: frame.depth,
                function: frame.function,
                active: Self::active_handlers(frame),
                in_body: in_body.as_ref(),
            })
            .collect();
        let stack = &self.stack.0;
        book.lookup(&scopes, |slot| slot < stack.len() && stack[slot] == value)
    }

    /// Handlers whose body covers the frame's PC, with their error slots and
    /// whether their body can store into those slots.
    fn active_handlers(frame: &HandlerFrame) -> Vec<ActiveHandler> {
        let mut handlers: Vec<usize> = frame
            .code
            .handler_context_table
            .iter()
            .filter(|entry| (entry.start_pc..entry.end_pc).contains(&frame.pc))
            .map(|entry| entry.handler_pc)
            .collect();
        handlers.sort_unstable();
        handlers.dedup();
        handlers
            .into_iter()
            .map(|handler_pc| {
                let mut locals: Vec<usize> = frame
                    .code
                    .exception_table
                    .iter()
                    .filter(|entry| entry.handler_pc == handler_pc)
                    .map(|entry| entry.error_slot)
                    .collect();
                locals.sort_unstable();
                locals.dedup();
                let written = locals
                    .iter()
                    .any(|local| handler_writes_local(frame.code, handler_pc, *local));
                ActiveHandler {
                    handler_pc,
                    error_slots: locals
                        .iter()
                        .map(|local| {
                            Self::local_slot_stack_index(frame.locals_offset, *local).raw()
                        })
                        .collect(),
                    slot_written: written,
                }
            })
            .collect()
    }

    /// The failed future's recorded raise. The await leaves its operand on
    /// top of the stack until the future is ready or unwinding truncates it.
    fn await_origin(&self) -> RaiseOrigin {
        let unresolved = |reason| RaiseOrigin::Unresolved {
            reason,
            via: OriginVia::Await,
        };
        let future = self
            .stack
            .0
            .last()
            .and_then(bex_vm_types::Value::as_object_ptr)
            .and_then(|ptr| match self.get_object(ptr) {
                Object::Future(future) => Some(future.id()),
                _ => None,
            });
        let Some(future) = future else {
            return unresolved(UnresolvedOrigin::NoFuture);
        };
        let Some(telemetry) = &self.telemetry else {
            return unresolved(UnresolvedOrigin::FutureLinkMissing);
        };
        match telemetry.future_error(future.as_usize() as u64) {
            FutureErrorLookup::Found(link) => {
                carried(link.origin, Some(link.raise), OriginVia::Await)
            }
            FutureErrorLookup::Missing => unresolved(UnresolvedOrigin::FutureLinkMissing),
            FutureErrorLookup::PossiblyEvicted => unresolved(UnresolvedOrigin::FutureLinkEvicted),
        }
    }

    /// Live stack, innermost first: the same frames and PCs the unwinder and
    /// diagnostic traces use. Only for raises without a call path.
    fn error_frames(&self, raise_frame: Option<usize>) -> Vec<ErrorFrame> {
        (0..self.frames.len())
            .rev()
            .take(MAX_ERROR_FRAMES)
            .map(|idx| match &self.frames[idx] {
                Frame::Bytecode(frame) => ErrorFrame {
                    function: self.frame_registered_function(idx),
                    pc: Some(if Some(idx) == raise_frame {
                        self.cur_pc
                    } else {
                        frame.faulting_pc
                    })
                    .map(|pc| u32::try_from(pc).unwrap_or(u32::MAX)),
                    native: false,
                },
                Frame::Native(native) => ErrorFrame {
                    function: match self.get_object(native.function) {
                        // SAFETY: as above; native frames root their function.
                        Object::Function(_) => unsafe {
                            self.heap.register_telemetry_function(native.function)
                        },
                        _ => None,
                    },
                    pc: None,
                    native: true,
                },
            })
            .collect()
    }

    /// Whether every bytecode frame on this thread has telemetry, so each one
    /// is on the active call path: its callers name the whole stack, native
    /// frames and direct recursion aside. Reads only the frames the unwinder
    /// just walked; never a function object.
    fn stack_is_observed(&self) -> bool {
        self.frames.iter().all(|frame| match frame {
            Frame::Bytecode(frame) => frame.telemetry.is_some(),
            Frame::Native(_) => true,
        })
    }

    /// Record the start of an unwind. `carried_context` is the throw context
    /// the unwinder consumed or read for this raise: its trace becomes
    /// inherited evidence when the origin cannot be proven. `current` is the
    /// unwinder's frame and its already loaded function.
    ///
    /// A fresh raise with every frame observed emits one fixed-size record
    /// and allocates nothing: its stack is the active call path.
    #[cold]
    #[inline(never)]
    pub(super) fn begin_error_evidence(
        &mut self,
        thrown: &VmThrown,
        entry: RaiseEntry,
        current: Option<(usize, &'static Function)>,
        carried_context: Option<&ThrowContext>,
        consumed_preserved: bool,
    ) -> Option<UnwindEvidence> {
        if !self
            .telemetry
            .as_ref()
            .is_some_and(crate::telemetry::TelemetryState::error_evidence)
        {
            return None;
        }
        let live = self.frames.len();
        if let Some(telemetry) = &mut self.telemetry {
            telemetry.error_book().begin_raise(live);
        }
        let raise_id = allocate_telemetry_id();
        let raise_frame = self
            .frames
            .iter()
            .rposition(|frame| matches!(frame, Frame::Bytecode(_)));
        let raise_function = match (raise_frame, current) {
            (Some(frame), Some((idx, function))) if frame == idx => Some(function),
            // SAFETY: the bytecode frame roots its function under the permit.
            (Some(frame), _) => unsafe { self.load_function(frame) }.ok(),
            (None, _) => None,
        };
        let kind = self.raise_kind(entry, raise_function, thrown);
        let origin = match kind {
            RaiseKind::Rethrow | RaiseKind::PanicRethrow => {
                self.rethrow_origin(raise_frame, thrown.value)
            }
            RaiseKind::Await => self.await_origin(),
            // The converted value's source is only known by value. A catch
            // slot holding an equal value, even a running catch's, does not
            // show that the conversion read it, so the source's trace is kept
            // as inherited evidence and no origin is claimed.
            _ if consumed_preserved => RaiseOrigin::Unresolved {
                reason: UnresolvedOrigin::SourceNotRecorded,
                via: OriginVia::Normalization,
            },
            // A boundary passing along an earlier error has no link to it.
            _ if thrown.throw_kind == ThrowKind::Rethrow => RaiseOrigin::Unresolved {
                reason: UnresolvedOrigin::NoLanding,
                via: OriginVia::Rethrow,
            },
            _ => RaiseOrigin::Fresh,
        };
        let raise_telemetry = raise_frame.and_then(|idx| match &self.frames[idx] {
            Frame::Bytecode(frame) => frame.telemetry,
            Frame::Native(_) => None,
        });
        // An observed raise frame owns the active call path, and defining
        // that path registered its function.
        let on_path = raise_telemetry.is_some() && self.stack_is_observed();
        let (function, frames) = if on_path {
            (
                raise_function.and_then(|function| function.telemetry_function_id),
                Vec::new(),
            )
        } else {
            (
                raise_frame.and_then(|idx| self.frame_registered_function(idx)),
                self.error_frames(raise_frame),
            )
        };
        let inherited_trace: Option<&Arc<[StackFrame]>> = match origin {
            RaiseOrigin::Fresh | RaiseOrigin::Proven { .. } => None,
            _ => carried_context.map(|context| &context.trace),
        };
        let inherited: Vec<InheritedFrame> = inherited_trace
            .map(|trace| {
                trace
                    .iter()
                    .take(MAX_INHERITED_FRAMES)
                    .map(|frame| InheritedFrame {
                        function_name: clip(&frame.function_name),
                        file: clip(&frame.file_path),
                        line: u32::try_from(frame.error_line).unwrap_or(u32::MAX),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let inherited_count =
            inherited_trace.map_or(0, |trace| u32::try_from(trace.len()).unwrap_or(u32::MAX));
        // Completion promotes a timing-only frame only through its function's
        // policy; without one the marker could never name anything.
        let may_promote = raise_telemetry.is_some_and(|frame| !frame.is_span())
            && raise_function.is_some_and(|function| {
                function.telemetry_policy_id.load() != TelemetryPolicyId::NONE
            });
        let pc = raise_frame.map(|_| u32::try_from(self.cur_pc).unwrap_or(u32::MAX));
        let frame_count = u32::try_from(self.frames.len()).unwrap_or(u32::MAX);
        let telemetry = self.telemetry.as_mut().expect("checked above");
        if origin != RaiseOrigin::Fresh {
            telemetry.write_error_record(SpanRecord::ErrorRaiseOrigin { raise_id, origin });
        }
        if !frames.is_empty() || !inherited.is_empty() {
            telemetry.write_error_record(SpanRecord::ErrorRaiseStack(Box::new(RaiseStack {
                raise_id,
                frames,
                inherited,
                inherited_count,
            })));
        }
        let raised_at = telemetry.clock().read();
        // The raise frame is the innermost frame, so when it is a span it is
        // the thread's active span.
        let raise_call = raise_telemetry
            .filter(|frame| frame.is_span())
            .map(|_| telemetry.active_id());
        let call_path = if on_path {
            telemetry.active_call_path()
        } else {
            CallPathId::ROOT
        };
        let held = HeldRaise {
            kind,
            function,
            pc,
            call_path,
            frame_count,
        };
        // A span raise frame completes as a retained call of this raise, and a
        // promotable one needs its marker: both need the raise written first.
        let held = if raise_call.is_some() || may_promote {
            telemetry.write_error_record(raised(
                raise_id,
                raised_at,
                held,
                raise_call,
                may_promote,
            ));
            None
        } else {
            Some(held)
        };
        Some(UnwindEvidence {
            raise: raise_id,
            raised_at,
            origin: origin_evidence(raise_id, origin),
            raise_frame,
            on_path,
            may_promote,
            unwound: 0,
            held,
        })
    }

    /// Write a held raise before a record that must follow it: a retained
    /// call this unwind completes.
    #[cold]
    pub(super) fn release_held_raise(&mut self, evidence: &mut UnwindEvidence) {
        if let (Some(held), Some(telemetry)) = (evidence.held.take(), &mut self.telemetry) {
            telemetry.write_error_record(raised(
                evidence.raise,
                evidence.raised_at,
                held,
                None,
                false,
            ));
        }
    }

    /// The unwind's end: alone, or with its held raise in one record.
    fn write_unwind_end(
        &mut self,
        evidence: UnwindEvidence,
        result: UnwindResult,
        handler_function: Option<FunctionId>,
        handler_pc: Option<u32>,
    ) {
        let Some(telemetry) = &mut self.telemetry else {
            return;
        };
        let record = match evidence.held {
            Some(held) => SpanRecord::ErrorRaisedAndEnded {
                raise_id: evidence.raise,
                raised_at: evidence.raised_at,
                kind: held.kind,
                function: held.function,
                pc: held.pc.unwrap_or(NO_PC),
                call_path: held.call_path,
                frame_count: held.frame_count,
                result,
                handler_function,
                handler_pc: handler_pc.unwrap_or(NO_PC),
                unwound_frames: evidence.unwound,
            },
            None => SpanRecord::ErrorUnwindEnded {
                raise_id: evidence.raise,
                result,
                handler_function,
                handler_pc,
                unwound_frames: evidence.unwound,
            },
        };
        telemetry.write_error_record(record);
    }

    /// A bytecode frame was completed and popped by this unwind. Runs once
    /// per popped frame, so only the marker is out of line.
    #[inline]
    pub(super) fn error_frame_unwound(&mut self, evidence: &mut UnwindEvidence, depth: usize) {
        evidence.unwound = evidence.unwound.saturating_add(1);
        if evidence.may_promote && evidence.raise_frame == Some(depth) {
            self.error_raise_frame_completed(evidence);
        }
    }

    #[cold]
    fn error_raise_frame_completed(&mut self, evidence: &mut UnwindEvidence) {
        evidence.may_promote = false;
        if let Some(telemetry) = &mut self.telemetry {
            telemetry.write_error_record(SpanRecord::ErrorRaiseFrameCompleted {
                raise_id: evidence.raise,
            });
        }
    }

    /// Unwinding stored the error in a handler's slot of frame `depth`, whose
    /// function the unwinder loaded.
    #[cold]
    pub(super) fn error_caught(
        &mut self,
        evidence: UnwindEvidence,
        depth: usize,
        function: &Function,
        slot: usize,
        handler_pc: usize,
    ) {
        // The same ID a note lookup reads back with `frame_telemetry_function`.
        let handler_function = if evidence.on_path {
            function.telemetry_function_id
        } else {
            self.frame_registered_function(depth)
        };
        let Some(telemetry) = &mut self.telemetry else {
            return;
        };
        telemetry.error_book().land(
            depth,
            handler_function,
            handler_pc,
            slot,
            evidence.raise,
            evidence.origin,
        );
        self.write_unwind_end(
            evidence,
            UnwindResult::Caught,
            handler_function,
            u32::try_from(handler_pc).ok(),
        );
    }

    /// Unwinding stopped without a handler, or failed.
    #[cold]
    pub(super) fn error_not_caught(&mut self, evidence: UnwindEvidence, result: UnwindResult) {
        let Some(telemetry) = &mut self.telemetry else {
            return;
        };
        if matches!(
            result,
            UnwindResult::Unhandled | UnwindResult::EscapedToNative
        ) {
            telemetry.error_book().set_escaped(FutureErrorLink {
                raise: evidence.raise,
                origin: evidence.origin,
            });
        }
        self.write_unwind_end(evidence, result, None, None);
    }

    /// Before a failed spawned thread settles its future, store the raise that
    /// escaped it, so every await of the future can name its origin. Call
    /// before any awaiter can observe the settlement.
    pub fn link_escaped_error_to_future(&mut self, future: bex_vm_types::types::FutureId) {
        let Some(telemetry) = &mut self.telemetry else {
            return;
        };
        if let Some(link) = telemetry.take_escaped_error() {
            telemetry.link_future_error(future.as_usize() as u64, link);
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::{Arc, atomic::AtomicBool};

    use bex_vm_types::Object;
    use btel_records::{SpanRecord, TimingRecord, UnwindResult};
    use btel_settings::mode::AutoTelemetryLevel;
    use btel_types::CallPathId;

    use crate::{
        BexVm, VmExecState,
        telemetry::{
            FORCE_ERROR_EVIDENCE, TelemetryPolicies, TelemetryPolicy, TelemetryState, test_runtime,
        },
    };

    const SOURCE: &str = r#"
        class Failure { code int }
        function Thrower(n: int) -> int throws Failure { throw Failure { code: n } }
        function Main() -> int { Thrower(1) catch (e) { Failure => 0 } }
    "#;

    const THROUGH_SPAN: &str = r#"
        class Failure { code int }
        function Thrower(n: int) -> int throws Failure { throw Failure { code: n } }
        function Middle(n: int) -> int throws Failure { Thrower(n) + 1 }
        function Main() -> int { Middle(1) catch (e) { Failure => 0 } }
    "#;

    const PROMOTE_ERRORS: TelemetryPolicy = TelemetryPolicy {
        promote_errors: true,
        ..TelemetryPolicy::NONE
    };

    const SPAN: TelemetryPolicy = TelemetryPolicy {
        span_from_entry: true,
        ..TelemetryPolicy::NONE
    };

    /// Run `Main` of `SOURCE` at `level`, with `Thrower` promoted on error
    /// when `promote`, returning the exception-path span records.
    fn run(evidence: bool, promote: bool, level: AutoTelemetryLevel) -> Vec<String> {
        let policies: &[(&str, TelemetryPolicy)] = if promote {
            &[("user.Thrower", PROMOTE_ERRORS)]
        } else {
            &[]
        };
        run_source(SOURCE, evidence, level, policies)
    }

    fn run_source(
        source: &str,
        evidence: bool,
        level: AutoTelemetryLevel,
        policies: &[(&str, TelemetryPolicy)],
    ) -> Vec<String> {
        FORCE_ERROR_EVIDENCE.with(|force| force.set(evidence));
        let program = baml_db::testing::compile_source(source);
        let entry = program.function_index("user.Main").unwrap();
        let targets: Vec<_> = policies
            .iter()
            .map(|(name, policy)| (program.function_index(name).unwrap(), *policy))
            .collect();
        let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
        vm.telemetry = Some(TelemetryState::new_root(
            Arc::new(TelemetryPolicies::with_auto_level(level)),
            btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run(),
            test_runtime(),
        ));
        for (index, policy) in targets {
            // SAFETY: the compile-time function is live and never collected.
            let Object::Function(function) = (unsafe { vm.heap.compile_time_ptr(index).get() })
            else {
                unreachable!()
            };
            vm.telemetry
                .as_ref()
                .unwrap()
                .set_policy(function, policy)
                .unwrap();
        }
        vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
        loop {
            match vm.exec().unwrap() {
                VmExecState::EarlyYield => {}
                VmExecState::Complete(_) => break,
                other => panic!("unexpected {other:?}"),
            }
        }
        FORCE_ERROR_EVIDENCE.with(|force| force.set(false));
        let telemetry = vm.telemetry.as_ref().unwrap();
        assert!(telemetry.timing_records().iter().all(|record| matches!(
            record,
            TimingRecord::ThreadSelected { .. } | TimingRecord::FunctionTimingCompletion { .. }
        )));
        telemetry
            .span_records()
            .iter()
            .filter_map(|record| match record {
                SpanRecord::ErrorRaiseOrigin { .. } => Some("origin".into()),
                SpanRecord::ErrorRaiseStack(stack) => {
                    Some(format!("stack frames={}", stack.frames.len()))
                }
                SpanRecord::ErrorRaised {
                    raise_call,
                    raise_frame_may_promote,
                    call_path,
                    frame_count,
                    ..
                } => Some(format!(
                    "raise call={} promote={raise_frame_may_promote} path={} count={frame_count}",
                    raise_call.is_some(),
                    *call_path != CallPathId::ROOT,
                )),
                SpanRecord::ErrorRaisedAndEnded {
                    call_path,
                    frame_count,
                    result,
                    unwound_frames,
                    ..
                } => Some(format!(
                    "raise+end path={} count={frame_count} caught={} frames={unwound_frames}",
                    *call_path != CallPathId::ROOT,
                    *result == UnwindResult::Caught
                )),
                SpanRecord::FunctionSpanCompletionErrored { .. } => Some("failed".into()),
                SpanRecord::LateFunctionSpanCompletionErrored { .. } => Some("late".into()),
                SpanRecord::ErrorRaiseFrameCompleted { .. } => Some("marker".into()),
                SpanRecord::ErrorUnwindEnded {
                    result,
                    unwound_frames,
                    ..
                } => Some(format!(
                    "end caught={} frames={unwound_frames}",
                    *result == UnwindResult::Caught
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_promoted_raise_frame_completes_before_its_marker() {
        assert_eq!(
            run(true, true, AutoTelemetryLevel::Medium),
            [
                "raise call=false promote=true path=true count=2",
                "late",
                "marker",
                "end caught=true frames=1",
            ]
        );
    }

    #[test]
    fn an_observed_stack_is_named_by_its_call_path_alone() {
        // No policy can promote the timing-only raise frame, and no retained
        // call completes: one record for the raise and its end.
        assert_eq!(
            run(true, false, AutoTelemetryLevel::Medium),
            ["raise+end path=true count=2 caught=true frames=1"]
        );
    }

    #[test]
    fn a_retained_call_completes_after_its_raise() {
        // The raise waits through the timing-only frame, then is written
        // before the retained call it failed.
        assert_eq!(
            run_source(
                THROUGH_SPAN,
                true,
                AutoTelemetryLevel::Medium,
                &[("user.Middle", SPAN)]
            ),
            [
                "raise call=false promote=false path=true count=3",
                "failed",
                "end caught=true frames=2",
            ]
        );
    }

    #[test]
    fn unobserved_frames_are_listed_explicitly() {
        // Low telemetry observes neither frame, so no call path covers them.
        assert_eq!(
            run(true, false, AutoTelemetryLevel::Low),
            [
                "stack frames=2",
                "raise+end path=false count=2 caught=true frames=1",
            ]
        );
    }

    #[test]
    fn no_raise_evidence_is_built_without_a_recording() {
        // The shared test runtime has no recording publisher.
        assert_eq!(run(false, true, AutoTelemetryLevel::Medium), ["late"]);
    }
}
