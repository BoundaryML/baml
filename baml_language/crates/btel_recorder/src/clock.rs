use btel_clock::{CalibrationOutcome, EpochMetadata, FallbackReason, Source, TimingStatus};

use crate::proto;

fn nanos(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_nanos()).expect("clock uncertainty exceeds wire range")
}

pub(crate) fn definition(m: &EpochMetadata) -> proto::ClockEpochDefinition {
    let unix_bytes = m.utc.unix_nanos.get().to_le_bytes();
    proto::ClockEpochDefinition {
        epoch_id: m.epoch.get(),
        domain_id: m.domain.get(),
        source: match m.source {
            Source::Monotonic => proto::ClockSource::OsMonotonic,
            Source::PerformanceCounter => proto::ClockSource::WindowsQpc,
            Source::Tsc => proto::ClockSource::X86Tsc,
            Source::SystemCounter => proto::ClockSource::ArmSystemCounter,
        } as i32,
        reference_tick: m.reference_tick.get(),
        reference_monotonic_ns: m.reference_time.get(),
        multiplier: m.multiplier,
        shift: m.shift,
        origin_uncertainty_ns: nanos(m.origin_uncertainty),
        rate_error_ppb: m.rate_error.get(),
        calibration: Some(proto::CalibrationQuality {
            status: match m.calibration.status {
                CalibrationOutcome::NotRequired => proto::CalibrationStatus::NotRequired,
                CalibrationOutcome::Converged => proto::CalibrationStatus::Converged,
                CalibrationOutcome::DeadlineReached => proto::CalibrationStatus::DeadlineReached,
            } as i32,
            samples: m.calibration.samples,
            elapsed_ns: m.calibration.elapsed_ns,
            mean_residual_ns: m.calibration.mean_residual_ns,
            mean_error_ns: m.calibration.mean_error_ns,
        }),
        fallback: match m.fallback {
            None => proto::FallbackReason::None,
            Some(FallbackReason::Requested) => proto::FallbackReason::Requested,
            Some(FallbackReason::Unsupported) => proto::FallbackReason::Unsupported,
            Some(FallbackReason::Calibration) => proto::FallbackReason::Calibration,
            Some(FallbackReason::ScaleValidation) => proto::FallbackReason::ScaleValidation,
            Some(FallbackReason::Discontinuity) => proto::FallbackReason::Discontinuity,
        } as i32,
        utc: Some(proto::UtcAnchor {
            ticks: m.utc.ticks.get(),
            unix_nanos: Some(proto::UnixNanos {
                low: u64::from_le_bytes(unix_bytes[..8].try_into().unwrap()),
                high: i64::from_le_bytes(unix_bytes[8..].try_into().unwrap()),
            }),
            uncertainty_ns: nanos(m.utc.uncertainty),
        }),
    }
}

/// Latest observation. Final only once the epoch itself reports every attached
/// thread finished; one thread's completion is not proof the epoch settled.
pub(crate) fn state(epoch: &btel_clock::ClockEpoch) -> proto::ClockEpochState {
    let settled = epoch.settled_status();
    proto::ClockEpochState {
        epoch_id: epoch.metadata().epoch.get(),
        status: match settled.unwrap_or_else(|| epoch.status()) {
            TimingStatus::Valid => proto::TimingStatus::Valid,
            TimingStatus::Restored => proto::TimingStatus::Restored,
            TimingStatus::Discontinuity => proto::TimingStatus::Discontinuity,
            TimingStatus::Uncertain => proto::TimingStatus::Uncertain,
            TimingStatus::ModeChanged => proto::TimingStatus::ModeChanged,
        } as i32,
        r#final: settled.is_some(),
    }
}
