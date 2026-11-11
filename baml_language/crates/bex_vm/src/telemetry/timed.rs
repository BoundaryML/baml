//! Compiled code's timing producer: calls a level times but does not span,
//! counted in place.
//!
//! The shape is the old runtime branch's (`brr/runtime-v2` `rt/telemetry.rs`):
//! a direct call site's key is fixed when the module is generated (calling
//! body, call instruction, callee), so the site resolves its call path once
//! per parent path ([`crate::vm::compiled_rt`]'s site cache) rather than per
//! call, and each completion adds to a per-path aggregate in place. The
//! aggregates are written to the existing recorder as
//! [`TimingRecord::FunctionTimingAggregate`]s when compiled code returns to
//! the engine ([`TelemetryState::flush_timed`]): one record per path where
//! the interpreter writes one `FunctionTimingCompletion` per call, which
//! the processor sums to the same aggregate.
//!
//! Only successful completions are counted here. A timed call an error
//! leaves becomes an ordinary receipt ([`TelemetryState::timed_receipt`]),
//! which the unwinder completes with the interpreter's outcome.

use btel_records::TimingRecord;
use btel_settings::mode::AutoTelemetryLevel;
use btel_types::{CallPathId, ClockDuration, ClockInstant, FunctionId};
use bex_vm_types::HeapPtr;
use rustc_hash::FxHashMap;

use super::{CallPathEdge, CallPathKey, FrameTelemetry, REENTRY, TelemetryState};

/// The in-place aggregates of one VM's timed calls, by call-path node.
#[derive(Default)]
pub(crate) struct TimedCounts {
    nodes: Vec<TimedNode>,
    by_node: FxHashMap<(CallPathId, bool), u32>,
    /// Per compiled call site: the parent path it last resolved under and
    /// where it went. Kept with the counts, whose node indices it holds.
    sites: Vec<Option<(CallPathId, TimedPath)>>,
    /// What timed calls read, chosen on first use.
    timer: Option<Timer>,
}

/// The clock a timed call reads. The epoch's own when it is a hardware
/// counter; when the epoch fell back to the OS monotonic clock (whose ticks
/// are nanoseconds), the CPU's generic counter, the one the OS clock reads
/// on aarch64, with counts converted to nanoseconds when they are written.
#[derive(Clone, Copy)]
enum Timer {
    Epoch,
    Counter { frequency: u64 },
}

/// The aarch64 generic counter and its frequency.
#[cfg(target_arch = "aarch64")]
#[allow(clippy::inline_always, unsafe_code)]
#[inline(always)]
fn counter() -> (u64, u64) {
    let (count, frequency): (u64, u64);
    // SAFETY: both registers are readable at EL0 on Linux.
    unsafe {
        std::arch::asm!("mrs {}, cntvct_el0", out(reg) count, options(nomem, nostack, preserves_flags));
        std::arch::asm!("mrs {}, cntfrq_el0", out(reg) frequency, options(nomem, nostack, preserves_flags));
    }
    (count, frequency)
}

struct TimedNode {
    call_path: CallPathId,
    reentry: bool,
    count: u32,
    total: u64,
}

/// Where a timed call's site resolved to for one parent path.
#[derive(Clone, Copy)]
pub(crate) struct TimedPath {
    pub(crate) path: CallPathId,
    pub(crate) reentry: bool,
    /// The aggregate the call adds to.
    pub(crate) node: u32,
}

impl TelemetryState {
    /// Whether a call this level observes only times it, with no span,
    /// capture or policy anywhere: medium, with no policy ever published.
    #[inline]
    pub(crate) fn times_only(&self) -> bool {
        self.auto_level == AutoTelemetryLevel::Medium && !self.policies.any_published()
    }

    /// Whether this level observes no call at all: low, with no policy ever
    /// published.
    #[inline]
    pub(crate) fn observes_none(&self) -> bool {
        self.auto_level == AutoTelemetryLevel::Low && !self.policies.any_published()
    }

    #[inline]
    pub(crate) fn set_active_call_path(&mut self, path: CallPathId) {
        self.thread.active_call_path = path;
    }

    /// A timed call's reading of its timer.
    #[inline]
    pub(crate) fn timer_now(&mut self) -> u64 {
        match self.timer() {
            Timer::Epoch => self.clock.read().get(),
            #[cfg(target_arch = "aarch64")]
            Timer::Counter { .. } => counter().0,
            #[cfg(not(target_arch = "aarch64"))]
            Timer::Counter { .. } => unreachable!("chosen on aarch64 only"),
        }
    }

    #[inline]
    fn timer(&mut self) -> Timer {
        if let Some(timer) = self.timed.timer {
            return timer;
        }
        let monotonic = matches!(self.clock.metadata().source, btel_clock::Source::Monotonic);
        #[cfg(target_arch = "aarch64")]
        let timer = if monotonic {
            Timer::Counter {
                frequency: counter().1,
            }
        } else {
            Timer::Epoch
        };
        #[cfg(not(target_arch = "aarch64"))]
        let timer = {
            let _ = monotonic;
            Timer::Epoch
        };
        self.timed.timer = Some(timer);
        timer
    }

    /// `elapsed` timer units in epoch ticks.
    fn epoch_ticks(&mut self, elapsed: u64) -> u64 {
        to_epoch(self.timer(), elapsed)
    }

    /// Where call site `site` goes under the active path, when it resolved
    /// under that path before.
    #[inline]
    pub(crate) fn cached_site(&self, site: usize) -> Option<TimedPath> {
        match self.timed.sites.get(site) {
            Some(Some((parent, path))) if *parent == self.thread.active_call_path => Some(*path),
            _ => None,
        }
    }

    pub(crate) fn cache_site(&mut self, site: usize, path: TimedPath) {
        if self.timed.sites.len() <= site {
            self.timed.sites.resize(site + 1, None);
        }
        self.timed.sites[site] = Some((self.thread.active_call_path, path));
    }

    /// The path a timed call from `visible_caller` at `caller_pc` to
    /// `callee` takes under the active path, as `enter_timing` resolves it
    /// for an observed caller, and its aggregate.
    pub(crate) fn resolve_timed(
        &mut self,
        visible_caller: Option<HeapPtr>,
        caller_pc: u32,
        callee: HeapPtr,
        register: impl FnOnce(Option<HeapPtr>, HeapPtr) -> (Option<FunctionId>, FunctionId),
    ) -> TimedPath {
        let parent = self.thread.active_call_path;
        let reentry = visible_caller == Some(callee)
            && self
                .call_path_keys
                .get(&parent)
                .is_some_and(|key| key.callee == callee);
        let path = if reentry {
            parent
        } else {
            self.resolve_call_path(
                CallPathKey {
                    parent,
                    visible_caller,
                    caller_pc,
                    callee,
                    edge: CallPathEdge::Synchronous,
                },
                register,
            )
        };
        let next = u32::try_from(self.timed.nodes.len()).expect("fewer than 2^32 call paths");
        let node = *self.timed.by_node.entry((path, reentry)).or_insert(next);
        if node == next {
            self.timed.nodes.push(TimedNode {
                call_path: path,
                reentry,
                count: 0,
                total: 0,
            });
        }
        TimedPath {
            path,
            reentry,
            node,
        }
    }

    /// A timed call on `node` that entered at timer reading `entered`
    /// returned.
    #[inline]
    pub(crate) fn add_timed(&mut self, node: u32, entered: u64) {
        let elapsed = self.timer_now().wrapping_sub(entered);
        let node = &mut self.timed.nodes[node as usize];
        node.count += 1;
        node.total = node.total.wrapping_add(elapsed);
        if node.count == u32::MAX {
            self.flush_timed();
        }
    }

    /// The receipt a timed call an error leaves hands the unwinder: the one
    /// `enter_timing` would have made.
    pub(crate) fn timed_receipt(
        &mut self,
        entered: u64,
        saved_call_path: CallPathId,
        reentry: bool,
    ) -> FrameTelemetry {
        let elapsed = self.timer_now().wrapping_sub(entered);
        let elapsed = self.epoch_ticks(elapsed);
        let entered_at = ClockInstant::from_ticks(self.clock.read().get().saturating_sub(elapsed));
        FrameTelemetry {
            entered_at,
            saved_parent_id: self.thread.active_id,
            span_id: None,
            await_duration: btel_types::AwaitDuration::ZERO,
            saved_call_path,
            flags: if reentry { REENTRY } else { 0 },
            output_request: false,
            error_request: false,
        }
    }

    /// Write every counted aggregate to the recorder and reset the counts.
    pub(crate) fn flush_timed(&mut self) {
        let timer = self.timed.timer.unwrap_or(Timer::Epoch);
        let records: Vec<TimingRecord> = self
            .timed
            .nodes
            .iter_mut()
            .filter(|node| node.count > 0)
            .map(|node| {
                let ticks = to_epoch(timer, node.total);
                let record = TimingRecord::FunctionTimingAggregate {
                    call_path: node.call_path,
                    total_duration: ClockDuration::from_ticks(ticks),
                    count: node.count,
                    reentry: node.reentry,
                };
                node.count = 0;
                node.total = 0;
                record
            })
            .collect();
        for record in records {
            self.write_timing(record);
        }
    }
}

/// `elapsed` units of `timer` in epoch ticks: nanoseconds for a counter
/// standing in for the OS monotonic clock.
fn to_epoch(timer: Timer, elapsed: u64) -> u64 {
    match timer {
        Timer::Epoch => elapsed,
        Timer::Counter { frequency } => {
            u64::try_from(u128::from(elapsed) * 1_000_000_000 / u128::from(frequency.max(1)))
                .unwrap_or(u64::MAX)
        }
    }
}
