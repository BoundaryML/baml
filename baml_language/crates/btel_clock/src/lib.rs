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
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use btel_types::{ClockDuration, ClockInstant};
use quanta::{CalibrationMetadata, CalibrationQuality, CalibrationStatus, Clock, ClockSource};
use web_time::{SystemTime, UNIX_EPOCH};

mod calibration;
use calibration::{Calibrated, Probe, probe};
pub use quanta::ClockSource as Source;

// Budget: up to 100 ppm accepted scale uncertainty contributes 10 us between
// 100 ms checks. A 250 us residual margin leaves headroom within the 1 ms target.
// These are engineering tolerances, not statistical confidence guarantees.
const CHECK_INTERVAL: Duration = Duration::from_millis(100);
const ACCURACY_TARGET: u64 = 1_000_000;
const DISCONTINUITY_MARGIN: u64 = 250_000;
const MAX_SAMPLE_UNCERTAINTY: u64 = 50_000;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
fn next_id() -> NonZeroU64 {
    let raw = NEXT_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("clock identity space exhausted");
    NonZeroU64::new(raw).expect("clock identities start at one")
}

/// Process-local identity of a source and fixed tick scale. Thresholds are
/// valid only in it; exported IDs must be scoped by their telemetry session.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ClockDomainId(NonZeroU64);

/// Process-local identity of one immutable origin/mapping, scoped to a run.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ClockEpochId(NonZeroU64);

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

/// A duration threshold resolved once, outside invocation instrumentation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ClockThreshold {
    domain: ClockDomainId,
    ticks: ClockDuration,
}
impl ClockThreshold {
    #[inline(always)]
    pub fn reached(self, elapsed: ClockDuration, domain: ClockDomainId) -> bool {
        self.domain == domain && elapsed >= self.ticks
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockMode {
    Auto,
    Monotonic,
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
    pub multiplier: u64,
    pub shift: u32,
    pub origin_uncertainty: Duration,
    pub rate_error: RateErrorPpb,
    pub calibration: CalibrationQuality,
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
    Invalid(TimingStatus),
}

/// One run's retained clock. `metadata` and `clock` never change. Validity is
/// separate and can only become invalid, including for already-produced records
/// from this run. Completed runs are not invalidated by later restore/reset.
#[derive(Debug)]
pub struct ClockEpoch {
    clock: Clock,
    reference: Clock,
    resolution: u64,
    metadata: EpochMetadata,
    status: AtomicU8,
    active_threads: AtomicUsize,
    has_finished: AtomicBool,
    last_checked_tick: AtomicU64,
    interval_ticks: u64,
    validation: Mutex<()>,
}

impl ClockEpoch {
    fn new(calibrated: &Calibrated) -> Arc<Self> {
        let clock = calibrated.clock.clone();
        let reference = Clock::monotonic();
        let origin = probe(&clock, &reference, calibrated.resolution);
        let utc_before = reference.reference_nanos();
        let utc_tick = ClockInstant::from_ticks(clock.raw());
        let utc = SystemTime::now().duration_since(UNIX_EPOCH).map_or_else(
            |error| -i128::try_from(error.duration().as_nanos()).unwrap_or(i128::MAX),
            |duration| i128::try_from(duration.as_nanos()).unwrap_or(i128::MAX),
        );
        let utc_after = reference.reference_nanos();
        let conversion = calibrated.clock.calibration_metadata();
        Arc::new(Self {
            clock,
            reference,
            resolution: calibrated.resolution,
            metadata: EpochMetadata {
                epoch: ClockEpochId(next_id()),
                domain: calibrated.domain,
                source: conversion.source,
                reference_tick: origin.ticks,
                reference_time: origin.reference,
                multiplier: conversion.multiplier,
                shift: conversion.shift,
                origin_uncertainty: Duration::from_nanos(origin.uncertainty),
                rate_error: calibrated.rate_error,
                calibration: calibrated.quality,
                fallback: calibrated.fallback,
                utc: UtcAnchor {
                    ticks: utc_tick,
                    unix_nanos: UnixNanos(utc),
                    uncertainty: Duration::from_nanos(
                        utc_after
                            .saturating_sub(utc_before)
                            .saturating_add(calibrated.resolution.saturating_mul(2)),
                    ),
                },
            },
            status: AtomicU8::new(if origin.uncertainty > MAX_SAMPLE_UNCERTAINTY {
                TimingStatus::Uncertain as u8
            } else {
                TimingStatus::Valid as u8
            }),
            active_threads: AtomicUsize::new(0),
            has_finished: AtomicBool::new(false),
            last_checked_tick: AtomicU64::new(origin.ticks.get()),
            interval_ticks: to_ticks(CHECK_INTERVAL, conversion.multiplier, conversion.shift).get(),
            validation: Mutex::new(()),
        })
    }

    /// Raw reading only. No conversion, locks, reference counting, validation,
    /// calibration, or epoch selection. The backend branch is intentional.
    #[inline(always)]
    pub fn read(&self) -> ClockInstant {
        ClockInstant::from_ticks(self.clock.raw())
    }

    pub fn metadata(&self) -> &EpochMetadata {
        &self.metadata
    }
    pub fn domain(&self) -> ClockDomainId {
        self.metadata.domain
    }

    /// Attach this run's context to a raw record from this epoch. Do not attach
    /// another run's raw values: frames deliberately omit per-timestamp IDs.
    pub fn timestamp(&self, ticks: ClockInstant) -> Timestamp {
        Timestamp {
            epoch: self.metadata.epoch,
            ticks,
        }
    }

    /// Checked cold-path interpretation, including backward-delta clamping.
    pub fn duration(&self, start: Timestamp, end: Timestamp) -> Result<Duration, TimingError> {
        if start.epoch != self.metadata.epoch || end.epoch != self.metadata.epoch {
            return Err(TimingError::WrongEpoch);
        }
        let status = self.status();
        if status != TimingStatus::Valid {
            return Err(TimingError::Invalid(status));
        }
        Ok(Duration::from_nanos(scale(
            start.ticks.elapsed_until(end.ticks),
            self.metadata.multiplier,
            self.metadata.shift,
        )))
    }

    pub fn threshold(&self, duration: Duration) -> ClockThreshold {
        ClockThreshold {
            domain: self.domain(),
            ticks: to_ticks(duration, self.metadata.multiplier, self.metadata.shift),
        }
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

    /// Thread lifecycle bookkeeping, outside function instrumentation.
    pub fn attach_thread(&self) {
        self.active_threads.fetch_add(1, Ordering::Relaxed);
    }
    pub fn finish_thread(&self) {
        if self.active_threads.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.has_finished.store(true, Ordering::Release);
        }
    }

    fn invalidate(&self, reason: TimingStatus) {
        let _ = self.status.compare_exchange(
            TimingStatus::Valid as u8,
            reason as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    fn check(&self, force: bool) -> TimingStatus {
        if self.status() != TimingStatus::Valid {
            return self.status();
        }
        let raw = self.read();
        let previous = self.last_checked_tick.load(Ordering::Relaxed);
        if !force && raw.get() >= previous && raw.get() - previous < self.interval_ticks {
            return TimingStatus::Valid;
        }
        let Ok(_guard) = self.validation.try_lock() else {
            return self.status();
        };
        let sample = probe(&self.clock, &self.reference, self.resolution);
        let status = self.assess(sample, ClockInstant::from_ticks(previous));
        if status != TimingStatus::Valid {
            self.invalidate(status);
        }
        self.last_checked_tick
            .store(sample.ticks.get(), Ordering::Relaxed);
        self.status()
    }

    fn assess(&self, sample: Probe, previous: ClockInstant) -> TimingStatus {
        if sample.uncertainty > MAX_SAMPLE_UNCERTAINTY {
            return TimingStatus::Uncertain;
        }
        if sample.reference < self.metadata.reference_time {
            return TimingStatus::Discontinuity;
        }
        // Small core-to-core skews are within the accuracy target. Individual
        // backward intervals clamp to zero; do not abandon the fast clock for
        // one sub-budget boundary observation.
        if sample.ticks < previous
            && scale(
                sample.ticks.elapsed_until(previous),
                self.metadata.multiplier,
                self.metadata.shift,
            ) > DISCONTINUITY_MARGIN.saturating_add(sample.uncertainty)
        {
            return TimingStatus::Discontinuity;
        }
        let elapsed = sample.reference.0 - self.metadata.reference_time.0;
        let predicted = scale(
            self.metadata.reference_tick.elapsed_until(sample.ticks),
            self.metadata.multiplier,
            self.metadata.shift,
        );
        let discrepancy = predicted.abs_diff(elapsed);
        let sampling = sample.uncertainty.saturating_add(
            u64::try_from(self.metadata.origin_uncertainty.as_nanos()).unwrap_or(u64::MAX),
        );
        let scale_error =
            narrow(u128::from(elapsed) * u128::from(self.metadata.rate_error.0) / 1_000_000_000);
        let tolerance = DISCONTINUITY_MARGIN
            .saturating_add(sampling)
            .saturating_add(scale_error);
        // A growing scale-error allowance must not silently accept a known
        // >1 ms origin error. Both tests subtract the measurement uncertainty.
        if discrepancy > tolerance || discrepancy > ACCURACY_TARGET.saturating_add(sampling) {
            TimingStatus::Discontinuity
        } else {
            TimingStatus::Valid
        }
    }
}

struct RuntimeState {
    mode: ClockMode,
    calibrated: Calibrated,
    epochs: Vec<Weak<ClockEpoch>>,
}

/// Engine-owned lifecycle state. Only run creation, reset, and detected faults
/// acquire this lock. No global Quanta calibration is used.
pub struct ClockRuntime {
    state: Mutex<RuntimeState>,
}
impl ClockRuntime {
    pub fn new(mode: ClockMode) -> Self {
        Self {
            state: Mutex::new(RuntimeState {
                mode,
                calibrated: Calibrated::new(mode),
                epochs: Vec::new(),
            }),
        }
    }

    /// Each independent root gets a fresh origin and UTC anchor. Spawned work
    /// must instead clone its parent's epoch.
    pub fn start_run(&self) -> Arc<ClockEpoch> {
        let mut state = self.state.lock().expect("clock lifecycle poisoned");
        Self::start_run_locked(&mut state)
    }

    fn start_run_locked(state: &mut RuntimeState) -> Arc<ClockEpoch> {
        state.epochs.retain(|epoch| epoch.strong_count() != 0);
        let epoch = ClockEpoch::new(&state.calibrated);
        state.epochs.push(Arc::downgrade(&epoch));
        epoch
    }

    /// Generic after-restore hook; call before resuming execution. All active
    /// old runs become invalid even if the counter looks unchanged. Fresh
    /// Quanta calibration and scale validation bypass its process-global cache.
    /// Completed records keep their original mapping and validity.
    pub fn reset_after_restore(&self) -> Arc<ClockEpoch> {
        let mut state = self.state.lock().expect("clock lifecycle poisoned");
        Self::invalidate_active(&state, TimingStatus::Restored);
        state.calibrated = Calibrated::new(state.mode);
        Self::start_run_locked(&mut state)
    }

    /// Deployment-level OS-clock option, also available at construction.
    /// Changing mode invalidates active timings rather than mixing domains.
    pub fn set_mode(&self, mode: ClockMode) -> Arc<ClockEpoch> {
        let mut state = self.state.lock().expect("clock lifecycle poisoned");
        Self::invalidate_active(&state, TimingStatus::ModeChanged);
        state.mode = mode;
        state.calibrated = Calibrated::new(mode);
        Self::start_run_locked(&mut state)
    }

    /// Rate-limited execution-boundary check. Force a sample on root completion.
    /// No background task is created; a busy uninterrupted VM region is checked
    /// at its next boundary. Transient jumps between samples can escape detection.
    pub fn validate(&self, epoch: &ClockEpoch, force: bool) {
        // Already-invalid runs keep their old clock; do not keep taking the
        // lifecycle lock at every subsequent engine handoff.
        if epoch.status() != TimingStatus::Valid {
            return;
        }
        let status = epoch.check(force);
        if !matches!(
            status,
            TimingStatus::Discontinuity | TimingStatus::Uncertain
        ) {
            return;
        }
        let mut state = self.state.lock().expect("clock lifecycle poisoned");
        if state.calibrated.domain == epoch.domain() {
            Self::invalidate_active(&state, status);
            // Do not stall a running request for hardware recalibration. Use OS
            // monotonic until an explicit reset/reselection retries hardware.
            state.calibrated = Calibrated::monotonic(FallbackReason::Discontinuity, None);
        }
    }

    fn invalidate_active(state: &RuntimeState, reason: TimingStatus) {
        for epoch in state.epochs.iter().filter_map(Weak::upgrade) {
            if epoch.active_threads.load(Ordering::Acquire) != 0
                || !epoch.has_finished.load(Ordering::Acquire)
            {
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
fn to_ticks(duration: Duration, multiplier: u64, shift: u32) -> ClockDuration {
    let numerator = duration.as_nanos().saturating_mul(1_u128 << shift);
    ClockDuration::from_ticks(narrow(numerator.div_ceil(u128::from(multiplier))))
}

#[cfg(test)]
mod tests;
