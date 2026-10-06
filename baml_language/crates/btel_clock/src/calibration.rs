//! Bounded reference sampling and incremental scale estimation. Never waits.
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use btel_settings::clock::{MAX_RATE_ERROR_PPB, MAX_SAMPLE_UNCERTAINTY_NS, RATE_ERROR_FLOOR_PPB};
use btel_types::ClockInstant;
use quanta::{CalibrationQuality, CalibrationStatus, Clock, RawClock, RawScale};

use super::{
    ClockDomainId, ClockMapping, ClockMode, FallbackReason, MonotonicNanos, Precision,
    RateErrorPpb, narrow, next_id,
};

pub(super) struct SourceClock {
    pub clock: RawClock,
    pub domain: ClockDomainId,
    pub resolution: u64,
    pub mapping: Arc<OnceLock<ClockMapping>>,
    pub fallback: Option<FallbackReason>,
}
impl SourceClock {
    pub(super) fn new(mode: ClockMode) -> Self {
        let clock = if mode == ClockMode::Monotonic || cfg!(miri) {
            RawClock::monotonic()
        } else {
            RawClock::new()
        };
        let mapping = OnceLock::new();
        if let Some(scale) = clock.reported_scale() {
            let _ = mapping.set(ClockMapping::reported(scale));
        }
        Self {
            clock,
            domain: ClockDomainId(next_id()),
            resolution: narrow(Clock::monotonic().reference_resolution().as_nanos()),
            mapping: Arc::new(mapping),
            fallback: (mode == ClockMode::Monotonic).then_some(FallbackReason::Requested),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Probe {
    pub ticks: ClockInstant,
    pub reference: MonotonicNanos,
    pub uncertainty: u64,
}

/// At most three bracketed samples; preemption makes a sample noisy, not invalid.
pub(super) fn probe(clock: &RawClock, reference: &Clock, resolution: u64) -> Probe {
    let mut best = Probe {
        ticks: ClockInstant::from_ticks(0),
        reference: MonotonicNanos(0),
        uncertainty: u64::MAX,
    };
    for _ in 0..btel_settings::clock::PROBE_ATTEMPTS {
        let before = reference.reference_nanos();
        let ticks = ClockInstant::from_ticks(clock.raw());
        let after = reference.reference_nanos();
        let width = after.saturating_sub(before);
        let sample = Probe {
            ticks,
            reference: MonotonicNanos(before.saturating_add(width / 2)),
            uncertainty: width / 2 + resolution,
        };
        if sample.uncertainty < best.uncertainty {
            best = sample;
        }
        if width <= resolution.max(btel_settings::clock::EARLY_PROBE_WIDTH_NS) {
            break;
        }
    }
    best
}

pub(super) fn estimate(start: Probe, end: Probe, precision: Precision) -> Option<ClockMapping> {
    let elapsed = end.reference.0.checked_sub(start.reference.0)?;
    let ticks = end.ticks.get().checked_sub(start.ticks.get())?;
    let sampling = start.uncertainty.saturating_add(end.uncertainty);
    // No meaningful rate can be inferred if the observation is below precision.
    if elapsed <= sampling || ticks == 0 {
        return None;
    }
    let ratio = RawScale::ratio(elapsed, ticks)?;
    let rate_error = narrow(u128::from(sampling) * 1_000_000_000 / u128::from(elapsed))
        .max(RATE_ERROR_FLOOR_PPB);
    Some(ClockMapping {
        multiplier: ratio.multiplier,
        shift: ratio.shift,
        precision,
        rate_error: RateErrorPpb(rate_error),
        calibration: CalibrationQuality {
            status: if precision == Precision::Calibrated {
                CalibrationStatus::Converged
            } else {
                CalibrationStatus::DeadlineReached
            },
            samples: 2,
            elapsed_ns: elapsed,
            mean_residual_ns: 0.0,
            // Sampling error is diagnostic; integer bounds drive acceptance.
            #[expect(clippy::cast_precision_loss)]
            mean_error_ns: sampling as f64,
        },
    })
}

#[derive(Debug)]
pub(super) struct Sampling {
    pub start: Probe,
    pub last: Probe,
    pub candidate: Option<(Probe, ClockMapping)>,
    pub elapsed: Option<(u64, u64)>,
    pub fault: bool,
}
impl Sampling {
    pub(super) fn new(origin: Probe) -> Self {
        Self {
            start: origin,
            last: origin,
            candidate: None,
            elapsed: None,
            fault: false,
        }
    }
    pub(super) fn observe(&mut self, sample: Probe) -> Option<ClockMapping> {
        self.last = sample;
        if self.fault {
            return None;
        }
        if sample.uncertainty > MAX_SAMPLE_UNCERTAINTY_NS {
            return None;
        }
        if self.start.uncertainty > MAX_SAMPLE_UNCERTAINTY_NS {
            self.start = sample;
            self.candidate = None;
            return None;
        }
        if let Some((previous, candidate)) = self.candidate {
            if sample.reference.0.saturating_sub(previous.reference.0)
                < narrow(btel_settings::clock::SCALE_CHECK_INTERVAL_DURATION.as_nanos())
            {
                return None;
            }
            let holdout = estimate(previous, sample, Precision::Calibrated)?;
            let predicted = super::scale(
                previous.ticks.elapsed_until(sample.ticks),
                candidate.multiplier,
                candidate.shift,
            );
            let elapsed = sample.reference.0 - previous.reference.0;
            let error = narrow(
                u128::from(predicted.abs_diff(elapsed)) * 1_000_000_000 / u128::from(elapsed),
            );
            if error
                > MAX_RATE_ERROR_PPB
                    .saturating_add(candidate.rate_error.0)
                    .saturating_add(holdout.rate_error.0)
            {
                if holdout.rate_error.0 <= MAX_RATE_ERROR_PPB
                    && predicted.abs_diff(elapsed)
                        > btel_settings::clock::DISCONTINUITY_MARGIN_NS
                            .saturating_add(previous.uncertainty)
                            .saturating_add(sample.uncertainty)
                {
                    self.fault = true;
                    return None;
                }
                // Start another independent observation rather than trusting a
                // rate spanning a possible discontinuity.
                self.start = sample;
                self.candidate = None;
                return None;
            }
            let mut mapping = estimate(self.start, sample, Precision::Calibrated)?;
            if mapping.rate_error.0 > MAX_RATE_ERROR_PPB {
                return None;
            }
            mapping.calibration.samples = 3;
            return Some(mapping);
        }
        if sample.reference.0.saturating_sub(self.start.reference.0)
            < narrow(SAMPLE_INTERVAL.as_nanos())
        {
            return None;
        }
        let mapping = estimate(self.start, sample, Precision::Calibrated)?;
        if mapping.rate_error.0 <= MAX_RATE_ERROR_PPB {
            self.candidate = Some((sample, mapping));
        }
        None
    }
}

pub(super) const SAMPLE_INTERVAL: Duration =
    btel_settings::clock::CALIBRATION_SAMPLE_INTERVAL_DURATION;
