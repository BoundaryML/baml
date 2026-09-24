//! Cold-path calibration and validation tolerances. Reads stay raw.
use std::time::Duration;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockMode {
    Auto,
    Monotonic,
}
/// **Explicit backend tradeoff.** Auto can use cheap hardware reads; Monotonic favors OS
/// portability. Compare actual producer cost without weakening timing validity checks.
pub const DEFAULT_MODE: ClockMode = ClockMode::Auto;
// 100 ppm contributes 10 us between 100 ms checks. The residual margin leaves
// headroom within the 1 ms target; these are not statistical guarantees.
/// **Sensitive validation cadence.** Longer intervals reduce boundary checking but delay
/// fault detection and accumulate more scale uncertainty. Not a per-function read setting.
pub const VALIDATION_INTERVAL_DURATION: Duration = Duration::from_millis(100);
/// **Contract.** Normal-operation duration accuracy target, not a guarantee across host
/// migration. Change only with an explicit contract decision.
pub const ACCURACY_TARGET_NS: u64 = 1_000_000; // ns
/// **Sensitive tolerance.** Residual margin after accounting for sampling and rate
/// uncertainty. Revalidate fault detection and fallback before changing.
pub const DISCONTINUITY_MARGIN_NS: u64 = 250_000; // ns
/// **Sensitive tolerance.** Rejects noisy/descheduled samples. Raising it trades fewer
/// rejected samples for weaker observations.
pub const MAX_SAMPLE_UNCERTAINTY_NS: u64 = 50_000; // ns
/// **Sensitive startup cost.** Independent holdout window for validating Quanta scale.
/// Shortening it weakens rate estimation; it does not accelerate raw reads.
pub const SCALE_CHECK_INTERVAL_DURATION: Duration = Duration::from_millis(50);
/// **Sensitive tolerance.** Largest accepted scale-error estimate. Coupled to validation
/// interval and the duration accuracy budget.
pub const MAX_RATE_ERROR_PPB: u64 = 100_000;
/// **Sensitive tolerance.** Prevents treating statistical calibration as exact. Do not lower
/// merely to suppress uncertainty/fallback.
pub const RATE_ERROR_FLOOR_PPB: u64 = 10_000;
/// **Measure with calibration quality.** Maximum OS brackets sampled to find a sufficiently
/// narrow observation; affects cold validation work.
pub const PROBE_ATTEMPTS: usize = 3;
/// **Sensitive sampling tolerance.** Stops after a sufficiently tight bracket; validate
/// against OS clock resolution and scheduling noise.
pub const EARLY_PROBE_WIDTH_NS: u64 = 1_000;
/// **Sensitive startup cost.** Minimum calibration observation window, not per-call work.
/// Requires accuracy/fallback measurements before shortening.
pub const CALIBRATION_MIN_DURATION: Duration = Duration::from_millis(20);
/// **Sensitive convergence tolerance.** Shared by Quanta options and acceptance checks;
/// relaxation can admit a less accurate scale.
pub const CALIBRATION_MAX_ERROR_NS: u32 = 100;
/// **Startup bound.** Maximum calibration wait before fallback. Shorter waits can select the
/// slower OS backend more often.
pub const CALIBRATION_TIMEOUT_DURATION: Duration = Duration::from_millis(200);
/// **Sensitive convergence requirement.** Must observe strictly more samples than this;
/// shared with the vendored calibration loop.
pub const CALIBRATION_MIN_SAMPLES: u64 = 500;
const _: () = assert!(DISCONTINUITY_MARGIN_NS < ACCURACY_TARGET_NS);
const _: () = assert!(RATE_ERROR_FLOOR_PPB <= MAX_RATE_ERROR_PPB);
