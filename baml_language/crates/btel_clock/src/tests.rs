use super::*;

fn fixture(multiplier: u64, shift: u32) -> Arc<ClockEpoch> {
    let runtime = ClockRuntime::new(ClockMode::Monotonic);
    let mut clock = runtime.start_run();
    drop(runtime);
    let epoch = Arc::get_mut(&mut clock).unwrap();
    epoch.metadata.reference_tick = ClockInstant::from_ticks(1_000);
    epoch.metadata.reference_time = MonotonicNanos(10_000);
    epoch.metadata.origin_uncertainty = Duration::from_nanos(100);
    epoch.mapping = OnceLock::from(ClockMapping {
        multiplier,
        shift,
        ..ClockMapping::reported(RawScale { multiplier, shift })
    });
    clock
}
fn sample(elapsed: u64, offset: i64, uncertainty: u64) -> Probe {
    Probe {
        ticks: ClockInstant::from_ticks((1_000 + elapsed).checked_add_signed(offset).unwrap()),
        reference: MonotonicNanos(10_000 + elapsed),
        uncertainty,
    }
}

#[test]
fn conversion_is_typed_clamped_and_rejects_cross_epoch_intervals() {
    let epoch = fixture(125, 2);
    let start = epoch.timestamp(ClockInstant::from_ticks(1_000));
    let end = epoch.timestamp(ClockInstant::from_ticks(1_032));
    assert_eq!(epoch.duration(start, end), Ok(Duration::from_nanos(1_000)));
    assert_eq!(epoch.duration(end, start), Ok(Duration::ZERO));
    let other = fixture(1, 0);
    assert_eq!(other.duration(start, end), Err(TimingError::WrongEpoch));
    let threshold = epoch.threshold(Duration::from_nanos(1_001));
    assert!(!threshold.reached(ClockDuration::from_ticks(32), &epoch));
    assert!(threshold.reached(ClockDuration::from_ticks(33), &epoch));
    assert!(!threshold.reached(ClockDuration::from_ticks(33), &other));
    assert_eq!(
        scale(ClockDuration::from_ticks(u64::MAX), u64::MAX, 0),
        u64::MAX
    );
    assert_eq!(std::mem::size_of::<ClockInstant>(), 8);
}

#[test]
fn noisy_reference_is_not_a_fault_but_wrong_rate_and_jumps_are() {
    let epoch = fixture(1, 0);
    let origin = epoch.metadata.reference_tick;
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
        epoch.assess(sample(100_000_000, 50_000_000, 100_000), origin),
        TimingStatus::Valid
    );
    assert_eq!(
        epoch.assess(sample(300_000_000_000, 2_000_000, 100), origin),
        TimingStatus::Valid
    );
    assert_eq!(
        epoch.assess(sample(300_000_000_000, 60_000_000, 100), origin),
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
}

#[test]
fn duration_policy_retains_eligible_values_after_an_observed_clock_fault() {
    let epoch = fixture(1, 0);
    let policy = epoch.threshold(Duration::from_secs(1));
    assert!(!policy.reached(ClockDuration::from_ticks(1), &epoch));

    epoch.invalidate_if_fault(
        sample(100_000_000, 5_000_000, 100),
        epoch.metadata.reference_tick,
    );
    assert_eq!(epoch.status(), TimingStatus::Discontinuity);
    assert!(epoch.mapping().is_some(), "the conversion stays immutable");
    assert!(policy.reached(ClockDuration::ZERO, &epoch));

    let fresh = fixture(1, 0);
    assert!(!policy.reached(ClockDuration::from_ticks(1), &fresh));
}

#[test]
fn restore_checks_continuity_and_preserves_completed_records() {
    let runtime = ClockRuntime::new(ClockMode::Monotonic);
    let completed = runtime.start_run();
    completed.attach_thread();
    completed.finish_thread();
    let active = runtime.start_run();
    active.attach_thread();
    let mapping = *active.mapping().unwrap();
    let fresh = runtime.reset_after_restore();
    assert_eq!(active.status(), TimingStatus::Valid);
    assert_eq!(active.mapping().unwrap().multiplier, mapping.multiplier);
    assert_ne!(active.domain(), fresh.domain());
    assert_eq!(completed.status(), TimingStatus::Valid);
    active.last_checked_tick.store(u64::MAX, Ordering::Relaxed);
    runtime.reset_after_restore();
    assert_eq!(active.status(), TimingStatus::Discontinuity);
    assert_eq!(completed.status(), TimingStatus::Valid);
    active.finish_thread();
}

#[test]
fn children_delay_finality_and_a_mode_change_does_not_rewrite_mapping() {
    let runtime = ClockRuntime::new(ClockMode::Monotonic);
    let run = runtime.start_run();
    run.attach_thread();
    run.attach_thread();
    run.finish_thread();
    assert_eq!(run.settled_status(), None);
    runtime.set_mode(ClockMode::Auto);
    assert_eq!(run.status(), TimingStatus::ModeChanged);
    run.finish_thread();
    assert_eq!(run.settled_status(), Some(TimingStatus::ModeChanged));
    let mapping = run.mapping().unwrap().multiplier;
    runtime.set_mode(ClockMode::Monotonic);
    assert_eq!(run.mapping().unwrap().multiplier, mapping);
    assert!(run.elapsed_reference().is_some());
}

#[test]
fn finality_survives_mode_changes_racing_the_last_thread() {
    let runtime = Arc::new(ClockRuntime::new(ClockMode::Monotonic));
    for _ in 0..100 {
        let epoch = runtime.start_run();
        epoch.attach_thread();
        std::thread::scope(|threads| {
            threads.spawn(|| {
                runtime.set_mode(ClockMode::Monotonic);
            });
            threads.spawn(|| epoch.finish_thread());
            let observed = loop {
                if let Some(status) = epoch.settled_status() {
                    break status;
                }
                std::hint::spin_loop();
            };
            assert_eq!(epoch.status(), observed);
        });
    }
}

#[test]
fn detected_fault_retries_the_selected_source_in_a_new_generation() {
    let runtime = ClockRuntime::new(ClockMode::Auto);
    let run = runtime.start_run();
    run.attach_thread();
    let source = run.metadata().source;
    run.last_checked_tick.store(u64::MAX, Ordering::Relaxed);
    runtime.validate(&run, true);
    assert_eq!(run.status(), TimingStatus::Discontinuity);
    let replacement = runtime.start_run();
    assert_ne!(run.domain(), replacement.domain());
    assert_eq!(source, replacement.metadata().source);
    run.finish_thread();
}

#[test]
fn missing_scale_reads_immediately_and_short_runs_estimate_without_caching() {
    let runtime = ClockRuntime::without_reported_scale(ClockMode::Auto);
    let epoch = runtime.start_run();
    epoch.attach_thread();
    let start = epoch.timestamp(epoch.read());
    assert_eq!(epoch.duration(start, start), Err(TimingError::Pending));
    assert!(
        epoch
            .threshold(Duration::from_secs(1))
            .reached(ClockDuration::ZERO, &epoch)
    );
    std::thread::sleep(Duration::from_millis(2));
    epoch.finish_thread();
    let mapping = epoch.mapping().expect("above reference precision");
    assert_eq!(mapping.precision, Precision::Estimated);
    assert_eq!(epoch.settled_status(), Some(TimingStatus::Valid));
    assert!(epoch.duration(start, epoch.timestamp(epoch.read())).is_ok());
    let later = runtime.start_run();
    assert!(
        later.mapping().is_none(),
        "short-run estimate must never seed another run"
    );
}

#[test]
fn incremental_calibration_has_independent_holdout_and_no_spin() {
    let origin = sample(0, 0, 100);
    let mut sampling = Sampling::new(origin);
    assert!(sampling.observe(sample(1_000_000, 0, 100)).is_none());
    assert!(sampling.observe(sample(20_000_000, 0, 100)).is_none());
    assert!(sampling.observe(sample(30_000_000, 0, 100)).is_none());
    let mapping = sampling.observe(sample(70_000_000, 0, 100)).unwrap();
    assert_eq!(mapping.precision, Precision::Calibrated);
    assert_eq!(mapping.calibration.samples, 3);
    assert_eq!(
        scale(
            ClockDuration::from_ticks(10_000),
            mapping.multiplier,
            mapping.shift
        ),
        10_000
    );
    let noisy = sample(100_000_000, 0, MAX_SAMPLE_UNCERTAINTY_NS + 1);
    assert!(sampling.observe(noisy).is_none());
}

#[test]
fn observations_below_precision_do_not_invent_scale() {
    assert!(
        calibration::estimate(
            sample(0, 0, 1_000),
            sample(100, 0, 1_000),
            Precision::Estimated
        )
        .is_none()
    );
    assert!(
        calibration::estimate(sample(0, 0, 1), sample(100, -100, 1), Precision::Estimated)
            .is_none()
    );
}

#[test]
fn restore_cancels_pending_calibration_and_stale_results_cannot_seed_new_source() {
    let runtime = ClockRuntime::without_reported_scale(ClockMode::Auto);
    let old = runtime.start_run();
    old.attach_thread();
    let new = runtime.reset_after_restore();
    assert_eq!(old.status(), TimingStatus::Valid);
    assert_ne!(old.domain(), new.domain());
    let mapping = ClockMapping::reported(RawScale {
        multiplier: 999,
        shift: 0,
    });
    old.shared_mapping.set(mapping).unwrap();
    assert_ne!(new.mapping().map(|m| m.multiplier), Some(999));
    old.finish_thread();
    assert_ne!(new.mapping().map(|m| m.multiplier), Some(999));
}

#[test]
fn concurrent_engines_do_not_share_calibration_or_domains() {
    let first = ClockRuntime::without_reported_scale(ClockMode::Auto);
    let second = ClockRuntime::without_reported_scale(ClockMode::Auto);
    let a = first.start_run();
    let b = second.start_run();
    assert_ne!(a.domain(), b.domain());
    a.shared_mapping
        .set(ClockMapping::reported(RawScale {
            multiplier: 1,
            shift: 0,
        }))
        .unwrap();
    assert!(b.shared_mapping.get().is_none());
}

#[test]
fn actual_backend_agrees_with_reference_and_settled_validation_is_noop() {
    let runtime = ClockRuntime::new(ClockMode::Auto);
    let epoch = runtime.start_run();
    epoch.attach_thread();
    let start = probe(&epoch.clock, &epoch.reference, epoch.resolution);
    std::thread::sleep(Duration::from_millis(25));
    let end = probe(&epoch.clock, &epoch.reference, epoch.resolution);
    epoch.finish_thread();
    let elapsed = epoch
        .duration(epoch.timestamp(start.ticks), epoch.timestamp(end.ticks))
        .unwrap();
    assert!(
        elapsed
            .as_nanos()
            .abs_diff(u128::from(end.reference.0 - start.reference.0))
            < u128::from(ACCURACY_TARGET_NS + start.uncertainty + end.uncertainty)
    );
    epoch.last_checked_tick.store(u64::MAX, Ordering::Relaxed);
    runtime.validate(&epoch, true);
    assert_eq!(epoch.status(), TimingStatus::Valid);
}

#[test]
fn duration_policies_are_portable_across_pending_estimated_and_reported_runs() {
    let first = fixture(10, 0);
    let policy = first.threshold(Duration::from_nanos(1_000));
    assert!(policy.reached(ClockDuration::from_ticks(100), &first));
    let second = fixture(1, 0);
    assert!(!policy.reached(ClockDuration::from_ticks(100), &second));
    assert!(policy.reached(ClockDuration::from_ticks(1_000), &second));
    let pending = ClockRuntime::without_reported_scale(ClockMode::Auto).start_run();
    assert!(policy.reached(ClockDuration::ZERO, &pending));
}

#[test]
fn a_clean_holdout_reveals_a_pending_counter_rate_change() {
    let mut sampling = Sampling::new(sample(0, 0, 100));
    assert!(sampling.observe(sample(20_000_000, 0, 100)).is_none());
    assert!(
        sampling
            .observe(sample(70_000_000, 50_000_000, 100))
            .is_none()
    );
    assert!(sampling.fault);
    assert!(
        sampling
            .observe(sample(140_000_000, 50_000_000, 100))
            .is_none()
    );
}
