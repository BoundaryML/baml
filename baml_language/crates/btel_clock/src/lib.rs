//! Retained raw clocks for telemetry. Reads do not convert or validate time.
//!
//! The normal-operation target is 1 ms, not a host-migration guarantee. Each
//! run owns an immutable epoch; children share it. Boundary validation can
//! invalidate an epoch, never rewrite its conversion. An invalid active run
//! stays invalid until it finishes; future runs use a newly selected domain.
#![allow(clippy::inline_always, reason = "measured raw producer clock reads")]

use std::{
    num::NonZeroU64,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use btel_types::{ClockDuration, ClockInstant};
use quanta::{CalibrationQuality, CalibrationStatus, Clock, ClockSource, RawClock, RawScale};
use web_time::{SystemTime, UNIX_EPOCH};

mod calibration;
pub use btel_settings::clock::ClockMode;
use btel_settings::clock::{
    ACCURACY_TARGET_NS, DISCONTINUITY_MARGIN_NS, MAX_DRIFT_PPB, MAX_SAMPLE_UNCERTAINTY_NS,
    VALIDATION_INTERVAL_DURATION,
};
use calibration::{Probe, Sampling, SourceClock, probe};
pub use quanta::{CalibrationStatus as CalibrationOutcome, ClockSource as Source};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
fn next_id() -> NonZeroU64 {
    let raw = NEXT_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("clock identity space exhausted");
    NonZeroU64::new(raw).expect("clock identities start at one")
}

/// Process-local identity of a retained raw source generation.
/// Exported IDs must be scoped by their telemetry session.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ClockDomainId(NonZeroU64);

/// Process-local identity of one immutable origin/mapping, scoped to a run.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ClockEpochId(NonZeroU64);

impl ClockDomainId {
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}
impl ClockEpochId {
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// OS monotonic nanoseconds, never UTC or raw counter ticks.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct MonotonicNanos(u64);
impl MonotonicNanos {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Wall-clock nanoseconds since Unix epoch. Not used in duration arithmetic.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnixNanos(i128);
impl UnixNanos {
    pub const fn get(self) -> i128 {
        self.0
    }
}

/// Estimated relative scale uncertainty, in parts per billion.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RateErrorPpb(u64);
impl RateErrorPpb {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// An epoch-tagged timestamp for cold-path interpretation. Frames keep just
/// `ClockInstant`; the surrounding thread/run context supplies their epoch.
#[derive(Clone, Copy, Debug)]
pub struct Timestamp {
    epoch: ClockEpochId,
    ticks: ClockInstant,
}

/// A duration policy, portable across runs and source re-selection. Conversion
/// is consulted only by explicitly configured duration policies, never raw reads.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ClockThreshold {
    duration: Duration,
}
impl ClockThreshold {
    #[inline(always)]
    pub fn reached(self, elapsed: ClockDuration, epoch: &ClockEpoch) -> bool {
        epoch.mapping().is_none_or(|m| {
            ((u128::from(elapsed.get()) * u128::from(m.multiplier)) >> m.shift)
                >= self.duration.as_nanos()
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FallbackReason {
    Requested,
    Unsupported,
    Calibration,
    ScaleValidation,
    Discontinuity,
}

/// Separate wall-clock anchor with its own bracketing uncertainty.
#[derive(Clone, Copy, Debug)]
pub struct UtcAnchor {
    pub ticks: ClockInstant,
    pub unix_nanos: UnixNanos,
    pub uncertainty: Duration,
}

/// Immutable mapping attached to a run, not repeated in every frame/record.
#[derive(Clone, Debug)]
pub struct EpochMetadata {
    pub epoch: ClockEpochId,
    pub domain: ClockDomainId,
    pub source: ClockSource,
    pub reference_tick: ClockInstant,
    pub reference_time: MonotonicNanos,
    pub origin_uncertainty: Duration,
    pub fallback: Option<FallbackReason>,
    pub utc: UtcAnchor,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimingStatus {
    Valid,
    Restored,
    Discontinuity,
    Uncertain,
    ModeChanged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimingError {
    WrongEpoch,
    Pending,
    Invalid(TimingStatus),
}

/// Precision describes interpretation, separately from counter validity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Precision {
    Reported,
    Calibrated,
    Estimated,
}

/// Published exactly once. Estimated short-run rates are never cached for reuse.
#[derive(Clone, Copy, Debug)]
pub struct ClockMapping {
    pub multiplier: u64,
    pub shift: u32,
    pub rate_error: RateErrorPpb,
    pub calibration: CalibrationQuality,
    pub precision: Precision,
}
impl ClockMapping {
    fn reported(scale: RawScale) -> Self {
        Self {
            multiplier: scale.multiplier,
            shift: scale.shift,
            rate_error: RateErrorPpb(btel_settings::clock::RATE_ERROR_FLOOR_PPB),
            calibration: CalibrationQuality {
                status: CalibrationStatus::NotRequired,
                samples: 0,
                elapsed_ns: 0,
                mean_residual_ns: 0.0,
                mean_error_ns: 0.0,
            },
            precision: Precision::Reported,
        }
    }
}

/// A run retains its raw source and immutable anchor. Mapping is published once
/// on a cold path; raw producer reads never consult it or calibration state.
#[derive(Debug)]
pub struct ClockEpoch {
    clock: RawClock,
    reference: Clock,
    resolution: u64,
    metadata: EpochMetadata,
    mapping: OnceLock<ClockMapping>,
    shared_mapping: Arc<OnceLock<ClockMapping>>,
    runtime: Weak<Mutex<RuntimeState>>,
    status: AtomicU8,
    active_threads: AtomicUsize,
    has_finished: AtomicBool,
    last_checked_tick: AtomicU64,
    sampling: Mutex<Sampling>,
}

impl ClockEpoch {
    fn new(source: &SourceClock, runtime: Weak<Mutex<RuntimeState>>) -> Arc<Self> {
        let clock = source.clock.clone();
        let reference = Clock::monotonic();
        let origin = probe(&clock, &reference, source.resolution);
        let utc_before = reference.reference_nanos();
        let utc_tick = ClockInstant::from_ticks(clock.raw());
        let utc = SystemTime::now().duration_since(UNIX_EPOCH).map_or_else(
            |error| -i128::try_from(error.duration().as_nanos()).unwrap_or(i128::MAX),
            |duration| i128::try_from(duration.as_nanos()).unwrap_or(i128::MAX),
        );
        let utc_after = reference.reference_nanos();
        let mapping = OnceLock::new();
        if let Some(value) = source.mapping.get() {
            let _ = mapping.set(*value);
        }
        Arc::new(Self {
            metadata: EpochMetadata {
                epoch: ClockEpochId(next_id()),
                domain: source.domain,
                source: clock.source(),
                reference_tick: origin.ticks,
                reference_time: origin.reference,
                origin_uncertainty: Duration::from_nanos(origin.uncertainty),
                fallback: source.fallback,
                utc: UtcAnchor {
                    ticks: utc_tick,
                    unix_nanos: UnixNanos(utc),
                    uncertainty: Duration::from_nanos(
                        utc_after
                            .saturating_sub(utc_before)
                            .saturating_add(source.resolution.saturating_mul(2)),
                    ),
                },
            },
            clock,
            reference,
            resolution: source.resolution,
            mapping,
            shared_mapping: Arc::clone(&source.mapping),
            runtime,
            status: AtomicU8::new(TimingStatus::Valid as u8),
            active_threads: AtomicUsize::new(0),
            has_finished: AtomicBool::new(false),
            last_checked_tick: AtomicU64::new(origin.ticks.get()),
            sampling: Mutex::new(Sampling::new(origin)),
        })
    }

    /// No conversion, locks, reference counting, validation or calibration.
    #[inline(always)]
    pub fn read(&self) -> ClockInstant {
        ClockInstant::from_ticks(self.clock.raw())
    }
    pub fn metadata(&self) -> &EpochMetadata {
        &self.metadata
    }
    pub fn mapping(&self) -> Option<&ClockMapping> {
        self.mapping.get()
    }
    pub fn domain(&self) -> ClockDomainId {
        self.metadata.domain
    }
    pub fn timestamp(&self, ticks: ClockInstant) -> Timestamp {
        Timestamp {
            epoch: self.metadata.epoch,
            ticks,
        }
    }

    pub fn duration(&self, start: Timestamp, end: Timestamp) -> Result<Duration, TimingError> {
        if start.epoch != self.metadata.epoch || end.epoch != self.metadata.epoch {
            return Err(TimingError::WrongEpoch);
        }
        let status = self.status();
        if status != TimingStatus::Valid {
            return Err(TimingError::Invalid(status));
        }
        let mapping = self.mapping().ok_or(TimingError::Pending)?;
        Ok(Duration::from_nanos(scale(
            start.ticks.elapsed_until(end.ticks),
            mapping.multiplier,
            mapping.shift,
        )))
    }

    /// Pending duration policies conservatively capture eligible calls now;
    /// values cannot be captured after calibration has finished.
    pub fn threshold(&self, duration: Duration) -> ClockThreshold {
        ClockThreshold { duration }
    }
    pub fn status(&self) -> TimingStatus {
        match self.status.load(Ordering::Acquire) {
            0 => TimingStatus::Valid,
            1 => TimingStatus::Restored,
            2 => TimingStatus::Discontinuity,
            3 => TimingStatus::Uncertain,
            _ => TimingStatus::ModeChanged,
        }
    }
    pub fn attach_thread(&self) {
        self.active_threads.fetch_add(1, Ordering::Relaxed);
    }
    pub fn finish_thread(&self) {
        if self.active_threads.fetch_sub(1, Ordering::AcqRel) == 1 {
            // No deadline wait: one bounded final reference sample, then publish
            // an estimate if it is above the observation's precision.
            let mut sampling = self
                .sampling
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            self.observe(&mut sampling, true);
            self.has_finished.store(true, Ordering::Release);
            drop(sampling);
            self.recover();
        }
    }
    pub fn settled_status(&self) -> Option<TimingStatus> {
        if !self.is_settled() {
            return None;
        }
        let _guard = self
            .sampling
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Some(self.status())
    }
    pub fn elapsed_reference(&self) -> Option<(u64, u64)> {
        if !self.is_settled() {
            return None;
        }
        self.sampling
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .elapsed
    }
    fn is_settled(&self) -> bool {
        self.active_threads.load(Ordering::Acquire) == 0
            && self.has_finished.load(Ordering::Acquire)
    }
    fn invalidate(&self, reason: TimingStatus) {
        let _ = self.status.compare_exchange(
            TimingStatus::Valid as u8,
            reason as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    /// Existing processor and execution boundaries drive this bounded work.
    /// No dedicated worker and no calibration on the producer timestamp path.
    pub fn maintain(&self, force: bool) {
        self.check(force);
        self.recover();
    }
    fn recover(&self) {
        if self.status() != TimingStatus::Discontinuity {
            return;
        }
        if let Some(runtime) = self.runtime.upgrade() {
            let mut state = runtime.lock().expect("clock lifecycle poisoned");
            if state.source.domain == self.domain() {
                ClockRuntime::invalidate_active(&state, TimingStatus::Discontinuity);
                // A new generation retries the fast source automatically.
                state.source = SourceClock::new(state.mode);
                // Re-measure after a fault rather than trusting the same bad
                // reported frequency again. Retain cheap raw counter reads.
                state.source.mapping = Arc::new(OnceLock::new());
                state.source.fallback = Some(FallbackReason::Discontinuity);
            }
        }
    }
    pub fn maintenance_interval(&self) -> Option<Duration> {
        if self.is_settled() || self.status() != TimingStatus::Valid {
            None
        } else if self.mapping().is_none() {
            Some(calibration::SAMPLE_INTERVAL)
        } else {
            Some(VALIDATION_INTERVAL_DURATION)
        }
    }
    fn check(&self, force: bool) -> TimingStatus {
        if self.status() != TimingStatus::Valid || self.is_settled() {
            return self.status();
        }
        let raw = self.read();
        let previous = self.last_checked_tick.load(Ordering::Relaxed);
        if !force && raw.get() >= previous {
            if let Some(mapping) = self.mapping() {
                if scale(
                    ClockDuration::from_ticks(raw.get() - previous),
                    mapping.multiplier,
                    mapping.shift,
                ) < narrow(VALIDATION_INTERVAL_DURATION.as_nanos())
                {
                    return self.status();
                }
            }
        }
        let Ok(mut sampling) = self.sampling.try_lock() else {
            return self.status();
        };
        if !force
            && self.mapping().is_none()
            && raw.get() >= previous
            && self
                .reference
                .reference_nanos()
                .saturating_sub(sampling.last.reference.0)
                < narrow(calibration::SAMPLE_INTERVAL.as_nanos())
        {
            return self.status();
        }
        if !self.is_settled() {
            self.observe(&mut sampling, false);
        }
        self.status()
    }
    fn observe(&self, sampling: &mut Sampling, final_sample: bool) {
        let sample = probe(&self.clock, &self.reference, self.resolution);
        let previous = ClockInstant::from_ticks(self.last_checked_tick.load(Ordering::Relaxed));
        if self.status() == TimingStatus::Valid {
            self.invalidate_if_fault(sample, previous);
        }
        if self.status() == TimingStatus::Valid && self.mapping().is_none() {
            if let Some(mapping) = self.shared_mapping.get() {
                let _ = self.mapping.set(*mapping);
            } else if let Some(mapping) = sampling.observe(sample) {
                let _ = self.shared_mapping.set(mapping);
                let _ = self
                    .mapping
                    .set(*self.shared_mapping.get().expect("published mapping"));
            } else if sampling.fault {
                self.invalidate(TimingStatus::Discontinuity);
            } else if final_sample {
                if let Some(mapping) =
                    calibration::estimate(sampling.start, sample, Precision::Estimated)
                {
                    let _ = self.mapping.set(mapping);
                }
            }
        }
        // A newly learned rate also checks the original anchor, including an
        // interval crossing a restore while scale was still unknown.
        if self.status() == TimingStatus::Valid && self.mapping().is_some() {
            self.invalidate_if_fault(sample, previous);
        }
        if final_sample {
            sampling.elapsed = Some((
                sample
                    .reference
                    .0
                    .saturating_sub(self.metadata.reference_time.0),
                sample
                    .uncertainty
                    .saturating_add(narrow(self.metadata.origin_uncertainty.as_nanos())),
            ));
        }
        sampling.last = sample;
        self.last_checked_tick
            .store(sample.ticks.get(), Ordering::Relaxed);
    }
    fn invalidate_if_fault(&self, sample: Probe, previous: ClockInstant) {
        let status = self.assess(sample, previous);
        if status != TimingStatus::Valid {
            self.invalidate(status);
        }
    }
    fn assess(&self, sample: Probe, previous: ClockInstant) -> TimingStatus {
        // A noisy reference sample says nothing about counter validity.
        if sample.uncertainty > MAX_SAMPLE_UNCERTAINTY_NS {
            return TimingStatus::Valid;
        }
        if sample.reference < self.metadata.reference_time {
            return TimingStatus::Discontinuity;
        }
        let Some(mapping) = self.mapping() else {
            return if sample.ticks < previous {
                TimingStatus::Discontinuity
            } else {
                TimingStatus::Valid
            };
        };
        if sample.ticks < previous
            && scale(
                sample.ticks.elapsed_until(previous),
                mapping.multiplier,
                mapping.shift,
            ) > DISCONTINUITY_MARGIN_NS.saturating_add(sample.uncertainty)
        {
            return TimingStatus::Discontinuity;
        }
        let elapsed = sample.reference.0 - self.metadata.reference_time.0;
        let predicted = scale(
            self.metadata.reference_tick.elapsed_until(sample.ticks),
            mapping.multiplier,
            mapping.shift,
        );
        let discrepancy = predicted.abs_diff(elapsed);
        let sampling = sample
            .uncertainty
            .saturating_add(narrow(self.metadata.origin_uncertainty.as_nanos()));
        let scale_error =
            narrow(u128::from(elapsed) * u128::from(mapping.rate_error.0) / 1_000_000_000);
        let drift = narrow(u128::from(elapsed) * u128::from(MAX_DRIFT_PPB) / 1_000_000_000);
        if discrepancy
            > DISCONTINUITY_MARGIN_NS
                .saturating_add(sampling)
                .saturating_add(scale_error)
                .saturating_add(drift)
            || discrepancy
                > ACCURACY_TARGET_NS
                    .saturating_add(sampling)
                    .saturating_add(drift)
        {
            TimingStatus::Discontinuity
        } else {
            TimingStatus::Valid
        }
    }
}

struct RuntimeState {
    mode: ClockMode,
    source: SourceClock,
    epochs: Vec<Weak<ClockEpoch>>,
    cursor: usize,
}
impl std::fmt::Debug for RuntimeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeState")
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

/// Engine-owned source selection. Pending work belongs to a source generation;
/// after restore/reselection it cannot publish calibration into the new one.
pub struct ClockRuntime {
    state: Arc<Mutex<RuntimeState>>,
}
impl ClockRuntime {
    pub fn new(mode: ClockMode) -> Self {
        Self {
            state: Arc::new(Mutex::new(RuntimeState {
                mode,
                source: SourceClock::new(mode),
                epochs: Vec::new(),
                cursor: 0,
            })),
        }
    }
    /// Bounded cold work for the existing processor, including runs whose
    /// producer chunks have not yet been published. No extra OS thread.
    pub fn maintenance(&self) -> Duration {
        let mut epochs: [Option<Arc<ClockEpoch>>; 16] = std::array::from_fn(|_| None);
        {
            let mut state = self.state.lock().expect("clock lifecycle poisoned");
            let len = state.epochs.len();
            for slot in epochs.iter_mut().take(len.min(16)) {
                state.cursor %= len;
                *slot = state.epochs[state.cursor].upgrade();
                state.cursor += 1;
            }
        }
        let mut interval = VALIDATION_INTERVAL_DURATION;
        for epoch in epochs.into_iter().flatten() {
            epoch.maintain(false);
            if let Some(next) = epoch.maintenance_interval() {
                interval = interval.min(next);
            }
        }
        interval
    }
    /// Test the missing-frequency path on hosts that normally report scale.
    #[cfg(any(test, feature = "test-support"))]
    pub fn without_reported_scale(mode: ClockMode) -> Self {
        let runtime = Self::new(mode);
        runtime.state.lock().unwrap().source.mapping = Arc::new(OnceLock::new());
        runtime
    }
    pub fn start_run(&self) -> Arc<ClockEpoch> {
        let mut state = self.state.lock().expect("clock lifecycle poisoned");
        self.start_run_locked(&mut state)
    }
    fn start_run_locked(&self, state: &mut RuntimeState) -> Arc<ClockEpoch> {
        state.epochs.retain(|epoch| epoch.strong_count() != 0);
        let epoch = ClockEpoch::new(&state.source, Arc::downgrade(&self.state));
        state.epochs.push(Arc::downgrade(&epoch));
        epoch
    }
    /// Migration alone is not a fault. Check retained mappings for continuity;
    /// replace source generation to discard pending or cached calibration.
    pub fn reset_after_restore(&self) -> Arc<ClockEpoch> {
        let mut state = self.state.lock().expect("clock lifecycle poisoned");
        for epoch in state.epochs.iter().filter_map(Weak::upgrade) {
            if epoch.is_settled() {
                continue;
            }
            if epoch.mapping().is_some() {
                epoch.check(true);
            } else {
                let mut sampling = epoch
                    .sampling
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if !epoch.is_settled() {
                    let sample = probe(&epoch.clock, &epoch.reference, epoch.resolution);
                    epoch.invalidate_if_fault(
                        sample,
                        ClockInstant::from_ticks(epoch.last_checked_tick.load(Ordering::Relaxed)),
                    );
                    // Discard pending pre-restore fitting. Preserve the immutable
                    // anchor; a new mapping must still agree with that anchor.
                    *sampling = Sampling::new(sample);
                    epoch
                        .last_checked_tick
                        .store(sample.ticks.get(), Ordering::Relaxed);
                }
            }
        }
        state.source = SourceClock::new(state.mode);
        self.start_run_locked(&mut state)
    }
    pub fn set_mode(&self, mode: ClockMode) -> Arc<ClockEpoch> {
        let mut state = self.state.lock().expect("clock lifecycle poisoned");
        Self::invalidate_active(&state, TimingStatus::ModeChanged);
        state.mode = mode;
        state.source = SourceClock::new(mode);
        self.start_run_locked(&mut state)
    }
    pub fn validate(&self, epoch: &ClockEpoch, force: bool) {
        epoch.maintain(force);
    }
    fn invalidate_active(state: &RuntimeState, reason: TimingStatus) {
        for epoch in state.epochs.iter().filter_map(Weak::upgrade) {
            let _guard = epoch
                .sampling
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !epoch.is_settled() {
                epoch.invalidate(reason);
            }
        }
    }
}

fn narrow(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
fn scale(ticks: ClockDuration, multiplier: u64, shift: u32) -> u64 {
    narrow((u128::from(ticks.get()) * u128::from(multiplier)) >> shift)
}
#[cfg(test)]
mod tests;
