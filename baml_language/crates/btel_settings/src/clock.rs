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
/// migration, plus `MAX_DRIFT_PPB` of the epoch's length. Change only with an explicit
/// contract decision.
pub const ACCURACY_TARGET_NS: u64 = 1_000_000; // ns
/// **Contract.** How far a long epoch may drift from the OS monotonic clock beyond the
/// accuracy target, as a share of its length. NTP slews that clock by a few ppm, so a rate
/// fixed at calibration drifts from it; with a flat 1 ms, every duration of a run longer
/// than a few minutes was dropped. 100 ppm is 0.01% of a duration (6 ms over ten
/// minutes); a larger jump still ends the epoch.
pub const MAX_DRIFT_PPB: u64 = 100_000;
/// **Sensitive tolerance.** Residual margin after accounting for sampling and rate
/// uncertainty. Revalidate fault detection and fallback before changing.
pub const DISCONTINUITY_MARGIN_NS: u64 = 250_000; // ns
/// **Sensitive tolerance.** Rejects noisy/descheduled samples. Raising it trades fewer
/// rejected samples for weaker observations.
pub const MAX_SAMPLE_UNCERTAINTY_NS: u64 = 50_000; // ns
/// **Background observation.** Independent holdout window after a candidate scale.
/// This is elapsed program time; construction and shutdown never wait for it.
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
/// **Background sampling cadence.** Each visit takes at most three reference brackets.
/// A candidate needs this much naturally elapsed program time, never a sleep.
pub const CALIBRATION_SAMPLE_INTERVAL_DURATION: Duration = Duration::from_millis(20);
const _: () = assert!(DISCONTINUITY_MARGIN_NS < ACCURACY_TARGET_NS);
const _: () = assert!(RATE_ERROR_FLOOR_PPB <= MAX_RATE_ERROR_PPB);
