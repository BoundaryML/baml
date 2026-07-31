//! `baml clean` (observability design §6.7/§6.8): one retention pass over
//! the project's `.baml` (age/size budgets, ordered degradation,
//! tombstoned deletions) followed by one value-store GC pass (mark from
//! boundary manifests, sweep unreferenced packs past the grace window).
//!
//! GC never runs concurrently with live value writers: it skips with a
//! notice, and the command still exits 0 — a busy store is not an error.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use anyhow::Result;
use bex_events::store::{
    gc::{self, GcReport},
    retention::{self, RetentionPolicy, RetentionReport},
};
use clap::Args;

use crate::project_load::find_project_root_from;

/// Clean up `.baml` observability data (history, sessions, value store).
///
/// Runs the retention pass (age/size budgets over `history/`, `sessions/`,
/// and legacy `profiles/`) and then garbage-collects the content-addressed
/// value store by reachability from the kept boundaries.
#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Retention removes whole boundary/session directories once they age out or
their root exceeds its size budget (newest boundaries are always kept).
Store GC then sweeps value packs no kept boundary references, after a
grace window. Every deletion is tombstoned in `.baml/retention.log`.

If live writers hold the store lock, GC is skipped with a notice and the
command still succeeds.

Examples:
  Preview what would be removed:
    baml clean --dry-run

  Clean the nearest project:
    baml clean

  Only garbage-collect the value store:
    baml clean --gc-only

  Sweep unreferenced packs immediately:
    baml clean --grace-hours 0")]
pub struct CleanArgs {
    /// Report what would be removed without deleting anything.
    ///
    /// Applies to the retention pass; the store GC pass is skipped
    /// entirely under `--dry-run` (a GC sweep cannot be previewed without
    /// taking the writers lock).
    #[arg(long)]
    pub dry_run: bool,

    /// Run only the value-store GC pass (skip retention).
    #[arg(long, conflicts_with = "retention_only")]
    pub gc_only: bool,

    /// Run only the retention pass (skip value-store GC).
    #[arg(long, conflicts_with = "gc_only")]
    pub retention_only: bool,

    /// Grace window in hours before unreferenced store chunks become
    /// sweepable.
    #[arg(long, value_name = "N", default_value_t = 24)]
    pub grace_hours: u64,

    /// Deprecated alias for `--project`.
    #[arg(long, value_name = "PATH", hide = true)]
    pub from: Option<PathBuf>,
}

/// What one `baml clean` invocation did — separated from arg handling so
/// the pass composition and summary rendering are unit-testable.
#[derive(Debug, Default)]
pub(crate) struct CleanSummary {
    pub(crate) retention: Option<RetentionReport>,
    pub(crate) gc: Option<GcReport>,
    /// Why the GC pass did not run at all (dry-run / --retention-only).
    pub(crate) gc_not_run: Option<&'static str>,
}

impl CleanArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let Some(project_root) = find_project_root_from(self.from.as_deref())? else {
            crate::reporter::print_error(format_args!(
                "no BAML project found (looked for `baml.toml` or `baml_src/` \
                 from the current directory upward)"
            ));
            return Ok(crate::ExitCode::Other);
        };
        let baml_dir = project_root.join(".baml");
        if !baml_dir.is_dir() {
            println!("nothing to clean: {} does not exist", baml_dir.display());
            return Ok(crate::ExitCode::Success);
        }

        let summary = self.clean_baml_dir(&baml_dir)?;
        print!("{}", render_summary(&summary));
        // A skipped GC (live writers) is a notice, not a failure.
        Ok(crate::ExitCode::Success)
    }

    /// Retention first (releases whole boundaries and their manifests),
    /// then GC (sweeps the now-unreferenced CAS closure) — §6.8 ordering.
    pub(crate) fn clean_baml_dir(&self, baml_dir: &std::path::Path) -> Result<CleanSummary> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0);

        let mut summary = CleanSummary::default();
        if !self.gc_only {
            summary.retention = Some(retention::clean(
                baml_dir,
                &RetentionPolicy::default(),
                now_ms,
                self.dry_run,
            )?);
        }
        if self.retention_only {
            summary.gc_not_run = Some("--retention-only");
        } else if self.dry_run {
            summary.gc_not_run = Some("--dry-run; a GC sweep cannot be previewed");
        } else {
            summary.gc = Some(gc::gc(
                baml_dir,
                now_ms,
                self.grace_hours.saturating_mul(60 * 60 * 1000),
            )?);
        }
        Ok(summary)
    }
}

/// Human summary: what each pass removed/kept, byte totals, and the skip
/// notices (dry run, live writers).
pub(crate) fn render_summary(summary: &CleanSummary) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();

    if let Some(retention) = &summary.retention {
        let verb = if retention.dry_run {
            "retention (dry run): would remove"
        } else {
            "retention: removed"
        };
        let _ = writeln!(
            out,
            "{verb} {} boundar{}, {} session(s), {} raw file(s), {} flight dump(s), \
             {} trace segment(s), {} legacy profile(s) ({})",
            retention.boundaries_removed,
            if retention.boundaries_removed == 1 {
                "y"
            } else {
                "ies"
            },
            retention.sessions_removed,
            retention.raw_files_removed,
            retention.flight_dumps_removed,
            retention.trace_segments_removed,
            retention.legacy_profiles_removed,
            human_bytes(retention.bytes_reclaimed),
        );
    }

    match (&summary.gc, summary.gc_not_run) {
        (Some(gc), _) => {
            if let Some(reason) = &gc.skipped {
                let _ = writeln!(out, "store gc: skipped ({reason})");
            } else {
                let _ = writeln!(
                    out,
                    "store gc: {} root(s), {} chunk(s) marked; packs: {} kept, \
                     {} unlinked, {} compacted ({})",
                    gc.roots,
                    gc.marked,
                    gc.packs_kept,
                    gc.packs_unlinked,
                    gc.packs_compacted,
                    human_bytes(gc.bytes_reclaimed),
                );
            }
        }
        (None, Some(reason)) => {
            let _ = writeln!(out, "store gc: not run ({reason})");
        }
        (None, None) => {}
    }

    let reclaimed = summary.retention.as_ref().map_or(0, |r| r.bytes_reclaimed)
        + summary.gc.as_ref().map_or(0, |g| g.bytes_reclaimed);
    let dry = summary.retention.as_ref().is_some_and(|r| r.dry_run);
    let _ = writeln!(
        out,
        "{} {}",
        if dry { "would reclaim" } else { "reclaimed" },
        human_bytes(reclaimed),
    );
    out
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("{prefix}_{}_{n}", std::process::id()))
    }

    fn args() -> CleanArgs {
        CleanArgs {
            dry_run: false,
            gc_only: false,
            retention_only: false,
            grace_hours: 24,
            from: None,
        }
    }

    #[test]
    fn human_bytes_scales() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 KiB");
        assert_eq!(human_bytes(3 * 1024 * 1024), "3.0 MiB");
    }

    /// Full pass over an empty-ish `.baml`: retention runs, GC reports
    /// "no store" as a skip — and none of it is an error.
    #[test]
    fn clean_runs_both_passes_and_tolerates_missing_store() {
        let baml = unique_temp_dir("baml_clean_both");
        std::fs::create_dir_all(baml.join("history")).unwrap();
        let summary = args().clean_baml_dir(&baml).unwrap();
        let retention = summary.retention.as_ref().expect("retention ran");
        assert_eq!(retention.boundaries_removed, 0);
        let gc = summary.gc.as_ref().expect("gc ran");
        assert_eq!(gc.skipped.as_deref(), Some("no store"));
        let rendered = render_summary(&summary);
        assert!(
            rendered.contains("retention: removed 0 boundaries"),
            "{rendered}"
        );
        assert!(
            rendered.contains("store gc: skipped (no store)"),
            "{rendered}"
        );
        assert!(rendered.contains("reclaimed 0 B"), "{rendered}");
        let _ = std::fs::remove_dir_all(&baml);
    }

    /// `--dry-run` deletes nothing and skips GC with a notice.
    #[test]
    fn dry_run_previews_retention_and_skips_gc() {
        let baml = unique_temp_dir("baml_clean_dry");
        let raw = baml.join("sessions/1-aa-e1/raw");
        std::fs::create_dir_all(&raw).unwrap();
        std::fs::write(raw.join("raw-000001.bamlprof"), vec![0u8; 1024]).unwrap();
        let mut clean = args();
        clean.dry_run = true;
        let summary = clean.clean_baml_dir(&baml).unwrap();
        assert!(summary.retention.as_ref().unwrap().dry_run);
        assert!(summary.gc.is_none());
        assert!(summary.gc_not_run.unwrap().contains("--dry-run"));
        assert!(
            raw.join("raw-000001.bamlprof").exists(),
            "dry run keeps files"
        );
        let rendered = render_summary(&summary);
        assert!(rendered.contains("would reclaim"), "{rendered}");
        assert!(rendered.contains("store gc: not run"), "{rendered}");
        let _ = std::fs::remove_dir_all(&baml);
    }

    /// `--retention-only` / `--gc-only` each run exactly one pass.
    #[test]
    fn pass_selection_flags_are_exclusive_in_effect() {
        let baml = unique_temp_dir("baml_clean_sel");
        std::fs::create_dir_all(&baml).unwrap();

        let mut retention_only = args();
        retention_only.retention_only = true;
        let summary = retention_only.clean_baml_dir(&baml).unwrap();
        assert!(summary.retention.is_some());
        assert!(summary.gc.is_none());
        assert_eq!(summary.gc_not_run, Some("--retention-only"));

        let mut gc_only = args();
        gc_only.gc_only = true;
        let summary = gc_only.clean_baml_dir(&baml).unwrap();
        assert!(summary.retention.is_none());
        assert!(summary.gc.is_some());
        let _ = std::fs::remove_dir_all(&baml);
    }

    /// Old boundaries beyond the newest-20 floor actually go, and the
    /// deletions are tombstoned — the §6.8 contract end to end.
    #[test]
    fn clean_removes_aged_boundaries_beyond_the_floor() {
        let baml = unique_temp_dir("baml_clean_age");
        for i in 0..25 {
            let dir = baml.join("history").join(format!("{i:02}-run-x"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("boundary.bamlmeta"), vec![0u8; 16]).unwrap();
        }
        // Make everything "old": set mtimes into the past is awkward
        // portably, so lean on the retention test seam instead — a
        // zero-age policy through the library, then assert the CLI
        // summary rendering over the report.
        let now_ms = u64::MAX / 2;
        let report = retention::clean(
            &baml,
            &RetentionPolicy {
                history_age_ms: 0,
                ..RetentionPolicy::default()
            },
            now_ms,
            false,
        )
        .unwrap();
        assert_eq!(report.boundaries_removed, 5, "25 minus the newest-20 floor");
        assert!(
            baml.join("retention.log").is_file(),
            "deletions are tombstoned"
        );
        let summary = CleanSummary {
            retention: Some(report),
            gc: None,
            gc_not_run: Some("--retention-only"),
        };
        let rendered = render_summary(&summary);
        assert!(rendered.contains("removed 5 boundaries"), "{rendered}");
        let _ = std::fs::remove_dir_all(&baml);
    }

    /// GC exits cleanly when live writers hold the store lock: report
    /// carries the notice; the command's exit code stays success (the
    /// `run()` path always returns `Success` after a summary).
    #[test]
    fn gc_skip_with_live_writers_is_a_notice_not_an_error() {
        let baml = unique_temp_dir("baml_clean_lock");
        std::fs::create_dir_all(baml.join("store/packs")).unwrap();
        // A live store writer holds writers.lock SHARED.
        let mut store = bex_events::store::Store::open(&baml.join("store"), [7; 16]).unwrap();
        let encoded = bex_events::store::canon::encode(
            &bex_events::store::canon::CanonValue::String("busy".into()),
        );
        store.put_encoded(&encoded, 1).unwrap();

        let summary = args().clean_baml_dir(&baml).unwrap();
        let gc = summary.gc.as_ref().expect("gc pass ran");
        assert!(
            gc.skipped.as_deref().is_some_and(|s| s.contains("writers")),
            "{gc:?}"
        );
        let rendered = render_summary(&summary);
        assert!(rendered.contains("store gc: skipped"), "{rendered}");
        drop(store);
        let _ = std::fs::remove_dir_all(&baml);
    }
}
