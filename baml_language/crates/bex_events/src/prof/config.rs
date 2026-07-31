//! Knob parsing for the profiling event stream.
//!
//! All knobs are environment variables read once per process (see
//! [`ProfConfig::global`]). Unparseable values fall back to their defaults
//! rather than failing the host process; out-of-range values are clamped.

#[cfg(target_arch = "wasm32")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::{sync::OnceLock, time::Duration};

#[cfg(target_arch = "wasm32")]
static WASM_COOPERATIVE_PROFILE_ENABLED: AtomicBool = AtomicBool::new(false);
static GLOBAL_CONFIG: OnceLock<ProfConfig> = OnceLock::new();

/// Master switch for the profiling event stream. DEFAULT-ON (reconciliation
/// follow-up 16): set `0`/`false` to opt out. Any value other than
/// `1`/`true` disables; unset means enabled on native targets. On wasm32,
/// environment/default config stays off unless an adapter explicitly opts
/// into cooperative profiling.
pub const ENV_PROFILE: &str = "BAML_PROFILE";
/// Ring segment size in bytes, clamped to `[MIN_SEG_BYTES, MAX_SEG_BYTES]`.
pub const ENV_SEG_BYTES: &str = "BAML_RING_SEG_BYTES";
/// Hard cap on total live ring memory (bytes). Hitting it is a process error.
pub const ENV_MAX_OVERFLOW_BYTES: &str = "BAML_RING_MAX_OVERFLOW_BYTES";
/// Per-ring recycled-segment free-list cap (design D7).
pub const ENV_FREELIST_CAP: &str = "BAML_RING_FREELIST_CAP";
/// Consumer park timeout in milliseconds (design D4).
pub const ENV_WAKE_INTERVAL_MS: &str = "BAML_PROF_WAKE_INTERVAL_MS";
/// Directory for `.bamlprof` artifacts.
pub const ENV_PROFILE_DIR: &str = "BAML_PROFILE_DIR";
/// Consumer pipeline selected at the raw-record fan-out.
///
/// P0 deliberately routes every variant through the legacy sinks. Later
/// phases replace the `cct` arm and make `dual` feed both pipelines without
/// changing the producer hot path.
pub const ENV_PROFILE_PIPELINE: &str = "BAML_PROFILE_PIPELINE";
/// On-disk observability rollout. `dual` writes both the legacy event
/// artifact and the v2 session stream; `v1` and `v2` are explicit kill
/// switches for either side.
pub const ENV_OBS_LAYOUT: &str = "BAML_OBS_LAYOUT";
/// Optional path for a machine-readable consumer statistics snapshot.
pub const ENV_OBS_STATS: &str = "BAML_OBS_STATS";
/// Ring-cap behavior. `abort` is the loud development default; server/SDK
/// hosts set `shed` before starting their first engine.
pub const ENV_RING_OVERFLOW_POLICY: &str = "BAML_RING_OVERFLOW_POLICY";
/// Raw-byte flight-recorder capacity.
pub const ENV_FLIGHT_RECORDER_BYTES: &str = "BAML_FLIGHT_RECORDER_BYTES";
/// Opt-in exact full trace.
pub const ENV_FULL_TRACE: &str = "BAML_FULL_TRACE";
pub const ENV_FULL_TRACE_MAX_BYTES: &str = "BAML_FULL_TRACE_MAX_BYTES";
pub const ENV_FULL_TRACE_MAX_DURATION_MS: &str = "BAML_FULL_TRACE_MAX_DURATION_MS";
/// Optional root-boundary latency trigger. Unset/zero means disarmed.
pub const ENV_LATENCY_TRIGGER_MS: &str = "BAML_OBS_LATENCY_TRIGGER_MS";

/// Default `.bamlprof` home, relative to the working directory (`baml clean`
/// integration is an open coordination point).
pub const DEFAULT_PROFILE_DIR: &str = ".baml/profiles";

/// Default ring segment size. At ~28–40 B per record this holds roughly
/// 9,000 events, so the producer's slow path (segment link + possible wake)
/// runs about once per 9,000 pushes.
pub const DEFAULT_SEG_BYTES: usize = 256 * 1024;
/// Smallest allowed segment: keeps the slow path rare and guarantees any
/// single record (≤ [`crate::prof::record::MAX_RECORD_LEN`] bytes) fits.
pub const MIN_SEG_BYTES: usize = 64 * 1024;
/// Largest allowed segment; also keeps `commit_len` comfortably within `u32`.
pub const MAX_SEG_BYTES: usize = 16 * 1024 * 1024;
/// Default live-memory cap: 1 GiB ≈ 0.3 s of completely unconsumed burst at
/// the 100M ev/s × ~30 B/ev write budget (design D6).
pub const DEFAULT_MAX_OVERFLOW_BYTES: usize = 1 << 30;
/// Default free-list cap (design D7 names 2–4; idle cost is
/// `freelist_cap × seg_bytes` per ring, i.e. 1 MiB at the defaults).
pub const DEFAULT_FREELIST_CAP: usize = 4;
/// Default consumer park timeout. This timer is what bounds the documented
/// benign lost-wakeup race (design D4) to one interval of extra ring growth.
pub const DEFAULT_WAKE_INTERVAL: Duration = Duration::from_millis(50);
pub const DEFAULT_FLIGHT_RECORDER_BYTES: usize = 16 * 1024 * 1024;
pub const DEFAULT_FULL_TRACE_MAX_BYTES: usize = 64 * 1024 * 1024;
pub const DEFAULT_FULL_TRACE_MAX_DURATION: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RingOverflowPolicy {
    #[default]
    Abort,
    Shed,
}

impl RingOverflowPolicy {
    fn parse(value: Option<String>) -> Self {
        match value.as_deref().map(str::trim) {
            Some(value) if value.eq_ignore_ascii_case("shed") => Self::Shed,
            _ => Self::Abort,
        }
    }
}

/// Profiling consumer pipeline. The distinction is made only after a raw
/// range has left the producer rings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProfilePipeline {
    Legacy,
    #[default]
    Dual,
    Cct,
}

/// Durable artifact layout selected independently of the consumer algorithm.
/// Release-A defaults to `dual` so the existing `.bamlprof` oracle remains
/// available while readers prefer the v2 session stream.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ObsLayout {
    V1,
    #[default]
    Dual,
    V2,
}

impl ObsLayout {
    #[must_use]
    pub fn writes_v1(self) -> bool {
        matches!(self, Self::V1 | Self::Dual)
    }

    #[must_use]
    pub fn writes_v2(self) -> bool {
        matches!(self, Self::Dual | Self::V2)
    }

    fn parse(value: Option<String>) -> Self {
        match value.as_deref().map(str::trim) {
            Some(value) if value.eq_ignore_ascii_case("v1") => Self::V1,
            Some(value) if value.eq_ignore_ascii_case("v2") => Self::V2,
            Some(value) if value.eq_ignore_ascii_case("dual") => Self::Dual,
            _ => Self::Dual,
        }
    }
}

impl ProfilePipeline {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Dual => "dual",
            Self::Cct => "cct",
        }
    }

    fn parse(value: Option<String>) -> Self {
        match value.as_deref().map(str::trim) {
            Some(value) if value.eq_ignore_ascii_case("legacy") => Self::Legacy,
            Some(value) if value.eq_ignore_ascii_case("dual") => Self::Dual,
            Some(value) if value.eq_ignore_ascii_case("cct") => Self::Cct,
            _ => Self::Dual,
        }
    }
}

/// Parsed profiling knobs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfConfig {
    /// Master switch ([`ENV_PROFILE`]); default-on (follow-up 16). Always
    /// `false` on wasm32 from config alone: shared clock/bytes infrastructure
    /// exists, but a wasm adapter must explicitly own cooperative draining.
    pub enabled: bool,
    /// Ring segment size in bytes ([`ENV_SEG_BYTES`]).
    pub seg_bytes: usize,
    /// Hard cap on total live ring memory ([`ENV_MAX_OVERFLOW_BYTES`]).
    pub max_overflow_bytes: usize,
    /// Per-ring free-list cap ([`ENV_FREELIST_CAP`]).
    pub freelist_cap: usize,
    /// Consumer park timeout ([`ENV_WAKE_INTERVAL_MS`]).
    pub wake_interval: Duration,
    /// Where `.bamlprof` files land ([`ENV_PROFILE_DIR`]).
    pub profile_dir: std::path::PathBuf,
    /// Raw-record consumer fan-out ([`ENV_PROFILE_PIPELINE`]).
    pub pipeline: ProfilePipeline,
    /// Durable layout rollout ([`ENV_OBS_LAYOUT`]).
    pub obs_layout: ObsLayout,
    /// Machine-readable consumer statistics path ([`ENV_OBS_STATS`]).
    pub obs_stats_path: Option<std::path::PathBuf>,
    pub overflow_policy: RingOverflowPolicy,
    pub flight_recorder_bytes: usize,
    pub full_trace: bool,
    pub full_trace_max_bytes: usize,
    pub full_trace_max_duration: Duration,
    pub latency_trigger_ms: Option<u64>,
}

impl Default for ProfConfig {
    fn default() -> Self {
        ProfConfig {
            enabled: cfg!(not(target_arch = "wasm32")),
            seg_bytes: DEFAULT_SEG_BYTES,
            max_overflow_bytes: DEFAULT_MAX_OVERFLOW_BYTES,
            freelist_cap: DEFAULT_FREELIST_CAP,
            wake_interval: DEFAULT_WAKE_INTERVAL,
            profile_dir: std::path::PathBuf::from(DEFAULT_PROFILE_DIR),
            pipeline: ProfilePipeline::Dual,
            obs_layout: ObsLayout::Dual,
            obs_stats_path: None,
            overflow_policy: RingOverflowPolicy::Abort,
            flight_recorder_bytes: DEFAULT_FLIGHT_RECORDER_BYTES,
            full_trace: false,
            full_trace_max_bytes: DEFAULT_FULL_TRACE_MAX_BYTES,
            full_trace_max_duration: DEFAULT_FULL_TRACE_MAX_DURATION,
            latency_trigger_ms: None,
        }
    }
}

impl ProfConfig {
    /// Reads the knobs from the process environment.
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// The process-wide config, read from the environment exactly once.
    ///
    /// Producers never read this on the hot path: the engine snapshots
    /// `enabled` (and the ring pointer) into the VM once per exec resume.
    pub fn global() -> &'static ProfConfig {
        GLOBAL_CONFIG.get_or_init(ProfConfig::from_env)
    }

    /// Initialize the process profiler with a host-specific overflow policy.
    ///
    /// This is a race-free, first-initializer-wins seam for SDK/server hosts
    /// that need shedding rather than process abort. It must be called before
    /// constructing the first engine. Returns whether the effective global
    /// policy equals the requested policy (including when another initializer
    /// won the race).
    pub fn try_init_with_overflow_policy(policy: RingOverflowPolicy) -> bool {
        let mut config = Self::from_env();
        config.overflow_policy = policy;
        let _ = GLOBAL_CONFIG.set(config);
        Self::global().overflow_policy == policy
    }

    /// Effective profiling switch for producers.
    ///
    /// The parsed config remains default-off on wasm32 until an adapter that
    /// owns a cooperative drain opts in. This keeps non-playground wasm
    /// embedders from accumulating profile rings without an artifact sink.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled || wasm_cooperative_profile_enabled()
    }

    /// [`ProfConfig::from_env`] with an injectable lookup, so parsing and
    /// clamping are testable without touching the (process-global, racy)
    /// environment.
    pub(crate) fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let defaults = ProfConfig::default();

        let enabled = get(ENV_PROFILE)
            .map(|v| {
                let v = v.trim();
                v == "1" || v.eq_ignore_ascii_case("true")
            })
            .unwrap_or(defaults.enabled);
        // Environment/default profiling stays off on wasm32; adapters that
        // own a cooperative drain opt in through `is_enabled`.
        let enabled = enabled && cfg!(not(target_arch = "wasm32"));

        let seg_bytes = parse_usize(get(ENV_SEG_BYTES))
            .unwrap_or(defaults.seg_bytes)
            .clamp(MIN_SEG_BYTES, MAX_SEG_BYTES);

        // A cap below a handful of segments would make the very first ring
        // trip the hard error; keep at least a few segments of headroom.
        let max_overflow_bytes = parse_usize(get(ENV_MAX_OVERFLOW_BYTES))
            .unwrap_or(defaults.max_overflow_bytes)
            .max(seg_bytes * 4);

        let freelist_cap = parse_usize(get(ENV_FREELIST_CAP))
            .unwrap_or(defaults.freelist_cap)
            .clamp(0, 1024);

        let wake_interval = parse_usize(get(ENV_WAKE_INTERVAL_MS))
            .map_or(defaults.wake_interval, |ms| {
                Duration::from_millis(ms.clamp(1, 10_000) as u64)
            });

        let profile_dir =
            get(ENV_PROFILE_DIR).map_or(defaults.profile_dir, std::path::PathBuf::from);
        let pipeline = ProfilePipeline::parse(get(ENV_PROFILE_PIPELINE));
        let obs_layout = ObsLayout::parse(get(ENV_OBS_LAYOUT));
        let obs_stats_path = get(ENV_OBS_STATS)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from);
        let overflow_policy = RingOverflowPolicy::parse(get(ENV_RING_OVERFLOW_POLICY));
        let flight_recorder_bytes = parse_usize(get(ENV_FLIGHT_RECORDER_BYTES))
            .unwrap_or(defaults.flight_recorder_bytes)
            .min(1024 * 1024 * 1024);
        let full_trace = get(ENV_FULL_TRACE).is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "on" | "yes"
            )
        });
        let full_trace_max_bytes = parse_usize(get(ENV_FULL_TRACE_MAX_BYTES))
            .unwrap_or(defaults.full_trace_max_bytes)
            .max(64 * 1024);
        let full_trace_max_duration = parse_usize(get(ENV_FULL_TRACE_MAX_DURATION_MS)).map_or(
            defaults.full_trace_max_duration,
            |millis| {
                Duration::from_millis(
                    u64::try_from(millis.clamp(1, 24 * 60 * 60 * 1_000)).unwrap_or(u64::MAX),
                )
            },
        );
        let latency_trigger_ms = parse_usize(get(ENV_LATENCY_TRIGGER_MS))
            .filter(|millis| *millis != 0)
            .map(|millis| u64::try_from(millis).unwrap_or(u64::MAX));

        ProfConfig {
            enabled,
            seg_bytes,
            max_overflow_bytes,
            freelist_cap,
            wake_interval,
            profile_dir,
            pipeline,
            obs_layout,
            obs_stats_path,
            overflow_policy,
            flight_recorder_bytes,
            full_trace,
            full_trace_max_bytes,
            full_trace_max_duration,
            latency_trigger_ms,
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn enable_wasm_cooperative_profile() {
    WASM_COOPERATIVE_PROFILE_ENABLED.store(true, Ordering::Relaxed);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn enable_wasm_cooperative_profile() {}

#[cfg(target_arch = "wasm32")]
fn wasm_cooperative_profile_enabled() -> bool {
    WASM_COOPERATIVE_PROFILE_ENABLED.load(Ordering::Relaxed)
}

#[cfg(not(target_arch = "wasm32"))]
fn wasm_cooperative_profile_enabled() -> bool {
    false
}

fn parse_usize(value: Option<String>) -> Option<usize> {
    // Saturating on purpose: a numeric value beyond usize::MAX is an explicit
    // "huge" request, not garbage. Falling back to a default instead would
    // invert the user's intent — most dangerously for
    // BAML_RING_MAX_OVERFLOW_BYTES, where silently shrinking to the default
    // arms the very hard-error abort the user was trying to move away.
    value
        .and_then(|v| v.trim().parse::<u128>().ok())
        .map(|v| usize::try_from(v).unwrap_or(usize::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn defaults_when_unset() {
        let cfg = ProfConfig::from_lookup(lookup(&[]));
        assert_eq!(cfg, ProfConfig::default());
        // Follow-up 16: profiling ships default-on (native targets).
        assert_eq!(cfg.enabled, cfg!(not(target_arch = "wasm32")));
    }

    #[test]
    fn opt_out_disables() {
        for v in ["0", "false", "FALSE", "off"] {
            let cfg = ProfConfig::from_lookup(lookup(&[(ENV_PROFILE, v)]));
            assert!(!cfg.enabled, "BAML_PROFILE={v} must disable profiling");
        }
    }

    #[test]
    fn parses_all_knobs() {
        let cfg = ProfConfig::from_lookup(lookup(&[
            (ENV_PROFILE, "1"),
            (ENV_SEG_BYTES, "131072"),
            (ENV_MAX_OVERFLOW_BYTES, "536870912"),
            (ENV_FREELIST_CAP, "2"),
            (ENV_WAKE_INTERVAL_MS, "10"),
            (ENV_PROFILE_DIR, "/tmp/profs"),
            (ENV_PROFILE_PIPELINE, "dual"),
            (ENV_OBS_LAYOUT, "v2"),
            (ENV_OBS_STATS, "/tmp/obs-stats.json"),
            (ENV_RING_OVERFLOW_POLICY, "shed"),
            (ENV_FLIGHT_RECORDER_BYTES, "1048576"),
            (ENV_FULL_TRACE, "true"),
            (ENV_FULL_TRACE_MAX_BYTES, "2097152"),
            (ENV_FULL_TRACE_MAX_DURATION_MS, "2500"),
            (ENV_LATENCY_TRIGGER_MS, "9000"),
        ]));
        assert!(cfg.enabled);
        assert_eq!(cfg.seg_bytes, 128 * 1024);
        assert_eq!(cfg.max_overflow_bytes, 512 * 1024 * 1024);
        assert_eq!(cfg.freelist_cap, 2);
        assert_eq!(cfg.wake_interval, Duration::from_millis(10));
        assert_eq!(cfg.profile_dir, std::path::PathBuf::from("/tmp/profs"));
        assert_eq!(cfg.pipeline, ProfilePipeline::Dual);
        assert_eq!(cfg.obs_layout, ObsLayout::V2);
        assert_eq!(cfg.overflow_policy, RingOverflowPolicy::Shed);
        assert_eq!(cfg.flight_recorder_bytes, 1024 * 1024);
        assert!(cfg.full_trace);
        assert_eq!(cfg.full_trace_max_bytes, 2 * 1024 * 1024);
        assert_eq!(cfg.full_trace_max_duration, Duration::from_millis(2500));
        assert_eq!(cfg.latency_trigger_ms, Some(9000));
        assert_eq!(
            cfg.obs_stats_path,
            Some(std::path::PathBuf::from("/tmp/obs-stats.json"))
        );
    }

    #[test]
    fn profile_truthiness() {
        for on in ["1", "true", "TRUE", " true ", " 1"] {
            assert!(
                ProfConfig::from_lookup(lookup(&[(ENV_PROFILE, on)])).enabled,
                "{on:?} should enable"
            );
        }
        for off in ["0", "false", "", "yes", "2", "on"] {
            assert!(
                !ProfConfig::from_lookup(lookup(&[(ENV_PROFILE, off)])).enabled,
                "{off:?} should not enable"
            );
        }
    }

    #[test]
    fn parses_profile_pipeline_without_affecting_producer_enablement() {
        for (value, expected) in [
            ("legacy", ProfilePipeline::Legacy),
            ("LEGACY", ProfilePipeline::Legacy),
            ("dual", ProfilePipeline::Dual),
            ("cct", ProfilePipeline::Cct),
            ("unknown", ProfilePipeline::Dual),
        ] {
            let cfg = ProfConfig::from_lookup(lookup(&[(ENV_PROFILE_PIPELINE, value)]));
            assert_eq!(cfg.pipeline, expected);
            assert_eq!(cfg.enabled, ProfConfig::default().enabled);
        }
    }

    #[test]
    fn parses_obs_layout_and_defaults_dual() {
        assert_eq!(
            ProfConfig::from_lookup(lookup(&[])).obs_layout,
            ObsLayout::Dual
        );
        for (value, expected) in [
            ("v1", ObsLayout::V1),
            ("DUAL", ObsLayout::Dual),
            ("v2", ObsLayout::V2),
            ("unknown", ObsLayout::Dual),
        ] {
            assert_eq!(
                ProfConfig::from_lookup(lookup(&[(ENV_OBS_LAYOUT, value)])).obs_layout,
                expected
            );
        }
    }

    #[test]
    fn seg_bytes_clamped() {
        let low = ProfConfig::from_lookup(lookup(&[(ENV_SEG_BYTES, "1")]));
        assert_eq!(low.seg_bytes, MIN_SEG_BYTES);
        let high = ProfConfig::from_lookup(lookup(&[(ENV_SEG_BYTES, "999999999999")]));
        assert_eq!(high.seg_bytes, MAX_SEG_BYTES);
    }

    #[test]
    fn overflow_cap_keeps_segment_headroom() {
        let cfg = ProfConfig::from_lookup(lookup(&[(ENV_MAX_OVERFLOW_BYTES, "1")]));
        assert_eq!(cfg.max_overflow_bytes, cfg.seg_bytes * 4);
    }

    #[test]
    fn oversized_values_saturate_instead_of_shrinking_to_default() {
        // 2^64: beyond usize::MAX on every supported target. Must saturate —
        // falling back to the (smaller) default would arm the D6 abort the
        // user was raising the cap to avoid.
        let cfg =
            ProfConfig::from_lookup(lookup(&[(ENV_MAX_OVERFLOW_BYTES, "18446744073709551616")]));
        assert_eq!(cfg.max_overflow_bytes, usize::MAX);
        // seg_bytes still clamps to its documented range.
        let cfg = ProfConfig::from_lookup(lookup(&[(ENV_SEG_BYTES, "18446744073709551616")]));
        assert_eq!(cfg.seg_bytes, MAX_SEG_BYTES);
    }

    #[test]
    fn garbage_falls_back_to_defaults() {
        let cfg = ProfConfig::from_lookup(lookup(&[
            (ENV_SEG_BYTES, "not-a-number"),
            (ENV_MAX_OVERFLOW_BYTES, "-5"),
            (ENV_FREELIST_CAP, "1e3"),
            (ENV_WAKE_INTERVAL_MS, ""),
        ]));
        let defaults = ProfConfig::default();
        assert_eq!(cfg.seg_bytes, defaults.seg_bytes);
        assert_eq!(cfg.max_overflow_bytes, defaults.max_overflow_bytes);
        assert_eq!(cfg.freelist_cap, defaults.freelist_cap);
        assert_eq!(cfg.wake_interval, defaults.wake_interval);
    }

    #[test]
    fn wake_interval_clamped() {
        let cfg = ProfConfig::from_lookup(lookup(&[(ENV_WAKE_INTERVAL_MS, "0")]));
        assert_eq!(cfg.wake_interval, Duration::from_millis(1));
        let cfg = ProfConfig::from_lookup(lookup(&[(ENV_WAKE_INTERVAL_MS, "999999")]));
        assert_eq!(cfg.wake_interval, Duration::from_secs(10));
    }
}
