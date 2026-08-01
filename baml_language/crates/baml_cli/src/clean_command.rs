//! Retention and value-store garbage collection for local observability data.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use bex_events::{
    ids::ProcessEuid,
    value_cas::{
        ProjectGcOptions, RetentionCandidate, RetentionPolicy, derive_unsealed_bamlvalue_roots,
        execute_project_gc, execute_retention_plan, plan_retention,
    },
};
use clap::Args;

const DEFAULT_HISTORY_MAX_AGE_DAYS: u64 = 30;
const DEFAULT_HISTORY_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const DEFAULT_NEWEST_FLOOR: usize = 20;
const DEFAULT_GC_GRACE_HOURS: u64 = 24;

#[derive(Args, Clone, Debug)]
pub(crate) struct CleanArgs {
    /// Discover the BAML project from this path.
    #[arg(long, value_name = "PATH", help_heading = "Project options")]
    pub from: Option<PathBuf>,

    /// Report the retention and GC plan without deleting data.
    #[arg(long)]
    pub dry_run: bool,

    /// Remove all retained observability history before collecting values.
    #[arg(long, conflicts_with_all = ["max_age_days", "max_bytes", "keep"])]
    pub all: bool,

    /// Delete history older than this many days.
    #[arg(long)]
    pub max_age_days: Option<u64>,

    /// Bound retained boundary history bytes.
    #[arg(long)]
    pub max_bytes: Option<u64>,

    /// Always preserve this many newest boundaries.
    #[arg(long)]
    pub keep: Option<usize>,

    /// Do not reclaim newly-unreferenced value chunks until this grace expires.
    #[arg(long)]
    pub gc_grace_hours: Option<u64>,
}

impl CleanArgs {
    pub(crate) fn run(&self) -> Result<crate::ExitCode> {
        let project_root = crate::project_load::find_project_root_from(self.from.as_deref())?
            .with_context(|| {
                let start = self
                    .from
                    .as_deref()
                    .map_or_else(|| Path::new(".").to_path_buf(), Path::to_path_buf);
                format!(
                    "could not find a BAML project from {}; pass --project or --from",
                    start.display()
                )
            })?;
        let project_baml_dir = project_root.join(".baml");
        if !project_baml_dir.exists() {
            println!(
                "No observability history found in {}",
                project_baml_dir.display()
            );
            return Ok(crate::ExitCode::Success);
        }

        let configured = crate::manifest::observability_retention_for_root(&project_root)?;
        let max_age_days = self
            .max_age_days
            .or(configured.history_max_age_days)
            .unwrap_or(DEFAULT_HISTORY_MAX_AGE_DAYS);
        let max_bytes = self
            .max_bytes
            .or(configured.history_max_bytes)
            .unwrap_or(DEFAULT_HISTORY_MAX_BYTES);
        let newest_floor = self
            .keep
            .or(configured.newest_boundary_floor)
            .unwrap_or(DEFAULT_NEWEST_FLOOR);
        let gc_grace_hours = self
            .gc_grace_hours
            .or(configured.gc_grace_hours)
            .unwrap_or(DEFAULT_GC_GRACE_HOURS);
        let now_ms = now_ms()?;
        let history = collect_history_candidates(&project_baml_dir.join("history"))?;
        let history_plan = if self.all {
            history
        } else {
            let max_age_ms = max_age_days
                .checked_mul(24 * 60 * 60 * 1_000)
                .context("--max-age-days is too large")?;
            plan_retention(
                &history,
                RetentionPolicy {
                    max_age_ms: Some(max_age_ms),
                    max_bytes: Some(max_bytes),
                    newest_floor,
                },
                now_ms,
            )
        };
        let mut planned_bytes = history_plan
            .iter()
            .map(|candidate| candidate.bytes)
            .fold(0_u64, u64::saturating_add);
        let mut retention = execute_retention_plan(
            &project_baml_dir,
            &history_plan,
            now_ms,
            self.dry_run,
            "boundary",
            if self.all {
                "clean_all"
            } else {
                "history_policy"
            },
        )
        .context("failed to apply boundary retention")?;

        if self.all {
            let auxiliary = collect_all_auxiliary_candidates(&project_baml_dir)?;
            planned_bytes = planned_bytes.saturating_add(
                auxiliary
                    .iter()
                    .map(|candidate| candidate.bytes)
                    .fold(0_u64, u64::saturating_add),
            );
            let auxiliary_retention = execute_retention_plan(
                &project_baml_dir,
                &auxiliary,
                now_ms,
                self.dry_run,
                "observability_artifact",
                "clean_all",
            )
            .context("failed to remove auxiliary observability data")?;
            retention.planned = retention
                .planned
                .saturating_add(auxiliary_retention.planned);
            retention.deleted = retention
                .deleted
                .saturating_add(auxiliary_retention.deleted);
            retention.deleted_bytes = retention
                .deleted_bytes
                .saturating_add(auxiliary_retention.deleted_bytes);
        }

        let grace_ms = gc_grace_hours
            .checked_mul(60 * 60 * 1_000)
            .context("--gc-grace-hours is too large")?;
        let gc = execute_project_gc(
            &project_baml_dir,
            ProjectGcOptions {
                now_ms,
                grace_ms,
                dry_run: self.dry_run,
                origin_euid: ProcessEuid::current().0,
                first_repack_seq: 0x8000_0000,
            },
            derive_unsealed_bamlvalue_roots,
        )
        .context("failed to garbage-collect captured values")?;

        let action = if self.dry_run {
            "would delete"
        } else {
            "deleted"
        };
        println!(
            "{action} {} observability unit(s), {} byte(s); value GC examined {} pack(s), \
             deleted {}, compacted {}, reclaimable {} byte(s){}",
            if self.dry_run {
                retention.planned
            } else {
                retention.deleted
            },
            if self.dry_run {
                planned_bytes
            } else {
                retention.deleted_bytes
            },
            gc.packs_examined,
            gc.packs_deleted,
            gc.packs_compacted,
            gc.reclaimable_stored_bytes,
            if gc.skipped_live_writers {
                "; value GC skipped because a writer is live"
            } else {
                ""
            }
        );
        Ok(crate::ExitCode::Success)
    }
}

fn collect_history_candidates(history_root: &Path) -> io::Result<Vec<RetentionCandidate>> {
    let mut candidates = Vec::new();
    let entries = match fs::read_dir(history_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(candidates),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        let path = entry.path();
        candidates.push(RetentionCandidate {
            created_ms: boundary_created_ms(&path)?,
            bytes: directory_bytes(&path)?,
            path,
        });
    }
    candidates.sort_by(|left, right| {
        left.created_ms
            .cmp(&right.created_ms)
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(candidates)
}

fn collect_all_auxiliary_candidates(
    project_baml_dir: &Path,
) -> io::Result<Vec<RetentionCandidate>> {
    let mut candidates = Vec::new();
    for root_name in ["sessions", "profiles", "dict"] {
        let root = project_baml_dir.join(root_name);
        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        for entry in entries {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            candidates.push(RetentionCandidate {
                created_ms: entry_created_ms(&entry)?,
                bytes: if file_type.is_dir() {
                    directory_bytes(&path)?
                } else if file_type.is_file() {
                    entry.metadata()?.len()
                } else {
                    continue;
                },
                path,
            });
        }
    }
    Ok(candidates)
}

fn boundary_created_ms(path: &Path) -> io::Result<u64> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split_once('-'))
        .and_then(|(created, _)| created.parse().ok())
        .or_else(|| {
            fs::metadata(path)
                .ok()?
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()
                .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "boundary has no creation time"))
}

fn entry_created_ms(entry: &fs::DirEntry) -> io::Result<u64> {
    let modified = entry.metadata()?.modified()?;
    u64::try_from(
        modified
            .duration_since(UNIX_EPOCH)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "mtime predates Unix epoch"))?
            .as_millis(),
    )
    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "mtime is out of range"))
}

fn directory_bytes(root: &Path) -> io::Result<u64> {
    let mut total = 0_u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                total = total.saturating_add(entry.metadata()?.len());
            }
        }
    }
    Ok(total)
}

fn now_ms() -> Result<u64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock predates the Unix epoch")?;
    u64::try_from(duration.as_millis()).context("system clock is out of range")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_scan_is_boundary_scoped_and_never_follows_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let history = temp.path().join("history");
        let boundary = history.join("100-main-id");
        fs::create_dir_all(&boundary).unwrap();
        fs::write(boundary.join("cct.bamlcct"), [1_u8; 17]).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(temp.path(), history.join("escape")).unwrap();

        let candidates = collect_history_candidates(&history).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, boundary);
        assert_eq!(candidates[0].created_ms, 100);
        assert_eq!(candidates[0].bytes, 17);
    }

    #[test]
    fn oversized_history_preserves_newest_floor() {
        let candidates = (0..3)
            .map(|created_ms| RetentionCandidate {
                path: PathBuf::from(format!("/project/.baml/history/{created_ms}")),
                created_ms,
                bytes: 10,
            })
            .collect::<Vec<_>>();
        let plan = plan_retention(
            &candidates,
            RetentionPolicy {
                max_age_ms: None,
                max_bytes: Some(1),
                newest_floor: 1,
            },
            3,
        );
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].created_ms, 0);
        assert_eq!(plan[1].created_ms, 1);
    }
}
