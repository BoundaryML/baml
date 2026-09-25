use calibration::{acceptable_calibration, scale_quality};

use super::*;

fn fixture(multiplier: u64, shift: u32) -> Arc<ClockEpoch> {
    let mut clock = ClockEpoch::new(&Calibrated::monotonic(FallbackReason::Requested, None));
    let epoch = Arc::get_mut(&mut clock).unwrap();
    epoch.metadata.reference_tick = ClockInstant::from_ticks(1_000);
    epoch.metadata.reference_time = MonotonicNanos(10_000);
    epoch.metadata.multiplier = multiplier;
    epoch.metadata.shift = shift;
    epoch.metadata.origin_uncertainty = Duration::from_nanos(100);
    epoch.metadata.rate_error = RateErrorPpb(10_000);
    clock
}

#[test]
fn conversion_is_typed_clamped_and_rejects_cross_epoch_intervals() {
    // Fractional fixed-point conversion, preserving tick resolution.
    let epoch = fixture(125, 2);
    let start = epoch.timestamp(ClockInstant::from_ticks(1_000));
    let end = epoch.timestamp(ClockInstant::from_ticks(1_032));
    assert_eq!(epoch.duration(start, end), Ok(Duration::from_nanos(1_000)));
    assert_eq!(epoch.duration(end, start), Ok(Duration::ZERO));
    let other = fixture(1, 0);
    assert_eq!(other.duration(start, end), Err(TimingError::WrongEpoch));
    let threshold = epoch.threshold(Duration::from_nanos(1_001));
    assert!(!threshold.reached(ClockDuration::from_ticks(32), epoch.domain()));
    assert!(threshold.reached(ClockDuration::from_ticks(33), epoch.domain()));
    assert!(!threshold.reached(ClockDuration::from_ticks(33), other.domain()));
    assert_eq!(to_ticks(Duration::MAX, 1, 63).get(), u64::MAX);
    assert_eq!(
        scale(ClockDuration::from_ticks(u64::MAX), u64::MAX, 0),
        u64::MAX
    );
    assert_eq!(std::mem::size_of::<ClockInstant>(), 8);
    assert_eq!(std::mem::size_of::<MonotonicNanos>(), 8);
    assert_eq!(std::mem::size_of::<ClockDomainId>(), 8);
}

#[test]
fn detector_accounts_for_sampling_scale_error_and_the_accuracy_target() {
    let epoch = fixture(1, 0);
    let origin = epoch.metadata.reference_tick;
    let sample = |elapsed: u64, offset: i64, uncertainty| Probe {
        ticks: ClockInstant::from_ticks((1_000 + elapsed).checked_add_signed(offset).unwrap()),
        reference: MonotonicNanos(10_000 + elapsed),
        uncertainty,
    };
    assert_eq!(
        epoch.assess(sample(100_000_000, 0, 100), origin),
        TimingStatus::Valid
    );
    assert_eq!(
        epoch.assess(sample(100_000_000, 5_000_000, 100), origin),
        TimingStatus::Discontinuity
    );
    assert_eq!(
        epoch.assess(sample(100_000_000, -5_000_000, 100), origin),
        TimingStatus::Discontinuity
    );
    assert_eq!(
        epoch.assess(sample(100_000_000, 0, 100_000), origin),
        TimingStatus::Uncertain
    );
    // Ten ppm accumulated over 30 s is expected, not a fixed-threshold fault.
    assert_eq!(
        epoch.assess(sample(30_000_000_000, 300_000, 100), origin),
        TimingStatus::Valid
    );
    // But an uncertainty budget growing with time must not hide >1 ms drift.
    assert_eq!(
        epoch.assess(sample(300_000_000_000, 2_000_000, 100), origin),
        TimingStatus::Discontinuity
    );
    assert_eq!(
        epoch.assess(
            sample(100_000_000, 0, 100),
            ClockInstant::from_ticks(200_000_000)
        ),
        TimingStatus::Discontinuity
    );
    assert_eq!(
        epoch.assess(
            sample(100_000_000, 0, 100),
            ClockInstant::from_ticks(100_101_000)
        ),
        TimingStatus::Valid
    );
    // A transient jump that reverses between samples is inherently undetectable.
    assert_eq!(
        epoch.assess(sample(100_000_000, 0, 100), origin),
        TimingStatus::Valid
    );
}

#[test]
fn restore_invalidates_active_intervals_but_keeps_completed_mappings() {
    let runtime = ClockRuntime::new(ClockMode::Monotonic);
    let completed = runtime.start_run();
    completed.attach_thread();
    let completed_start = completed.timestamp(completed.read());
    completed.finish_thread();
    let active = runtime.start_run();
    active.attach_thread();
    let before = active.timestamp(active.read());
    let metadata_before = active.metadata().clone();
    let fresh = runtime.reset_after_restore();
    assert_eq!(active.status(), TimingStatus::Restored);
    assert_eq!(
        active.duration(before, active.timestamp(active.read())),
        Err(TimingError::Invalid(TimingStatus::Restored))
    );
    assert_eq!(
        active.metadata().reference_tick,
        metadata_before.reference_tick
    );
    assert_eq!(active.metadata().multiplier, metadata_before.multiplier);
    assert_ne!(active.domain(), fresh.domain());
    assert_ne!(active.metadata().epoch, fresh.metadata().epoch);
    assert!(completed.duration(completed_start, completed_start).is_ok());
    assert_eq!(fresh.status(), TimingStatus::Valid);
    active.finish_thread();
}

#[test]
fn fault_switches_future_runs_to_os_without_reinterpreting_active_records() {
    let runtime = ClockRuntime::new(ClockMode::Monotonic);
    let epoch = runtime.start_run();
    epoch.attach_thread();
    // A retained checkpoint ahead of the raw clock models a backward jump.
    epoch.last_checked_tick.store(u64::MAX, Ordering::Relaxed);
    let old_domain = epoch.domain();
    runtime.validate(&epoch, false);
    assert_eq!(epoch.status(), TimingStatus::Discontinuity);
    let replacement = runtime.start_run();
    assert_ne!(old_domain, replacement.domain());
    assert!(matches!(
        replacement.metadata().source,
        Source::Monotonic | Source::PerformanceCounter
    ));
    assert_eq!(
        replacement.metadata().fallback,
        Some(FallbackReason::Discontinuity)
    );
    assert_eq!(epoch.domain(), old_domain);
    epoch.finish_thread();
}

#[test]
fn calibration_deadline_and_inaccurate_scale_are_rejected() {
    let failed = Clock::new_uncached_with_options(quanta::CalibrationOptions {
        timeout: Duration::ZERO,
        ..quanta::CalibrationOptions::default()
    });
    if matches!(
        failed.calibration_metadata().source,
        Source::Tsc | Source::SystemCounter
    ) {
        let selected = Calibrated::from_clock(failed);
        assert_eq!(selected.fallback, Some(FallbackReason::Calibration));
        assert!(matches!(
            selected.clock.calibration_metadata().source,
            Source::Monotonic | Source::PerformanceCounter
        ));
        assert_eq!(selected.quality.status, CalibrationStatus::DeadlineReached);
    }
    let mut metadata = CalibrationMetadata {
        source: Source::Tsc,
        reference_ticks: 0,
        reference_time_ns: 0,
        multiplier: 1,
        shift: 0,
        quality: CalibrationQuality {
            status: CalibrationStatus::Converged,
            samples: 501,
            elapsed_ns: 1_000_000,
            mean_residual_ns: 1.0,
            mean_error_ns: 0.1,
        },
    };
    assert!(acceptable_calibration(metadata));
    metadata.quality.status = CalibrationStatus::DeadlineReached;
    assert!(!acceptable_calibration(metadata));
    let fallback = Calibrated::monotonic(FallbackReason::Calibration, Some(metadata.quality));
    assert!(matches!(
        fallback.clock.calibration_metadata().source,
        Source::Monotonic | Source::PerformanceCounter
    ));
    assert_eq!(fallback.quality.status, CalibrationStatus::DeadlineReached);
    metadata.quality.status = CalibrationStatus::Converged;
    let start = Probe {
        ticks: ClockInstant::from_ticks(0),
        reference: MonotonicNanos(0),
        uncertainty: 100,
    };
    let mut end = Probe {
        ticks: ClockInstant::from_ticks(50_000_000),
        reference: MonotonicNanos(50_000_000),
        uncertainty: 100,
    };
    assert!(scale_quality(metadata, start, end).is_some());
    end.ticks = ClockInstant::from_ticks(55_000_000);
    assert!(scale_quality(metadata, start, end).is_none());
    metadata.quality.mean_error_ns = f64::NAN;
    assert!(!acceptable_calibration(metadata));
}

#[test]
fn actual_backend_preserves_raw_and_reference_duration_agreement() {
    let runtime = ClockRuntime::new(ClockMode::Auto);
    let epoch = runtime.start_run();
    let os = Clock::monotonic();
    let start = probe(&epoch.clock, &os, epoch.resolution);
    std::thread::sleep(Duration::from_millis(25));
    let end = probe(&epoch.clock, &os, epoch.resolution);
    let elapsed = epoch
        .duration(epoch.timestamp(start.ticks), epoch.timestamp(end.ticks))
        .unwrap();
    let reference = end.reference.0 - start.reference.0;
    assert!(
        elapsed.as_nanos().abs_diff(u128::from(reference))
            < u128::from(ACCURACY_TARGET_NS + start.uncertainty + end.uncertainty)
    );
}
