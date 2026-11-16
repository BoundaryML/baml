//! Frameless calls: a compiled call that links no frame record.
//!
//! The compiler (`baml_compiler2_rust::admit`) decides which callees qualify
//! (`analysis::frameless_body`). A body that cannot pause runs on its
//! caller's record ([`BexVm::call_frameless`]). One that may pause runs on a
//! *pending* record ([`BexVm::call_pending`]): the record the record path
//! would build, held in the call's Rust frame but not linked into the chain
//! or counted. The body uses it as its own `fr`; its slots start at that
//! record's base, where the record path puts them.
//!
//! **Materialization.** A pending record is linked as soon as anything
//! outside the body could see the chain: before every suspension
//! ([`BexVm::suspend_from`]) and at the start of every runtime call that
//! extends the chain. [`BexVm::materialize`] links the pending ancestors
//! above the nearest linked record, oldest first, so the chain matches the
//! record path's (functions, pcs, levels, anchors, bases), and the engine,
//! GC root scan, cancellation and resume see every active function.
//!
//! **Spills.** A pausing body spills its live heap values to its own slots,
//! reserved at entry. A pending record's base never changes when it is linked
//! late, so every ancestor's live values sit in VM stack slots below the
//! suspended body's, and a moving collection forwards them in place. A
//! callee that cannot pause writes no slots.
//!
//! **Depth.** Every frameless call keeps the record path's native-stack
//! check ([`BexVm::stack_room`]) at the same point, so `StackOverflow` fires
//! at the same call.
//!
//! **Errors.** A frameless callee that fails leaves the zombie record it
//! would have had, linked above its nearest linked ancestor (see
//! `link_above`), so unwinding, `catch (e, ctx)` and uncaught traces
//! see the record path's chain.

use std::{cell::Cell, ptr};

use bex_vm_types::{HeapPtr, ObjectIndex, RealizedTy, Value};

use super::{FrameRecord, RecordChain, Step, record_ref};
use crate::{
    errors::VmError,
    vm::{BexVm, Frame},
};

impl BexVm {
    /// Whether a frameless callee may run without a record: direct calls
    /// are on and telemetry is off.
    #[allow(clippy::inline_always, reason = "checked at every frameless call site")]
    #[inline(always)]
    #[must_use]
    pub fn frameless_calls(&self) -> bool {
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
        let result = self
            .stack_room()
            .and_then(|()| body(self).inspect_err(|_| self.frameless_unwound(caller, callee)));
        caller.note_pc(pc);
        result
    }

    /// Link `fr`'s pending records, then suspend: what every suspension in
    /// compiled code runs, so whatever handles it sees the record path's
    /// chain.
    pub fn suspend_from(&mut self, fr: &FrameRecord, step: Step) -> Result<Value, Box<VmError>> {
        self.materialize(fr);
        self.suspend(step)
    }

    /// The callable a call of `caller`'s own function `callee` runs: a
    /// record other than an entry record holds it already, so it needs no
    /// lookup.
    #[allow(clippy::inline_always, reason = "the frameless call is the fast path")]
    #[inline(always)]
    #[must_use]
    pub fn own_callable(&self, caller: &FrameRecord, callee: ObjectIndex) -> HeapPtr {
        if caller.entry {
            self.record_object(caller, callee)
        } else {
            caller.function()
        }
    }

    /// Run `body`, a frameless callee that may poll, called at `pc`, on a
    /// pending record of its own that runs `function` with `type_args`
    /// (borrowed from the caller's call expression, as
    /// [`Self::call_record_typed`] borrows them). See the module notes.
    #[allow(
        clippy::inline_always,
        clippy::cast_possible_truncation,
        reason = "the frameless call is the fast path; levels and stack offsets are bounded \
                  by the thread stack budget, so they fit u32"
    )]
    #[inline(always)]
    pub fn call_pending<T>(
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
            raise_zombie(record, added, &mut self.manual_activations);
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

/// Move a zombie `added` levels up, with the manual span activations scoped
/// to it.
fn raise_zombie(record: &FrameRecord, added: u32, activations: &mut [crate::vm::ManualActivation]) {
    let old = record.level();
    record.level.set(record.level.get() + added);
    if !record.scoped.get() {
        return;
    }
    for active in activations {
        if active.record == Some(old) {
            active.record = Some(old + added as usize);
        }
    }
}
