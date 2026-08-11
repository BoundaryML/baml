//! The bound local query universe (D10): one fixed snapshot of the
//! `.baml` tree.
//!
//! Binding records every relevant file WITH its length at bind time;
//! every later read is truncated to the bound length, so a live session
//! segment or value stream growing underneath an executing query stays
//! invisible (later commits are not part of this snapshot). Sealed
//! artifacts are immutable, so the bound length is simply their size.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use baml_query::error::{QueryError, QueryErrorCode};
use baml_query::scope::Snapshot;
use bex_query::runs::{RunRow, SessionRow, list_runs, list_sessions};

/// One bound run with its bound artifact set.
pub struct BoundRun {
    pub row: RunRow,
    /// The sealed `cct.bamlcct`, when the boundary completed.
    pub snapshot_file: Option<BoundFile>,
    /// Value segments (`thread-*/value-*.bamlvalue`).
    pub value_files: Vec<BoundFile>,
}

/// One bound session with its live CCT segments in seq order.
pub struct BoundSession {
    pub row: SessionRow,
    pub cct_files: Vec<BoundFile>,
    /// Flight dumps under `flight/` (path + bound length).
    pub flight_files: Vec<BoundFile>,
}

/// One file frozen into the snapshot: reads never pass `len`.
#[derive(Debug, Clone)]
pub struct BoundFile {
    pub path: PathBuf,
    pub len: u64,
}

impl BoundFile {
    fn bind(path: PathBuf) -> Option<BoundFile> {
        let len = std::fs::metadata(&path).ok()?.len();
        Some(BoundFile { path, len })
    }

    /// Read the bound prefix.
    pub fn read(&self) -> std::io::Result<Vec<u8>> {
        let mut bytes = std::fs::read(&self.path)?;
        bytes.truncate(usize::try_from(self.len).unwrap_or(usize::MAX));
        Ok(bytes)
    }
}

/// The bound universe.
pub struct LocalUniverse {
    baml_dir: PathBuf,
    pub runs: Vec<BoundRun>,
    pub sessions: Vec<BoundSession>,
    /// Revision wire id → dictionary bytes.
    pub dicts: BTreeMap<String, Vec<u8>>,
    /// Wall clock at bind (ns since epoch): "so far" durations of running
    /// runs are relative to the snapshot, not to when a row is read.
    pub bound_at_ns: u64,
    generation: String,
}

impl LocalUniverse {
    /// Bind `.baml` into a fixed universe.
    pub fn bind(baml_dir: &Path) -> Result<LocalUniverse, QueryError> {
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        let sessions_rows = list_sessions(baml_dir, now_ns);
        let runs_rows = list_runs(baml_dir, &sessions_rows);

        let mut runs = Vec::with_capacity(runs_rows.len());
        for row in runs_rows {
            let snapshot_file = BoundFile::bind(row.dir.join("cct.bamlcct"));
            let mut value_files = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&row.dir) {
                for entry in entries.filter_map(Result::ok) {
                    let sub = entry.path();
                    if sub.is_dir()
                        && let Ok(inner) = std::fs::read_dir(&sub)
                    {
                        for file in inner.filter_map(Result::ok).map(|e| e.path()) {
                            if file.extension().is_some_and(|e| e == "bamlvalue")
                                && let Some(bound) = BoundFile::bind(file)
                            {
                                value_files.push(bound);
                            }
                        }
                    }
                }
            }
            value_files.sort_by(|a, b| a.path.cmp(&b.path));
            runs.push(BoundRun {
                row,
                snapshot_file,
                value_files,
            });
        }

        let mut sessions = Vec::with_capacity(sessions_rows.len());
        for row in sessions_rows {
            let mut cct_files = Vec::new();
            if let Ok(entries) = std::fs::read_dir(row.dir.join("cct")) {
                for file in entries.filter_map(Result::ok).map(|e| e.path()) {
                    if file.extension().is_some_and(|e| e == "bamlseg")
                        && let Some(bound) = BoundFile::bind(file)
                    {
                        cct_files.push(bound);
                    }
                }
            }
            cct_files.sort_by(|a, b| a.path.cmp(&b.path));
            let mut flight_files = Vec::new();
            if let Ok(entries) = std::fs::read_dir(row.dir.join("flight")) {
                for file in entries.filter_map(Result::ok).map(|e| e.path()) {
                    if file.extension().is_some_and(|e| e == "bamlprof")
                        && let Some(bound) = BoundFile::bind(file)
                    {
                        flight_files.push(bound);
                    }
                }
            }
            flight_files.sort_by(|a, b| a.path.cmp(&b.path));
            sessions.push(BoundSession {
                row,
                cct_files,
                flight_files,
            });
        }

        let mut dicts = BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(baml_dir.join("dict")) {
            for file in entries.filter_map(Result::ok).map(|e| e.path()) {
                if file.extension().is_some_and(|e| e == "bamldict")
                    && let Some(stem) = file.file_stem().and_then(|s| s.to_str())
                    && let Ok(bytes) = std::fs::read(&file)
                {
                    dicts.insert(stem.to_string(), bytes);
                }
            }
        }

        // Generation: deterministic identity of the bound file universe.
        let mut hasher = blake3_lite::Hasher::new();
        for run in &runs {
            hasher.update(run.row.dir.to_string_lossy().as_bytes());
            if let Some(f) = &run.snapshot_file {
                hasher.update(&f.len.to_le_bytes());
            }
            for f in &run.value_files {
                hasher.update(f.path.to_string_lossy().as_bytes());
                hasher.update(&f.len.to_le_bytes());
            }
        }
        for session in &sessions {
            for f in session.cct_files.iter().chain(&session.flight_files) {
                hasher.update(f.path.to_string_lossy().as_bytes());
                hasher.update(&f.len.to_le_bytes());
            }
        }
        for name in dicts.keys() {
            hasher.update(name.as_bytes());
        }
        let generation = hasher.finish_hex();

        if !baml_dir.exists() {
            return Err(QueryError::new(
                QueryErrorCode::DependencyUnavailable,
                format!("no .baml directory at {}", baml_dir.display()),
            ));
        }
        Ok(LocalUniverse {
            baml_dir: baml_dir.to_path_buf(),
            runs,
            sessions,
            dicts,
            bound_at_ns: now_ns,
            generation,
        })
    }

    #[must_use]
    pub fn baml_dir(&self) -> &Path {
        &self.baml_dir
    }

    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            catalog_version: baml_query::catalog::CATALOG_V1.to_string(),
            generation: self.generation.clone(),
            projected_through: None,
        }
    }

    /// The bound session for a run's `session_dir` (final path component
    /// match, same rule the readers use).
    #[must_use]
    pub fn session_of(&self, session_dir: &str) -> Option<&BoundSession> {
        let name = Path::new(session_dir).file_name()?.to_str()?;
        self.sessions
            .iter()
            .find(|s| s.row.dir.file_name().is_some_and(|n| n == name))
    }
}

/// Minimal streaming BLAKE3 wrapper (bex_events depends on blake3; going
/// through canon keeps this crate free of a direct hash dependency).
mod blake3_lite {
    pub struct Hasher(Vec<u8>);

    impl Hasher {
        pub fn new() -> Hasher {
            Hasher(Vec::new())
        }
        pub fn update(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
        pub fn finish_hex(&self) -> String {
            // The canonical chunk CID is a domain-separated BLAKE3 —
            // deterministic and collision-resistant, which is all a
            // generation identity needs.
            let cid = bex_events::store::canon::cid_for_chunk(&self.0);
            bex_events::store::canon::cid_wire(&cid)
        }
    }
}
