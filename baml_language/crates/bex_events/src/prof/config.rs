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
/// Profile artifact anchor directory. Post-P9 no per-engine `.bamlprof`
/// files are written here; the directory's PARENT is the `.baml` root under
/// which the v2 planes land (`sessions/`, `dict/`, flight dumps).
pub const ENV_PROFILE_DIR: &str = "BAML_PROFILE_DIR";
/// Consumer pipeline selector (observability design §10.3). Post-P9 there is
/// exactly one pipeline — `cct` — so this knob only survives for
/// compatibility: the historical `legacy` and `dual` values are accepted and
/// silently coerced to `cct` (the sinks they named were deleted in P9 step 4).
pub const ENV_PROFILE_PIPELINE: &str = "BAML_PROFILE_PIPELINE";
/// When set to a writable file path, the prof consumer (and later the value
/// drain service) appends one NDJSON self-report line per flush/engine-close
/// with thread CPU, event/byte/flush counters, and pipeline mode
/// (observability design §10.3 — converts consumer-cost claims from
/// inference to measurement).
pub const ENV_OBS_STATS: &str = "BAML_OBS_STATS";
/// On-disk layout rollout flag (design §6.10). Post-P9 only the v2
/// sessions/CCT layout exists; the historical `v1` and `dual` stages are
/// accepted and silently coerced to `v2` (their writers were deleted in P9).
pub const ENV_OBS_LAYOUT: &str = "BAML_OBS_LAYOUT";
/// Opt-in raw record firehose (design §6.1 `raw/`, absorbing N5): rotated
/// `.bamlprof` of every drained range under the session dir. The first
/// casualty of retention/shedding; off by default.
pub const ENV_PROFILE_RAW: &str = "BAML_PROFILE_RAW";
/// Structural-exhaustion policy: what happens when live ring memory would
/// exceed [`ENV_MAX_OVERFLOW_BYTES`]. `fail_run` (default) latches capture
/// off and records the loss; `abort_process` is the strict opt-in hard
/// abort; `continue_incomplete` sheds while over the cap and resumes.
pub const ENV_PROFILE_EXHAUSTION: &str = "BAML_PROFILE_EXHAUSTION";

/// Default profile anchor, relative to the working directory (`baml clean`
/// integration is an open coordination point). Its parent (`.baml`) roots
/// the v2 session/dict layout.
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

/// The consumer pipeline ([`ENV_PROFILE_PIPELINE`]).
///
/// Post-P9 (deletion step 4) exactly one pipeline exists: the CCT
/// aggregation plane (design §5) plus its raw-firehose/flight sidecars. The
/// deleted `Legacy` and `Dual` variants' env spellings are still *parsed* —
/// coerced to [`PipelineMode::Cct`] — so stale environments never fail (or
/// silently un-profile) a host that pinned `BAML_PROFILE_PIPELINE=dual`
/// during the rollout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PipelineMode {
    /// CCT aggregation — the only pipeline since P9.
    #[default]
    Cct,
}

impl PipelineMode {
    /// Recognizes the current spelling plus the retired `legacy`/`dual`
    /// stages, all of which mean [`PipelineMode::Cct`] now (silent coercion:
    /// config parsing is pure/target-neutral, so there is no reporting
    /// channel here; the knob's doc records the compatibility rule).
    fn parse(value: &str) -> Option<PipelineMode> {
        match value.trim().to_ascii_lowercase().as_str() {
            "legacy" | "dual" | "cct" => Some(PipelineMode::Cct),
            _ => None,
        }
    }

    /// Does this mode run the CCT pipeline? Always `true` post-P9; kept so
    /// call sites written against the rollout-era API stay valid.
    #[must_use]
    pub fn runs_cct(self) -> bool {
        true
    }

    /// Stable lowercase name (stats lines, bench rows).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        "cct"
    }
}

/// §6.10 layout stage ([`ENV_OBS_LAYOUT`]). Post-P9 only the v2
/// sessions/CCT layout exists; the retired `v1` and `dual` spellings parse
/// to [`ObsLayout::V2`] (same compatibility rule as [`PipelineMode`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ObsLayout {
    /// v2 sessions/CCT streams — the only layout since P9.
    #[default]
    V2,
}

impl ObsLayout {
    fn parse(value: &str) -> Option<ObsLayout> {
        match value.trim().to_ascii_lowercase().as_str() {
            "v1" | "dual" | "v2" => Some(ObsLayout::V2),
            _ => None,
        }
    }

    /// Does this layout write the v2 session streams? Always `true` post-P9;
    /// kept for rollout-era call sites.
    #[must_use]
    pub fn writes_v2(self) -> bool {
        true
    }
}

/// Structural-exhaustion policy ([`ENV_PROFILE_EXHAUSTION`]): the decided
/// replacement for the historical unconditional abort. All three modes
/// keep the loss visible — dropped-record counters, shed markers, and
/// boundary loss records — never a silent gap.
///
/// The register's recommended default is `fail_run`; the exact per-
/// environment defaults remain X1 policy work, so hosts may override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExhaustionPolicy {
    /// Latch capture off for the rest of the process on the first cap
    /// breach. The application continues; the run's structural evidence is
    /// explicitly failed/incomplete from the breach onward.
    #[default]
    FailRun,
    /// Strict opt-in: abort the process with the documented diagnostic
    /// (the pre-policy behavior).
    AbortProcess,
    /// Diagnostic admission: drop records while over the cap, resume when
    /// the consumer catches up. Every shed window is marked degraded.
    ContinueIncomplete,
}

impl ExhaustionPolicy {
    fn parse(value: &str) -> Option<ExhaustionPolicy> {
        match value.trim().to_ascii_lowercase().as_str() {
            "fail_run" => Some(ExhaustionPolicy::FailRun),
            "abort_process" => Some(ExhaustionPolicy::AbortProcess),
            "continue_incomplete" => Some(ExhaustionPolicy::ContinueIncomplete),
            _ => None,
        }
    }

    /// Stable lowercase name (stats lines, markers).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ExhaustionPolicy::FailRun => "fail_run",
            ExhaustionPolicy::AbortProcess => "abort_process",
            ExhaustionPolicy::ContinueIncomplete => "continue_incomplete",
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
    /// Consumer pipeline selection ([`ENV_PROFILE_PIPELINE`]).
    pub pipeline: PipelineMode,
    /// Self-report NDJSON path ([`ENV_OBS_STATS`]); `None` = reporting off.
    pub obs_stats_path: Option<std::path::PathBuf>,
    /// On-disk layout stage ([`ENV_OBS_LAYOUT`]).
    pub layout: ObsLayout,
    /// Raw firehose opt-in ([`ENV_PROFILE_RAW`]).
    pub profile_raw: bool,
    /// Structural-exhaustion policy ([`ENV_PROFILE_EXHAUSTION`]).
    pub exhaustion: ExhaustionPolicy,
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
            pipeline: PipelineMode::default(),
            obs_stats_path: None,
            layout: ObsLayout::default(),
            profile_raw: false,
            exhaustion: ExhaustionPolicy::default(),
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
        static GLOBAL: OnceLock<ProfConfig> = OnceLock::new();
        GLOBAL.get_or_init(ProfConfig::from_env)
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

        // Unknown pipeline values fall back to the default (same contract as
        // every other knob: garbage never fails the host process).
        let pipeline = get(ENV_PROFILE_PIPELINE)
            .and_then(|v| PipelineMode::parse(&v))
            .unwrap_or(defaults.pipeline);

        let obs_stats_path = get(ENV_OBS_STATS)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .map(std::path::PathBuf::from);

        let layout = get(ENV_OBS_LAYOUT)
            .and_then(|v| ObsLayout::parse(&v))
            .unwrap_or(defaults.layout);
        let profile_raw = get(ENV_PROFILE_RAW)
            .map(|v| {
                let v = v.trim();
                v == "1" || v.eq_ignore_ascii_case("true")
            })
            .unwrap_or(defaults.profile_raw);

        let exhaustion = get(ENV_PROFILE_EXHAUSTION)
            .and_then(|v| ExhaustionPolicy::parse(&v))
            .unwrap_or(defaults.exhaustion);

        ProfConfig {
            enabled,
            seg_bytes,
            max_overflow_bytes,
            freelist_cap,
            wake_interval,
            profile_dir,
            pipeline,
            obs_stats_path,
            layout,
            profile_raw,
            exhaustion,
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
        ]));
        assert!(cfg.enabled);
        assert_eq!(cfg.seg_bytes, 128 * 1024);
        assert_eq!(cfg.max_overflow_bytes, 512 * 1024 * 1024);
        assert_eq!(cfg.freelist_cap, 2);
        assert_eq!(cfg.wake_interval, Duration::from_millis(10));
        assert_eq!(cfg.profile_dir, std::path::PathBuf::from("/tmp/profs"));
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
    fn pipeline_mode_parses_and_defaults() {
        assert_eq!(
            ProfConfig::from_lookup(lookup(&[])).pipeline,
            PipelineMode::Cct,
            "P9: the CCT pipeline is the default"
        );
        // Post-P9 compatibility: the retired rollout stages coerce to Cct
        // instead of failing or silently disabling profiling.
        for v in ["legacy", "dual", "cct", " CCT ", "DUAL"] {
            assert_eq!(
                ProfConfig::from_lookup(lookup(&[(ENV_PROFILE_PIPELINE, v)])).pipeline,
                PipelineMode::Cct,
                "BAML_PROFILE_PIPELINE={v}"
            );
        }
        // Garbage falls back to the default instead of failing the host.
        assert_eq!(
            ProfConfig::from_lookup(lookup(&[(ENV_PROFILE_PIPELINE, "both")])).pipeline,
            PipelineMode::Cct
        );
    }

    #[test]
    fn pipeline_mode_sink_selection() {
        assert!(PipelineMode::Cct.runs_cct());
        assert_eq!(PipelineMode::Cct.as_str(), "cct");
    }

    #[test]
    fn obs_stats_path_parses() {
        assert_eq!(ProfConfig::from_lookup(lookup(&[])).obs_stats_path, None);
        assert_eq!(
            ProfConfig::from_lookup(lookup(&[(ENV_OBS_STATS, "/tmp/stats.ndjson")])).obs_stats_path,
            Some(std::path::PathBuf::from("/tmp/stats.ndjson"))
        );
        // Empty/whitespace value means off, not a file named "".
        assert_eq!(
            ProfConfig::from_lookup(lookup(&[(ENV_OBS_STATS, "  ")])).obs_stats_path,
            None
        );
    }

    #[test]
    fn layout_and_raw_parse() {
        assert_eq!(ProfConfig::from_lookup(lookup(&[])).layout, ObsLayout::V2);
        // Retired stages coerce; garbage falls back to the default.
        for v in ["v1", "dual", "v2", "junk"] {
            assert_eq!(
                ProfConfig::from_lookup(lookup(&[(ENV_OBS_LAYOUT, v)])).layout,
                ObsLayout::V2
            );
        }
        assert!(!ProfConfig::from_lookup(lookup(&[])).profile_raw);
        assert!(ProfConfig::from_lookup(lookup(&[(ENV_PROFILE_RAW, "1")])).profile_raw);
        assert!(ObsLayout::V2.writes_v2());
    }

    #[test]
    fn exhaustion_policy_parses_and_defaults() {
        assert_eq!(
            ProfConfig::from_lookup(lookup(&[])).exhaustion,
            ExhaustionPolicy::FailRun,
            "the register's recommended default is fail_run"
        );
        for (v, want) in [
            ("fail_run", ExhaustionPolicy::FailRun),
            ("abort_process", ExhaustionPolicy::AbortProcess),
            ("continue_incomplete", ExhaustionPolicy::ContinueIncomplete),
            (" ABORT_PROCESS ", ExhaustionPolicy::AbortProcess),
        ] {
            assert_eq!(
                ProfConfig::from_lookup(lookup(&[(ENV_PROFILE_EXHAUSTION, v)])).exhaustion,
                want,
                "BAML_PROFILE_EXHAUSTION={v}"
            );
        }
        // Garbage falls back to the default instead of failing the host.
        assert_eq!(
            ProfConfig::from_lookup(lookup(&[(ENV_PROFILE_EXHAUSTION, "shed-maybe")])).exhaustion,
            ExhaustionPolicy::FailRun
        );
    }

    #[test]
    fn wake_interval_clamped() {
        let cfg = ProfConfig::from_lookup(lookup(&[(ENV_WAKE_INTERVAL_MS, "0")]));
        assert_eq!(cfg.wake_interval, Duration::from_millis(1));
        let cfg = ProfConfig::from_lookup(lookup(&[(ENV_WAKE_INTERVAL_MS, "999999")]));
        assert_eq!(cfg.wake_interval, Duration::from_secs(10));
    }
}
