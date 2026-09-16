//! The isolated BAML patch: fresh calibration and immutable diagnostics.
use std::time::Duration;

use crate::{detection, Calibration, Clock, ClockType, Counter, Monotonic};

/// Parameters for the existing upstream calibration loop; no alternate fitting
/// algorithm is introduced. Defaults retain the upstream convergence criteria.
#[derive(Clone, Copy, Debug)]
pub struct CalibrationOptions {
    /// Minimum observation interval before accepting convergence.
    pub minimum_duration: Duration,
    /// Maximum mean absolute residual plus standard error.
    pub maximum_error: Duration,
    /// Maximum wall time allowed for calibration.
    pub timeout: Duration,
}
impl Default for CalibrationOptions {
    fn default() -> Self {
        Self {
            minimum_duration: Duration::ZERO,
            maximum_error: Duration::from_nanos(crate::MAXIMUM_CAL_ERROR_NS),
            timeout: Duration::from_nanos(crate::MAXIMUM_CAL_TIME_NS),
        }
    }
}

/// Backend selected by a retained clock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockSource {
    /// OS monotonic clock; raw values are nanoseconds.
    Monotonic,
    /// Windows OS performance counter; raw values are QPC ticks.
    PerformanceCounter,
    /// x86 invariant time stamp counter.
    Tsc,
    /// AArch64 generic system counter.
    SystemCounter,
    /// Test-controlled nanoseconds.
    #[cfg(feature = "mock")]
    Mock,
}

/// Whether the upstream calibration convergence condition was met.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalibrationStatus {
    /// The OS backend supplies its scale without statistical calibration.
    NotRequired,
    /// The upstream sample count and residual criteria were satisfied.
    Converged,
    /// Calibration ended at the deadline, without satisfying those criteria.
    DeadlineReached,
}

/// Diagnostics, not a guarantee of future accuracy or migration safety.
#[derive(Clone, Copy, Debug)]
pub struct CalibrationQuality {
    /// Reason the calibration loop terminated.
    pub status: CalibrationStatus,
    /// Number of residual samples collected.
    pub samples: u64,
    /// Elapsed reference nanoseconds during calibration.
    pub elapsed_ns: u64,
    /// Signed mean of the scaled-source minus reference residuals, in ns.
    pub mean_residual_ns: f64,
    /// Standard error of that mean, in ns.
    pub mean_error_ns: f64,
}

impl CalibrationQuality {
    pub(crate) const NOT_REQUIRED: Self = Self {
        status: CalibrationStatus::NotRequired,
        samples: 0,
        elapsed_ns: 0,
        mean_residual_ns: 0.0,
        mean_error_ns: 0.0,
    };
}

/// Immutable conversion snapshot. Scale a nonnegative tick delta with
/// `(u128::from(delta) * multiplier) >> shift`, then add the reference time.
#[derive(Clone, Copy, Debug)]
pub struct CalibrationMetadata {
    /// Selected source backend.
    pub source: ClockSource,
    /// Raw counter reference tick.
    pub reference_ticks: u64,
    /// OS monotonic reference time, in nanoseconds (not UTC).
    pub reference_time_ns: u64,
    /// Fixed-point conversion numerator.
    pub multiplier: u64,
    /// Fixed-point conversion denominator, expressed as a power of two.
    pub shift: u32,
    /// Calibration convergence and statistical diagnostics.
    pub quality: CalibrationQuality,
}

impl Clock {
    /// Select and freshly calibrate a clock without reading or writing the
    /// process-global calibration cache. Inspect the quality report before
    /// accepting a counter backend: reaching the deadline is not convergence.
    pub fn new_uncached() -> Self {
        Self::new_uncached_with_options(CalibrationOptions::default())
    }

    /// Fresh calibration with explicit stopping criteria, bypassing the cache.
    pub fn new_uncached_with_options(options: CalibrationOptions) -> Self {
        let reference = Monotonic::default();
        let inner = if detection::has_counter_support() {
            let source = Counter;
            let mut calibration = Calibration::new();
            calibration.calibrate_with_options(reference, &source, options);
            ClockType::Counter(reference, source, calibration)
        } else {
            ClockType::Monotonic(reference)
        };
        Self { inner }
    }

    /// Select the OS monotonic backend explicitly, without calibration.
    pub fn monotonic() -> Self {
        Self {
            inner: ClockType::Monotonic(Monotonic::default()),
        }
    }

    /// Read the OS monotonic reference in nanoseconds, outside the raw hot path.
    pub fn reference_nanos(&self) -> u64 {
        match &self.inner {
            ClockType::Monotonic(reference) | ClockType::Counter(reference, _, _) => {
                reference.now()
            }
            #[cfg(feature = "mock")]
            ClockType::Mock(mock) => mock.value(),
        }
    }

    /// Report the reference clock's nominal resolution. This is a cold-path
    /// query, not a raw timestamp read. Browser precision may be reduced further
    /// by the host; 2 ms is conservatively reported without probing/spinning.
    pub fn reference_resolution(&self) -> Duration {
        #[cfg(any(
            target_os = "emscripten",
            not(any(target_os = "windows", target_arch = "wasm32"))
        ))]
        {
            let mut resolution = libc::timespec::default();
            // SAFETY: a valid writable timespec and a supported clock ID.
            if unsafe { libc::clock_getres(libc::CLOCK_MONOTONIC, &mut resolution) } == 0 {
                return Duration::new(
                    resolution.tv_sec.max(0) as u64,
                    resolution.tv_nsec.max(0) as u32,
                );
            }
        }
        #[cfg(target_os = "windows")]
        {
            let mut frequency = 0;
            // SAFETY: a valid writable QPC frequency result.
            if unsafe {
                windows_sys::Win32::System::Performance::QueryPerformanceFrequency(&mut frequency)
            } > 0
                && frequency > 0
            {
                return Duration::from_nanos(1_000_000_000_u64.div_ceil(frequency as u64));
            }
        }
        Duration::from_millis(2)
    }

    /// Copy this clock's source, conversion, and calibration diagnostics.
    /// This does not mutate the clock or the global calibration cache.
    pub fn calibration_metadata(&self) -> CalibrationMetadata {
        match &self.inner {
            #[cfg(target_os = "windows")]
            ClockType::Monotonic(monotonic) => {
                let raw = monotonic.raw();
                let (multiplier, shift) = monotonic.conversion();
                CalibrationMetadata {
                    source: ClockSource::PerformanceCounter,
                    reference_ticks: raw,
                    reference_time_ns: monotonic.scale(raw),
                    multiplier,
                    shift,
                    quality: CalibrationQuality::NOT_REQUIRED,
                }
            }
            ClockType::Counter(_, _, calibration) => CalibrationMetadata {
                source: if cfg!(target_arch = "aarch64") {
                    ClockSource::SystemCounter
                } else {
                    ClockSource::Tsc
                },
                reference_ticks: calibration.src_time,
                reference_time_ns: calibration.ref_time,
                multiplier: calibration.scale_factor,
                shift: calibration.scale_shift,
                quality: calibration.quality,
            },
            #[cfg(any(not(target_os = "windows"), feature = "mock"))]
            _ => {
                let raw = self.raw();
                CalibrationMetadata {
                    source: match &self.inner {
                        #[cfg(feature = "mock")]
                        ClockType::Mock(_) => ClockSource::Mock,
                        _ => ClockSource::Monotonic,
                    },
                    reference_ticks: raw,
                    reference_time_ns: raw,
                    multiplier: 1,
                    shift: 0,
                    quality: CalibrationQuality::NOT_REQUIRED,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_is_not_success_and_fresh_calibration_ignores_global_cache() {
        if !detection::has_counter_support() {
            return;
        }
        let reference = Monotonic::default();
        let mut calibration = Calibration::new();
        calibration.calibrate_with_options(
            reference,
            &Counter,
            CalibrationOptions {
                timeout: Duration::ZERO,
                ..CalibrationOptions::default()
            },
        );
        assert_eq!(
            calibration.quality.status,
            CalibrationStatus::DeadlineReached
        );
        assert_eq!(calibration.quality.samples, 0);
        let cached = Clock::new().calibration_metadata();
        let fresh = Clock::new_uncached().calibration_metadata();
        assert!(fresh.reference_ticks > cached.reference_ticks);
        assert_eq!(
            Clock::new().calibration_metadata().reference_ticks,
            cached.reference_ticks
        );
        assert!(fresh.quality.samples > 0);
    }
}
