//! Compiled calls under a level that only times them (medium, with no
//! policy published): a plain callee runs plain between two clock reads, and
//! its completion adds to its call path's aggregate in place
//! ([`crate::telemetry::timed`]).
//!
//! Replaces, for those calls, the frame record, the receipt and the
//! `FunctionTimingCompletion` record per call (`call_record`,
//! `enter_record_receipt`, `complete_record`). The call path is the one the
//! record path resolves: the site's key (calling body, call instruction,
//! callee) is fixed in the generated code, and the site resolves it once per
//! parent path.

use btel_types::CallPathId;
use bex_vm_types::{HeapPtr, ObjectIndex};

use super::FrameRecord;
use crate::{BexVm, errors::VmError, telemetry::timed::TimedPath};

/// A timed call in progress.
#[derive(Clone, Copy)]
pub struct Timed {
    path: TimedPath,
    saved: CallPathId,
    /// The timer's reading at entry.
    entered: u64,
}

impl BexVm {
    /// Whether a compiled call to a plain body may run it timed
    /// ([`Self::timed_enter`]): the level only times calls and direct calls
    /// are on.
    #[allow(clippy::inline_always, reason = "checked at every plain call site")]
    #[inline(always)]
    #[must_use]
    pub fn timing_calls(&self) -> bool {
        self.compiled.call_mode == super::state::CallMode::Timed && self.direct_calls()
    }

    /// Settle how compiled calls run for the slice of execution `exec`
    /// starts: plain when nothing observes calls, timed when the level only
    /// times them, else with records. A policy published while a slice runs
    /// takes effect at the next slice.
    pub(crate) fn settle_call_mode(&mut self) {
        use super::state::CallMode;
        self.compiled.call_mode = match &self.telemetry {
            None => CallMode::Plain,
            Some(telemetry) if telemetry.observes_none() => CallMode::Plain,
            Some(telemetry) if telemetry.times_only() => CallMode::Timed,
            Some(_) => CallMode::Recorded,
        };
    }

    /// Enter a timed call from the compiled body of object `caller`, at its
    /// call instruction `pc`, to the compiled body of object `callee`, at
    /// call site `site`. `None` when telemetry does not record the callee:
    /// the call then takes its record path.
    #[inline]
    pub fn timed_enter(
        &mut self,
        fr: &FrameRecord,
        site: usize,
        caller: usize,
        pc: u32,
        callee: usize,
    ) -> Option<Timed> {
        let telemetry = self.telemetry.as_ref()?;
        let saved = telemetry.active_call_path();
        let path = match telemetry.cached_site(site) {
            Some(path) => path,
            None => self.resolve_site(fr, site, caller, pc, callee)?,
        };
        let telemetry = self.telemetry.as_mut()?;
        let entered = telemetry.timer_now();
        telemetry.set_active_call_path(path.path);
        Some(Timed {
            path,
            saved,
            entered,
        })
    }

    #[cold]
    #[inline(never)]
    fn resolve_site(
        &mut self,
        fr: &FrameRecord,
        site: usize,
        caller: usize,
        pc: u32,
        callee: usize,
    ) -> Option<TimedPath> {
        let caller = self.record_object(fr, ObjectIndex::from_raw(caller));
        let callee = self.record_object(fr, ObjectIndex::from_raw(callee));
        // SAFETY: the callee is a compile-time function the program holds.
        #[allow(unsafe_code)]
        let function = unsafe { self.load_callable(callee) }.ok()?;
        if !self.telemetry.as_ref()?.may_record(function) {
            return None;
        }
        let visible_caller = self.callable_function_identity(caller);
        let callee = self.callable_function_identity(callee).unwrap_or(callee);
        let heap = &self.heap;
        let telemetry = self.telemetry.as_mut()?;
        // SAFETY: the heap permit is held, and both functions are
        // compile-time objects.
        #[allow(unsafe_code)]
        let path = telemetry.resolve_timed(visible_caller, pc, callee, |caller, callee| unsafe {
            Self::register_call_path_functions(heap, caller, callee)
        });
        telemetry.cache_site(site, path);
        Some(path)
    }

    /// The timed call returned.
    #[inline]
    pub fn timed_exit(&mut self, timed: Timed) {
        if let Some(telemetry) = self.telemetry.as_mut() {
            telemetry.add_timed(timed.path.node, timed.entered);
            telemetry.set_active_call_path(timed.saved);
        }
    }

    /// An error left the timed call to `callee` that a plain call ran
    /// under `caller`: its zombie carries the receipt the record path would
    /// have made, so the unwinder completes it with the interpreter's
    /// outcome.
    #[cold]
    #[inline(never)]
    pub fn timed_unwound(
        &mut self,
        caller: &FrameRecord,
        callee: HeapPtr,
        timed: Timed,
        error: Box<VmError>,
    ) -> Box<VmError> {
        let receipt = self.telemetry.as_mut().map(|telemetry| {
            telemetry.timed_receipt(timed.entered, timed.saved, timed.path.reentry)
        });
        self.unwound_with(caller, callee, error, receipt)
    }
}
