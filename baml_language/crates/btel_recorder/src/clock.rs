use btel_clock::{
    CalibrationOutcome, EpochMetadata, FallbackReason, Precision, Source, TimingStatus,
};

use crate::proto;

fn nanos(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_nanos()).expect("clock uncertainty exceeds wire range")
}

pub(crate) fn definition(epoch: &btel_clock::ClockEpoch) -> Option<proto::ClockEpochDefinition> {
    let m = epoch.metadata();
    let mapping = epoch.mapping()?;
    let unix_bytes = m.utc.unix_nanos.get().to_le_bytes();
    Some(proto::ClockEpochDefinition {
        epoch_id: m.epoch.get(),
        domain_id: m.domain.get(),
        source: match m.source {
            Source::Monotonic => proto::ClockSource::OsMonotonic,
            Source::PerformanceCounter => proto::ClockSource::WindowsQpc,
            Source::Tsc => proto::ClockSource::X86Tsc,
            Source::SystemCounter => proto::ClockSource::ArmSystemCounter,
            Source::MachAbsolute => proto::ClockSource::MachAbsolute,
        } as i32,
        reference_tick: m.reference_tick.get(),
        reference_monotonic_ns: m.reference_time.get(),
        multiplier: mapping.multiplier,
        shift: mapping.shift,
        origin_uncertainty_ns: nanos(m.origin_uncertainty),
        rate_error_ppb: mapping.rate_error.get(),
        calibration: Some(proto::CalibrationQuality {
            status: match mapping.calibration.status {
                CalibrationOutcome::NotRequired => proto::CalibrationStatus::NotRequired,
                CalibrationOutcome::Converged => proto::CalibrationStatus::Converged,
                CalibrationOutcome::DeadlineReached => proto::CalibrationStatus::DeadlineReached,
            } as i32,
            samples: mapping.calibration.samples,
            elapsed_ns: mapping.calibration.elapsed_ns,
            mean_residual_ns: mapping.calibration.mean_residual_ns,
            mean_error_ns: mapping.calibration.mean_error_ns,
        }),
        fallback: match m.fallback {
            None => proto::FallbackReason::None,
            Some(FallbackReason::Requested) => proto::FallbackReason::Requested,
            Some(FallbackReason::Unsupported) => proto::FallbackReason::Unsupported,
            Some(FallbackReason::Calibration) => proto::FallbackReason::Calibration,
            Some(FallbackReason::ScaleValidation) => proto::FallbackReason::ScaleValidation,
            Some(FallbackReason::Discontinuity) => proto::FallbackReason::Discontinuity,
        } as i32,
        precision: match mapping.precision {
            Precision::Reported => proto::ClockPrecision::Reported,
            Precision::Calibrated => proto::ClockPrecision::Calibrated,
            Precision::Estimated => proto::ClockPrecision::Estimated,
        } as i32,
        utc: Some(proto::UtcAnchor {
            ticks: m.utc.ticks.get(),
            unix_nanos: Some(proto::UnixNanos {
                low: u64::from_le_bytes(unix_bytes[..8].try_into().unwrap()),
                high: i64::from_le_bytes(unix_bytes[8..].try_into().unwrap()),
            }),
            uncertainty_ns: nanos(m.utc.uncertainty),
        }),
    })
}

pub(crate) fn anchor(m: &EpochMetadata) -> proto::ClockEpochAnchor {
    let bytes = m.utc.unix_nanos.get().to_le_bytes();
    proto::ClockEpochAnchor {
        epoch_id: m.epoch.get(),
        domain_id: m.domain.get(),
        source: match m.source {
            Source::Monotonic => proto::ClockSource::OsMonotonic,
            Source::PerformanceCounter => proto::ClockSource::WindowsQpc,
            Source::Tsc => proto::ClockSource::X86Tsc,
            Source::SystemCounter => proto::ClockSource::ArmSystemCounter,
            Source::MachAbsolute => proto::ClockSource::MachAbsolute,
        } as i32,
        reference_tick: m.reference_tick.get(),
        reference_monotonic_ns: m.reference_time.get(),
        origin_uncertainty_ns: nanos(m.origin_uncertainty),
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
                low: u64::from_le_bytes(bytes[..8].try_into().unwrap()),
                high: i64::from_le_bytes(bytes[8..].try_into().unwrap()),
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
        elapsed_reference_ns: epoch.elapsed_reference().map(|e| e.0),
        elapsed_uncertainty_ns: epoch.elapsed_reference().map(|e| e.1),
    }
}
