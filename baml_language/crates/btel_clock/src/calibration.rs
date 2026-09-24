use super::{
    CalibrationMetadata, CalibrationQuality, CalibrationStatus, Clock, ClockDomainId, ClockInstant,
    ClockMode, ClockSource, Duration, FallbackReason, MAX_SAMPLE_UNCERTAINTY, MonotonicNanos,
    RateErrorPpb, narrow, next_id, scale,
};

const SCALE_CHECK_INTERVAL: Duration = Duration::from_millis(50);
const MAX_RATE_ERROR_PPB: u64 = 100_000; // 100 ppm; at most 10 us per 100 ms check interval.
const RATE_ERROR_FLOOR_PPB: u64 = 10_000; // Statistical calibration is not a hard error bound.

pub(super) struct Calibrated {
    pub clock: Clock,
    pub domain: ClockDomainId,
    pub quality: CalibrationQuality,
    pub rate_error: RateErrorPpb,
    pub resolution: u64,
    pub fallback: Option<FallbackReason>,
}

#[derive(Clone, Copy)]
pub(super) struct Probe {
    pub ticks: ClockInstant,
    pub reference: MonotonicNanos,
    pub uncertainty: u64,
}

/// Take the narrowest of three OS brackets; a descheduled sample contributes
/// its measured uncertainty rather than looking like a counter discontinuity.
pub(super) fn probe(clock: &Clock, reference: &Clock, resolution: u64) -> Probe {
    let mut best = Probe {
        ticks: ClockInstant::default(),
        reference: MonotonicNanos(0),
        uncertainty: u64::MAX,
    };
    for _ in 0..3 {
        let before = reference.reference_nanos();
        let ticks = ClockInstant::from_ticks(clock.raw());
        let after = reference.reference_nanos();
        let width = after.checked_sub(before).unwrap_or(u64::MAX);
        let sample = Probe {
            ticks,
            reference: MonotonicNanos(before.saturating_add(width / 2)),
            uncertainty: width.div_ceil(2).saturating_add(resolution),
        };
        if sample.uncertainty < best.uncertainty {
            best = sample;
        }
        // The first tight bracket is sufficient; repeated queries primarily
        // help when a sample was descheduled. No need to optimize for perfect phase.
        if width <= resolution.max(1_000) {
            break;
        }
    }
    best
}

impl Calibrated {
    pub(super) fn new(mode: ClockMode) -> Self {
        if mode == ClockMode::Monotonic || cfg!(miri) {
            return Self::monotonic(FallbackReason::Requested, None);
        }
        let clock = Clock::new_uncached_with_options(quanta::CalibrationOptions {
            minimum_duration: Duration::from_millis(20),
            maximum_error: Duration::from_nanos(100),
            timeout: Duration::from_millis(200),
        });
        Self::from_clock(clock)
    }

    pub(super) fn from_clock(clock: Clock) -> Self {
        let metadata = clock.calibration_metadata();
        if matches!(
            metadata.source,
            ClockSource::Monotonic | ClockSource::PerformanceCounter
        ) {
            return Self::monotonic(FallbackReason::Unsupported, None);
        }
        if !acceptable_calibration(metadata) {
            return Self::monotonic(FallbackReason::Calibration, Some(metadata.quality));
        }
        let reference = Clock::monotonic();
        let resolution = narrow(clock.reference_resolution().as_nanos());
        let start = probe(&clock, &reference, resolution);
        // Holdout validation only. The scale still comes entirely from Quanta's
        // unchanged calibration algorithm; we do not fit a second ratio.
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::sleep(SCALE_CHECK_INTERVAL);
        let end = probe(&clock, &reference, resolution);
        let Some(rate_error) = scale_quality(metadata, start, end) else {
            return Self::monotonic(FallbackReason::ScaleValidation, Some(metadata.quality));
        };
        Self {
            clock,
            domain: ClockDomainId(next_id()),
            quality: metadata.quality,
            rate_error,
            resolution,
            fallback: None,
        }
    }

    pub(super) fn monotonic(reason: FallbackReason, attempted: Option<CalibrationQuality>) -> Self {
        let clock = Clock::monotonic();
        let resolution = narrow(clock.reference_resolution().as_nanos());
        let quality = attempted.unwrap_or_else(|| clock.calibration_metadata().quality);
        Self {
            clock,
            domain: ClockDomainId(next_id()),
            quality,
            rate_error: RateErrorPpb(0),
            resolution,
            fallback: Some(reason),
        }
    }
}

pub(super) fn acceptable_calibration(metadata: CalibrationMetadata) -> bool {
    let quality = metadata.quality;
    quality.status == CalibrationStatus::Converged
        && quality.samples > 500
        && quality.elapsed_ns != 0
        && metadata.multiplier != 0
        && metadata.shift < 64
        && quality.mean_residual_ns.is_finite()
        && quality.mean_error_ns.is_finite()
        && quality.mean_residual_ns.abs() + quality.mean_error_ns.abs() < 100.0
}

// Calibrated slope uncertainty is estimated from both Quanta's residual over
// its measurement interval and an independent 50 ms OS bracket. It is not a
// proof of future scale stability. Poor scheduling/precision chooses OS fallback.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "finite nonnegative calibration residual is bounded before rounding to nanoseconds"
)]
pub(super) fn scale_quality(
    metadata: CalibrationMetadata,
    start: Probe,
    end: Probe,
) -> Option<RateErrorPpb> {
    let elapsed = end.reference.0.checked_sub(start.reference.0)?;
    if elapsed < u64::try_from(SCALE_CHECK_INTERVAL.as_nanos()).ok()?
        || start.uncertainty > MAX_SAMPLE_UNCERTAINTY
        || end.uncertainty > MAX_SAMPLE_UNCERTAINTY
        || end.ticks < start.ticks
    {
        return None;
    }
    let predicted = scale(
        start.ticks.elapsed_until(end.ticks),
        metadata.multiplier,
        metadata.shift,
    );
    let measured_error = predicted
        .abs_diff(elapsed)
        .saturating_add(start.uncertainty)
        .saturating_add(end.uncertainty);
    let measured_ppb =
        narrow((u128::from(measured_error) * 1_000_000_000).div_ceil(u128::from(elapsed)));
    let residual = (metadata.quality.mean_residual_ns.abs() + metadata.quality.mean_error_ns.abs())
        .ceil() as u64;
    let calibration_ppb = narrow(
        (u128::from(residual) * 1_000_000_000).div_ceil(u128::from(metadata.quality.elapsed_ns)),
    );
    let estimate = measured_ppb.max(calibration_ppb).max(RATE_ERROR_FLOOR_PPB);
    (estimate <= MAX_RATE_ERROR_PPB).then_some(RateErrorPpb(estimate))
}
