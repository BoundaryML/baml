//! Frameless calls: a compiled call that links no frame record.
//!
//! **Condition.** The compiler (`baml_compiler2_rust::admit`) calls a callee
//! frameless when it is a compiled function that keeps no VM-frame state
//! (not framed), has no catch handler or `defer` (so no handler slots), and
//! calls no `trace` builtin (which reads or scopes to the innermost frame).
//! It may pause.
//!
//! **Two shapes.** A body that cannot pause runs on its caller's record,
//! `fr` ([`BexVm::call_frameless`]): it writes only `fr`'s pc and has no
//! slots. A body that may pause runs on a *pending* record
//! ([`BexVm::call_pending`], or [`BexVm::call_pending_typed`] with the
//! call's type arguments): the record it would have had, built in the
//! call's Rust frame (callee, package, anchor, base at the call, `prev` =
//! the caller's record, pending or not), but not linked into the chain and
//! not counted, so the call costs no chain or count updates and no telemetry
//! check. The body sees it as its own `fr`: it notes its pcs there, and its
//! slots (spills) start at that base, above the caller's, exactly where the
//! record path puts them. The caller notes its call site before the call,
//! as the record path does.
//!
//! **Materialization.** A pending record is linked when something outside
//! the body could see the chain: before every suspension
//! ([`BexVm::suspend_from`]: a yield poll, an await, a log, a call the VM
//! runs, a dispatch fallback), and at the start of every call that extends
//! the chain from a compiled body (`call_record*`, `call_compiled`,
//! `call_closure_compiled`, `call_inline`, `log_event`).
//! [`BexVm::materialize`] walks `prev` from `fr` through the pending
//! ancestors to the nearest linked record `L`, and links them above `L`
//! oldest first, each one level above its `prev`, so the chain is the one the
//! record path would have built: same functions, pcs (each caller's call
//! site), levels, anchors and bases. The engine, GC root scan, `stack_view`,
//! cancellation delivery and resume therefore see every active function.
//! After that the records stay linked; each is still owned by its call's
//! Rust frame, which pops it (top back to `prev`, count down by one) when it
//! returns, or when a fiber dropped while parked unwinds it. A later
//! materialization links only what is still pending, so a return pops
//! exactly what was linked for its own call.
//!
//! **Spills.** A body that may pause spills its live heap values at its
//! suspensions and around calls to callees that may pause, as any pausing
//! body does, to its own slots: `fr.base() + i`, reserved at entry (`reserve_slots`). The
//! base of a pending record is the stack height at the call, the base the
//! record path gives the same call, and it does not change when the record is
//! linked late: materialization only links. So every ancestor's live values
//! are in VM stack slots below the suspended body's, inside the slot range
//! of the record linked for it, and a moving collection forwards them in
//! place. A callee that cannot pause writes no slots, so running it on its
//! caller's record (pending or linked) disturbs none.
//!
//! **Depth.** Every frameless call keeps the native-stack check of a record
//! call ([`BexVm::stack_room`]) at the same point, so `StackOverflow` fires at
//! the same call as on the record path; levels (`level_count`, `stack_view`)
//! never count a frameless body that has not been materialized, as nothing
//! can observe it.
//!
//! **Errors.** A frameless callee that returns `Err` leaves the zombie record
//! it would have had. For a body on a pending record that record is linked
//! as before a suspension (with its pending ancestors), then boxed in place
//! ([`BexVm::zombify`]); a materialized one is only boxed. For a body on its
//! caller's record, [`BexVm::call_frameless`] builds the zombie: function,
//! the pc the body reported (its exit writes it into `fr`), anchor and base as
//! at the call, and links it above `fr` (materializing `fr` first if it is
//! pending); `fr` then notes the call site, as the record path noted it
//! before the call. Both link through one routine, which places the new
//! records directly above their linked ancestor, below the zombies their
//! own callees left, whose levels rise by the number linked. The unwinder,
//! `catch (e, ctx)` and uncaught traces therefore see the same chain as with
//! records.
//!
//! **Telemetry.** A call records its visible caller, the caller's pc, and the
//! span its callees nest in from the record chain, so with telemetry on
//! ([`BexVm::frameless_calls`] is false) every call takes the record path and
//! records exactly what it did before.
//!
//! **Escape hatch.** `BAML_COMPILED_FRAMES=1` turns off direct calls, frameless
//! ones included: every compiled call then runs in a VM frame.

use std::{cell::Cell, ptr};

use bex_vm_types::{HeapPtr, ObjectIndex, RealizedTy, Value};

use super::{FrameRecord, RecordChain, Step, record_ref};
use crate::{
    errors::VmError,
    vm::{BexVm, Frame},
};

#[cfg(test)]
thread_local! {
    /// Take the record path at frameless call sites, to compare the two.
    pub(crate) static FORCE_RECORDS: Cell<bool> = const { Cell::new(false) };
}

impl BexVm {
    /// Whether a frameless callee may run without a record: direct calls
    /// are on and telemetry is off.
    #[allow(clippy::inline_always, reason = "checked at every frameless call site")]
    #[inline(always)]
    #[must_use]
    pub fn frameless_calls(&self) -> bool {
        #[cfg(test)]
        if FORCE_RECORDS.with(Cell::get) {
            return false;
        }
        self.direct_calls() && self.telemetry.is_none()
    }

    /// Run `body`, a frameless callee (object `callee` of `caller`'s package)
    /// that cannot poll, called at `pc`, on `caller`'s record. See the module
    /// notes.
    #[allow(clippy::inline_always, reason = "the frameless call is the fast path")]
    #[inline(always)]
    pub fn call_frameless<T>(
        &mut self,
        caller: &FrameRecord,
        callee: ObjectIndex,
        pc: u32,
        body: impl FnOnce(&mut BexVm) -> Result<T, Box<VmError>>,
    ) -> Result<T, Box<VmError>> {
        let result = match self.stack_room() {
            Ok(()) => {
                let result = body(self);
                if result.is_err() {
                    self.frameless_unwound(caller, callee);
                }
                result
            }
            Err(error) => Err(error),
        };
        caller.note_pc(pc);
        result
    }

    /// Run `body`, a frameless callee that may poll (object `callee` of
    /// `caller`'s package), called at `pc`, on a pending record of its own.
    /// See the module notes.
    #[allow(clippy::inline_always, reason = "the frameless call is the fast path")]
    #[inline(always)]
    pub fn call_pending<T>(
        &mut self,
        caller: &FrameRecord,
        callee: ObjectIndex,
        pc: u32,
        body: impl FnOnce(&mut BexVm, &FrameRecord) -> Result<T, Box<VmError>>,
    ) -> Result<T, Box<VmError>> {
        let function = self.record_object(caller, callee);
        self.call_pending_to(caller, function, &[], pc, body)
    }

    /// [`Self::call_pending`] for a callee whose frame starts with the
    /// call's type arguments, borrowed from the caller's call expression as
    /// [`Self::call_record_typed`] borrows them.
    #[allow(clippy::inline_always, reason = "the frameless call is the fast path")]
    #[inline(always)]
    pub fn call_pending_typed<T>(
        &mut self,
        caller: &FrameRecord,
        callee: ObjectIndex,
        type_args: &[RealizedTy],
        pc: u32,
        body: impl FnOnce(&mut BexVm, &FrameRecord) -> Result<T, Box<VmError>>,
    ) -> Result<T, Box<VmError>> {
        let function = self.record_object(caller, callee);
        self.call_pending_to(caller, function, type_args, pc, body)
    }

    /// Link `fr`'s pending records, then suspend: what every suspension in
    /// compiled code runs, so whatever handles it sees the record path's
    /// chain.
    pub fn suspend_from(&mut self, fr: &FrameRecord, step: Step) -> Result<Value, Box<VmError>> {
        self.materialize(fr);
        self.suspend(step)
    }

    /// [`Self::call_pending`] for a call of the caller's own function: a
    /// record other than an entry record holds the callable its call
    /// resolved in the same package, so it needs no lookup.
    #[allow(clippy::inline_always, reason = "the frameless call is the fast path")]
    #[inline(always)]
    pub fn call_pending_self<T>(
        &mut self,
        caller: &FrameRecord,
        callee: ObjectIndex,
        pc: u32,
        body: impl FnOnce(&mut BexVm, &FrameRecord) -> Result<T, Box<VmError>>,
    ) -> Result<T, Box<VmError>> {
        let function = if caller.entry {
            self.record_object(caller, callee)
        } else {
            caller.function()
        };
        self.call_pending_to(caller, function, &[], pc, body)
    }

    #[allow(
        clippy::inline_always,
        clippy::cast_possible_truncation,
        reason = "the frameless call is the fast path; levels and stack offsets are bounded \
                  by the thread stack budget, so they fit u32"
    )]
    #[inline(always)]
    fn call_pending_to<T>(
        &mut self,
        caller: &FrameRecord,
        function: HeapPtr,
        type_args: &[RealizedTy],
        pc: u32,
        body: impl FnOnce(&mut BexVm, &FrameRecord) -> Result<T, Box<VmError>>,
    ) -> Result<T, Box<VmError>> {
        caller.note_pc(pc);
        self.stack_room()?;
        let base = self.stack.len();
        let record = FrameRecord {
            prev: Cell::new(ptr::from_ref(caller)),
            function: Cell::new(function),
            package: Cell::new(caller.package.get()),
            pc: Cell::new(0),
            anchor: self.frames.len() as u32,
            base: base as u32,
            level: Cell::new(PENDING),
            entry: false,
            scoped: Cell::new(false),
            receipt: Cell::new(None),
            type_args,
        };
        let unlink = Unlink {
            chain: &raw const *self.compiled.records,
            record: &record,
        };
        let result = body(self, &record);
        if result.is_ok() {
            self.stack.truncate(base);
            drop(unlink);
        } else {
            // The record path's zombie: linked where it stands, with its
            // pending callers, then boxed in place.
            std::mem::forget(unlink);
            self.materialize(&record);
            let linked: *const FrameRecord = &raw const record;
            self.zombify(linked, record);
        }
        result
    }

    /// Link `record` and its pending ancestors into the chain, if it is
    /// pending, as the record path would have: before a body on it suspends.
    #[allow(clippy::inline_always, reason = "one test at every poll that yields")]
    #[inline(always)]
    pub fn materialize(&mut self, record: &FrameRecord) {
        if record.is_pending() {
            self.link_pending(record);
        }
    }

    #[cold]
    #[inline(never)]
    fn link_pending(&mut self, record: &FrameRecord) {
        let mut pending = Vec::new();
        let mut at: *const FrameRecord = record;
        while record_ref(at).is_pending() {
            pending.push(at);
            at = record_ref(at).prev();
        }
        pending.reverse();
        self.link_above(record_ref(at), &pending);
    }

    /// Link `records`, oldest first, each `prev` of the next, the first's
    /// `prev` being `at`, directly above the linked record `at`: below the
    /// zombies standing above it, whose levels rise by as many.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "levels are bounded by the thread stack budget, so they fit u32"
    )]
    fn link_above(&mut self, at: &FrameRecord, records: &[*const FrameRecord]) {
        let (Some(&first), Some(&last)) = (records.first(), records.last()) else {
            return;
        };
        debug_assert!(ptr::eq(record_ref(first).prev(), at));
        let chain = &*self.compiled.records;
        let added = records.len() as u32;
        let mut above: Option<&FrameRecord> = None;
        for record in chain.iter() {
            if ptr::eq(record, at) {
                break;
            }
            debug_assert!(
                self.is_zombie(record),
                "only zombies stand above a running caller"
            );
            let old = record.level();
            record.level.set(record.level.get() + added);
            if record.scoped.get() {
                for active in &mut self.manual_activations {
                    if active.record == Some(old) {
                        active.record = Some(old + added as usize);
                    }
                }
            }
            above = Some(record);
        }
        for &record in records {
            let record = record_ref(record);
            record.level.set(record_ref(record.prev()).level.get() + 1);
        }
        match above {
            Some(record) => record.prev.set(last),
            None => chain.top.set(last),
        }
        chain.count.set(chain.count.get() + added);
    }

    /// Link the zombie record a frameless callee that returned an error would
    /// have left, directly above `caller`.
    #[cold]
    #[inline(never)]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "levels and stack offsets are bounded by the thread stack budget, so they fit u32"
    )]
    fn frameless_unwound(&mut self, caller: &FrameRecord, callee: ObjectIndex) {
        self.materialize(caller);
        let function = self.record_object(caller, callee);
        // The callee's base is the stack height at the call: the callee pushes
        // nothing of its own, and what its callees left starts above it.
        let mut base = self.stack.len();
        let at: *const FrameRecord = caller;
        for record in self.chain().iter() {
            if ptr::eq(record, at) {
                break;
            }
            base = base.min(record.base());
        }
        for frame in self.frames.iter().skip(caller.anchor()) {
            if let Frame::Bytecode(frame) = frame {
                base = base.min(frame.locals_offset.raw());
            }
        }
        let zombie = Box::new(FrameRecord {
            prev: Cell::new(at),
            function: Cell::new(function),
            package: Cell::new(caller.package.get()),
            pc: Cell::new(caller.pc()),
            anchor: caller.anchor,
            base: base as u32,
            level: Cell::new(PENDING),
            entry: false,
            scoped: Cell::new(false),
            receipt: Cell::new(None),
            type_args: &[],
        });
        self.link_above(caller, &[&raw const *zombie]);
        self.compiled.records.zombies().push(zombie);
    }
}

/// The level of a pending record: one a frameless call built for its callee
/// but has not linked.
const PENDING: u32 = u32::MAX;

impl FrameRecord {
    /// Whether the record is pending: built by [`BexVm::call_pending`] and
    /// not yet linked.
    #[allow(
        clippy::inline_always,
        reason = "tested at every pending call's return"
    )]
    #[inline(always)]
    pub(crate) fn is_pending(&self) -> bool {
        self.level.get() == PENDING
    }
}

/// Pops a pending record that was materialized when its call ends normally or
/// is unwound by a fiber drop; see `Pop` in `record.rs`. An error path
/// disarms it and leaves a zombie instead.
struct Unlink<'a> {
    chain: *const RecordChain,
    record: &'a FrameRecord,
}

impl Drop for Unlink<'_> {
    #[allow(clippy::inline_always, reason = "the pending call's return")]
    #[inline(always)]
    fn drop(&mut self) {
        if !self.record.is_pending() {
            // SAFETY: the chain is boxed in `CompiledState`, which drops the
            // fiber (and so unwinds this guard) before the chain.
            #[allow(unsafe_code)]
            let chain = unsafe { &*self.chain };
            pop_materialized(chain, self.record);
        }
    }
}

#[cold]
#[inline(never)]
fn pop_materialized(chain: &RecordChain, record: &FrameRecord) {
    debug_assert!(
        ptr::eq(chain.top.get(), record),
        "a returning body's record is the innermost"
    );
    chain.top.set(record.prev());
    chain.count.set(chain.count.get() - 1);
}
