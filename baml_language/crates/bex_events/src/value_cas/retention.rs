use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tombstone {
    pub deleted_at_ms: u64,
    pub kind: String,
    pub path: PathBuf,
    pub reason: String,
    pub bytes: Option<u64>,
}

#[derive(Debug)]
pub struct RetentionLog {
    file: File,
}

impl RetentionLog {
    pub fn open(project_baml_dir: impl AsRef<Path>) -> io::Result<Self> {
        let path = project_baml_dir.as_ref().join("retention.log");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().append(true).create(true).open(path)?;
        Ok(Self { file })
    }

    /// Append one JSONL audit record. Call this only after the named material
    /// has actually been removed/replaced.
    pub fn append(&mut self, tombstone: &Tombstone) -> io::Result<()> {
        let record = serde_json::json!({
            "deleted_at_ms": tombstone.deleted_at_ms,
            "kind": tombstone.kind,
            "path": tombstone.path.to_string_lossy(),
            "reason": tombstone.reason,
            "bytes": tombstone.bytes,
        });
        serde_json::to_writer(&mut self.file, &record)?;
        self.file.write_all(b"\n")?;
        self.file.flush()
    }

    pub fn sync_data(&mut self) -> io::Result<()> {
        self.file.flush()?;
        self.file.sync_data()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetentionCandidate {
    pub path: PathBuf,
    pub created_ms: u64,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub max_age_ms: Option<u64>,
    pub max_bytes: Option<u64>,
    pub newest_floor: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RetentionExecution {
    pub planned: usize,
    pub deleted: usize,
    pub deleted_bytes: u64,
}

/// Select whole retention units, never individual value chunks. Candidates
/// protected by `newest_floor` survive both age and size pressure.
#[must_use]
pub fn plan_retention(
    candidates: &[RetentionCandidate],
    policy: RetentionPolicy,
    now_ms: u64,
) -> Vec<RetentionCandidate> {
    let mut newest = candidates.iter().collect::<Vec<_>>();
    newest.sort_by(|left, right| {
        right
            .created_ms
            .cmp(&left.created_ms)
            .then_with(|| right.path.cmp(&left.path))
    });
    let protected = newest
        .iter()
        .take(policy.newest_floor)
        .map(|candidate| candidate.path.clone())
        .collect::<BTreeSet<_>>();
    let mut selected = BTreeSet::<PathBuf>::new();

    if let Some(max_age_ms) = policy.max_age_ms {
        let cutoff = now_ms.saturating_sub(max_age_ms);
        for candidate in candidates {
            if candidate.created_ms < cutoff && !protected.contains(&candidate.path) {
                selected.insert(candidate.path.clone());
            }
        }
    }

    if let Some(max_bytes) = policy.max_bytes {
        let mut retained_bytes = candidates
            .iter()
            .filter(|candidate| !selected.contains(&candidate.path))
            .map(|candidate| candidate.bytes)
            .fold(0_u64, u64::saturating_add);
        let mut oldest = candidates.iter().collect::<Vec<_>>();
        oldest.sort_by(|left, right| {
            left.created_ms
                .cmp(&right.created_ms)
                .then_with(|| left.path.cmp(&right.path))
        });
        for candidate in oldest {
            if retained_bytes <= max_bytes {
                break;
            }
            if protected.contains(&candidate.path) || selected.contains(&candidate.path) {
                continue;
            }
            selected.insert(candidate.path.clone());
            retained_bytes = retained_bytes.saturating_sub(candidate.bytes);
        }
    }

    let mut planned = candidates
        .iter()
        .filter(|candidate| selected.contains(&candidate.path))
        .cloned()
        .collect::<Vec<_>>();
    planned.sort_by(|left, right| {
        left.created_ms
            .cmp(&right.created_ms)
            .then_with(|| left.path.cmp(&right.path))
    });
    planned
}

/// Apply a whole-unit retention plan with lexical scope and symlink guards.
///
/// Callers are responsible for completing higher-level prerequisites such as
/// crashed-boundary snapshot materialization before placing a session in this
/// plan. The helper never follows symlinks and never deletes the project root.
pub fn execute_retention_plan(
    project_baml_dir: impl AsRef<Path>,
    planned: &[RetentionCandidate],
    now_ms: u64,
    dry_run: bool,
    kind: &str,
    reason: &str,
) -> io::Result<RetentionExecution> {
    let project_baml_dir = project_baml_dir.as_ref();
    let mut execution = RetentionExecution {
        planned: planned.len(),
        ..RetentionExecution::default()
    };
    for candidate in planned {
        let relative = candidate.path.strip_prefix(project_baml_dir).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "retention candidate {} is outside the project observability root",
                    candidate.path.display()
                ),
            )
        })?;
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "retention candidate {} is outside the project observability root",
                    candidate.path.display()
                ),
            ));
        }
        let metadata = fs::symlink_metadata(&candidate.path)?;
        if metadata.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "retention refuses symlink candidate {}",
                    candidate.path.display()
                ),
            ));
        }
    }
    if dry_run {
        return Ok(execution);
    }

    let mut retention_log = RetentionLog::open(project_baml_dir)?;
    for candidate in planned {
        let metadata = fs::symlink_metadata(&candidate.path)?;
        if metadata.is_dir() {
            fs::remove_dir_all(&candidate.path)?;
        } else if metadata.is_file() {
            fs::remove_file(&candidate.path)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "retention candidate {} is neither a file nor directory",
                    candidate.path.display()
                ),
            ));
        }
        execution.deleted = execution.deleted.saturating_add(1);
        execution.deleted_bytes = execution.deleted_bytes.saturating_add(candidate.bytes);
        retention_log.append(&Tombstone {
            deleted_at_ms: now_ms,
            kind: kind.to_string(),
            path: candidate.path.clone(),
            reason: reason.to_string(),
            bytes: Some(candidate.bytes),
        })?;
    }
    retention_log.sync_data()?;
    Ok(execution)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        RetentionCandidate, RetentionLog, RetentionPolicy, Tombstone, execute_retention_plan,
        plan_retention,
    };

    #[test]
    fn newest_floor_survives_age_and_size_pressure() {
        let candidates = (0..4)
            .map(|index| RetentionCandidate {
                path: format!("boundary-{index}").into(),
                created_ms: index * 100,
                bytes: 10,
            })
            .collect::<Vec<_>>();
        let planned = plan_retention(
            &candidates,
            RetentionPolicy {
                max_age_ms: Some(150),
                max_bytes: Some(10),
                newest_floor: 2,
            },
            400,
        );
        assert_eq!(
            planned
                .iter()
                .map(|candidate| candidate.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["boundary-0", "boundary-1"]
        );
    }

    #[test]
    fn tombstone_log_is_append_only_jsonl() {
        let root = std::env::temp_dir().join(format!(
            "baml-retention-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let mut log = RetentionLog::open(&root).unwrap();
        log.append(&Tombstone {
            deleted_at_ms: 12,
            kind: "value_pack".to_string(),
            path: root.join("pack-1.bamlpack"),
            reason: "gc_fully_dead".to_string(),
            bytes: Some(99),
        })
        .unwrap();
        log.sync_data().unwrap();
        let line = fs::read_to_string(root.join("retention.log")).unwrap();
        let value: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(value["reason"], "gc_fully_dead");
        assert_eq!(value["bytes"], 99);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn executable_retention_is_scoped_dry_runnable_and_tombstoned() {
        let root = std::env::temp_dir().join(format!(
            "baml-retention-exec-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let candidate_path = root.join("history/old-boundary");
        fs::create_dir_all(&candidate_path).unwrap();
        fs::write(candidate_path.join("data"), [1, 2, 3]).unwrap();
        let candidate = RetentionCandidate {
            path: candidate_path.clone(),
            created_ms: 1,
            bytes: 3,
        };
        let dry = execute_retention_plan(
            &root,
            std::slice::from_ref(&candidate),
            10,
            true,
            "boundary",
            "age",
        )
        .unwrap();
        assert_eq!(dry.planned, 1);
        assert!(candidate_path.exists());
        let applied =
            execute_retention_plan(&root, &[candidate], 10, false, "boundary", "age").unwrap();
        assert_eq!(applied.deleted, 1);
        assert!(!candidate_path.exists());
        assert!(
            fs::read_to_string(root.join("retention.log"))
                .unwrap()
                .contains("\"reason\":\"age\"")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
