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

use super::{BexVm, Frame};
use crate::{compiled_rt::FrameAt, telemetry::LandingMatch};

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

/// Whether any instruction of the function stores into frame-local `local`.
/// Conservative: undecodable code counts as a store.
fn writes_local(code: &bex_vm_types::bytecode::CompactCode, local: usize) -> bool {
    let operand = |at: usize| -> Option<usize> {
        let bytes = code.code.get(at..at + 4)?;
        usize::try_from(u32::from_le_bytes(bytes.try_into().ok()?)).ok()
    };
    let mut pc = 0;
    while pc < code.code.len() {
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
}

impl BexVm {
    /// The underlying function's telemetry ID, without registering it.
    fn frame_telemetry_function(&self, frame: FrameAt) -> Option<FunctionId> {
        let ptr = self.frame_identity(frame)?;
        match self.get_object(ptr) {
            Object::Function(function) => function.telemetry_function_id,
            _ => None,
        }
    }

    /// Register and return the frame's function ID before a record names it.
    fn frame_registered_function(&self, frame: FrameAt) -> Option<FunctionId> {
        let ptr = self.frame_identity(frame)?;
        // SAFETY: unwinding holds the heap permit and the frame roots `ptr`.
        unsafe { self.heap.register_telemetry_function(ptr) }
    }

    fn raise_kind(
        entry: RaiseEntry,
        function: Option<&Function>,
        pc: Option<usize>,
        thrown: &VmThrown,
    ) -> RaiseKind {
        if entry == RaiseEntry::Host {
            return RaiseKind::HostBoundary;
        }
        let opcode = function
            .and_then(|function| function.bytecode.compact.as_ref()?.code.get(pc?))
            .and_then(|byte| OpCode::try_from(*byte).ok());
        match opcode {
            Some(OpCode::Throw) => RaiseKind::Throw,
            Some(OpCode::Rethrow) => RaiseKind::Rethrow,
            Some(OpCode::Await) if thrown.throw_kind == ThrowKind::Rethrow => RaiseKind::Await,
            Some(OpCode::Await) => RaiseKind::AwaitCancelled,
            Some(
                OpCode::Call
                | OpCode::CallHooked
                | OpCode::CallExactArgs
                | OpCode::CallIndirect
                | OpCode::VirtualCall
                | OpCode::SysOp,
            ) => RaiseKind::NativeBoundary,
            _ => RaiseKind::Runtime,
        }
    }

    /// The raise a rethrow continues, found by the `baml.errors.Context` it
    /// carries: the landing whose context slot, in a live frame, still holds
    /// that object. `None` when the unwinder did not carry a context, which
    /// makes it a new throw.
    fn rethrow_origin(&self, carried: Option<bex_vm_types::Value>) -> RaiseOrigin {
        let unresolved = RaiseOrigin::Unresolved {
            reason: UnresolvedOrigin::NoLanding,
            via: OriginVia::Rethrow,
        };
        let Some(context) = carried else {
            return unresolved;
        };
        let Some(book) = self
            .telemetry
            .as_ref()
            .and_then(|telemetry| telemetry.error_book_ref())
        else {
            return unresolved;
        };
        let live: Vec<(usize, Option<FunctionId>)> = self
            .stack_view()
            .into_iter()
            .enumerate()
            .filter(|(_, frame)| self.frame_is_bytecode(*frame))
            .map(|(level, frame)| (level, self.frame_telemetry_function(frame)))
            .collect();
        let stack = &self.stack.0;
        from_match(
            book.lookup(&live, |slot| slot < stack.len() && stack[slot] == context),
            OriginVia::Rethrow,
        )
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
        let frames = self.stack_view();
        let live = raise_frame.and_then(|level| match frames.get(level) {
            Some(FrameAt::Vm(idx)) => Some(*idx),
            _ => None,
        });
        frames
            .into_iter()
            .rev()
            .filter(|frame| {
                !matches!(frame, FrameAt::Vm(idx) if self
                    .pending_trace_hooks
                    .iter()
                    .any(|hook| hook.frame == *idx && hook.evaluating))
            })
            .take(MAX_ERROR_FRAMES)
            .map(|frame| match frame {
                FrameAt::Vm(idx) if matches!(self.frames[idx], Frame::Native(_)) => {
                    let Frame::Native(native) = &self.frames[idx] else {
                        unreachable!()
                    };
                    ErrorFrame {
                        function: match self.get_object(native.function) {
                            // SAFETY: as above; native frames root their function.
                            Object::Function(_) => unsafe {
                                self.heap.register_telemetry_function(native.function)
                            },
                            _ => None,
                        },
                        pc: None,
                        native: true,
                    }
                }
                _ => ErrorFrame {
                    function: self.frame_registered_function(frame),
                    pc: self
                        .frame_at_pc(frame, live)
                        .map(|pc| u32::try_from(pc).unwrap_or(u32::MAX)),
                    native: false,
                },
            })
            .collect()
    }

    /// Whether every bytecode frame on this thread has telemetry, so each one
    /// is on the active call path: its callers name the whole stack, native
    /// frames and direct recursion aside. Reads only the frames the unwinder
    /// just walked; never a function object.
    fn stack_is_observed(&self) -> bool {
        self.stack_view()
            .into_iter()
            .all(|frame| !self.frame_is_bytecode(frame) || self.frame_receipt(frame).is_some())
    }

    /// Record the start of an unwind. `carried_context` is the
    /// `baml.errors.Context` the error already carries (a caught error raised
    /// again, one awaited from another task, or, when `converted`, a
    /// conversion of the error a handler is handling): its trace becomes
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
        carried_context: Option<bex_vm_types::Value>,
        converted: bool,
    ) -> Option<UnwindEvidence> {
        if !self
            .telemetry
            .as_ref()
            .is_some_and(crate::telemetry::TelemetryState::error_evidence)
        {
            return None;
        }
        let live = self.level_count();
        if let Some(telemetry) = &mut self.telemetry {
            telemetry.error_book().begin_raise(live);
        }
        let raise_id = allocate_telemetry_id();
        let frames = self.stack_view();
        let raise_frame = frames
            .iter()
            .rposition(|frame| self.frame_is_bytecode(*frame));
        let raise_at = raise_frame.map(|level| frames[level]);
        let raise_function = match (raise_at, current) {
            (Some(FrameAt::Vm(frame)), Some((idx, function))) if frame == idx => Some(function),
            // SAFETY: the bytecode frame roots its function under the permit.
            (Some(frame), _) => unsafe { self.frame_function(frame) }.ok(),
            (None, _) => None,
        };
        // Compiled records report instruction indices; resolve those to the
        // original compact bytecode offset before classifying the raise.
        // Values and ThrowKind do not distinguish a source throw from a
        // failing native call, or an await from a rethrow.
        let pc = raise_at.and_then(|frame| self.frame_at_pc(frame, self.live_bytecode_frame()));
        let kind = Self::raise_kind(entry, raise_function, pc, thrown);
        let origin = match kind {
            RaiseKind::Rethrow | RaiseKind::PanicRethrow => self.rethrow_origin(carried_context),
            RaiseKind::Await => self.await_origin(),
            // The converted value's source is only known by value. A catch
            // slot holding an equal value, even a running catch's, does not
            // show that the conversion read it, so the source's trace is kept
            // as inherited evidence and no origin is claimed.
            _ if converted => RaiseOrigin::Unresolved {
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
        let raise_telemetry = raise_at.and_then(|frame| self.frame_receipt(frame));
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
                raise_at.and_then(|frame| self.frame_registered_function(frame)),
                self.error_frames(raise_frame),
            )
        };
        // The trace the error already carries, read from its context only when
        // this raise inherits it.
        let inherited_trace: Option<Vec<StackFrame>> = match origin {
            RaiseOrigin::Fresh | RaiseOrigin::Proven { .. } => None,
            _ => carried_context.and_then(|context| self.context_trace(context).ok()),
        };
        let inherited: Vec<InheritedFrame> = inherited_trace
            .as_deref()
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
        let pc = pc.map(|pc| u32::try_from(pc).unwrap_or(u32::MAX));
        let frame_count = u32::try_from(self.level_count()).unwrap_or(u32::MAX);
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

    /// Start an unwind, or continue the same unwind delivered through a compiled body.
    pub(super) fn exception_evidence(
        &mut self,
        thrown: &VmThrown,
        entry: RaiseEntry,
        current: Option<(usize, &'static Function)>,
    ) -> Result<(super::InFlightContext, Option<UnwindEvidence>), super::VmError> {
        let (context, converted) = self.in_flight_context(thrown.context, thrown.value)?;
        let carried = match &context {
            super::InFlightContext::Carried(context) => Some(*context),
            super::InFlightContext::Fresh { .. } => None,
        };
        let evidence = self
            .compiled
            .error_evidence
            .take()
            .or_else(|| self.begin_error_evidence(thrown, entry, current, carried, converted));
        Ok((context, evidence))
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

    /// Unwinding landed the error in a handler of frame `depth`, whose
    /// function the unwinder loaded, and stored its context in frame-local
    /// `context_local` (absolute stack index `context_slot`).
    #[cold]
    pub(super) fn error_caught(
        &mut self,
        evidence: UnwindEvidence,
        depth: usize,
        function: &Function,
        context_local: usize,
        context_slot: usize,
        handler_pc: usize,
    ) {
        let trusted = function
            .bytecode
            .compact
            .as_ref()
            .is_some_and(|code| !writes_local(code, context_local));
        self.error_landed(
            evidence,
            FrameAt::Vm(depth),
            Some((context_slot, trusted)),
            handler_pc,
        );
    }

    /// Shared landing bookkeeping for bytecode slots and compiled handler roots.
    pub(super) fn error_landed(
        &mut self,
        evidence: UnwindEvidence,
        frame: FrameAt,
        context: Option<(usize, bool)>,
        handler_pc: usize,
    ) {
        let handler_function = if evidence.on_path {
            self.frame_telemetry_function(frame)
        } else {
            self.frame_registered_function(frame)
        };
        let level = self.level_of(frame);
        let Some(telemetry) = &mut self.telemetry else {
            return;
        };
        if let Some((context_slot, trusted)) = context {
            telemetry.error_book().land(
                level,
                handler_function,
                handler_pc,
                context_slot,
                trusted,
                evidence.raise,
                evidence.origin,
            );
        }
        self.write_unwind_end(
            evidence,
            UnwindResult::Caught,
            handler_function,
            u32::try_from(handler_pc).ok(),
        );
    }

    /// A hook failure was handled by the VM's transparent decision boundary.
    #[cold]
    pub(super) fn error_hook_fallback(&mut self, evidence: UnwindEvidence) {
        self.write_unwind_end(evidence, UnwindResult::Caught, None, None);
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

    #[test]
    fn host_error_in_suspended_compiled_frame_is_a_host_boundary() {
        use bex_vm_types::{Value, errors::VmThrown};

        use super::{Frame, RaiseEntry, RaiseKind};
        use crate::compiled_rt::{CompiledAt, CompiledEntry, stub, stub_start};

        let mut program = baml_test_support::compile_source("function Main() -> int { 0 }");
        let entry = program.rendered_callables()["user.Main"].object;
        let Object::Function(function) = &mut program.objects[entry] else {
            unreachable!()
        };
        function.compiled_body = Some(
            CompiledEntry {
                object: 0,
                name: "user.Main",
                arity: 0,
                body: |_, _, _| unreachable!(),
                slots: 0,
                cause_slot: None,
                framed: false,
            }
            .erase(),
        );
        let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
        vm.telemetry = Some(TelemetryState::new_root(
            Arc::new(TelemetryPolicies::with_auto_level(AutoTelemetryLevel::Low)),
            btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run(),
            test_runtime(),
        ));
        vm.set_entry_point(vm.heap.compile_time_ptr(entry.raw()), &[]);
        // SAFETY: the entry frame roots the function under the heap permit.
        let function = unsafe { vm.load_function(0) }.unwrap();
        let Frame::Bytecode(frame) = &mut vm.frames[0] else {
            unreachable!()
        };
        frame.instruction_ptr = stub_start(function).unwrap() + stub::AFTER_CALL;
        assert_eq!(vm.compiled_at(0), Some(CompiledAt::Suspended));

        FORCE_ERROR_EVIDENCE.with(|force| force.set(true));
        let evidence = vm.begin_error_evidence(
            &VmThrown::fresh(Value::int(1)),
            RaiseEntry::Host,
            None,
            None,
            false,
        );
        FORCE_ERROR_EVIDENCE.with(|force| force.set(false));
        assert_eq!(
            evidence.unwrap().held.unwrap().kind,
            RaiseKind::HostBoundary
        );
    }

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
        let program = baml_test_support::compile_source(source);
        let callables = program.rendered_callables();
        let entry = callables["user.Main"].object.raw();
        let targets: Vec<_> = policies
            .iter()
            .map(|(name, policy)| (callables[*name].object.raw(), *policy))
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
