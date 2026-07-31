//! §9.6 runs list: a bamlmeta scan — O(#runs), ~200 B each, no segment
//! reads. Crashed = begin-without-complete + dead session heartbeat.

use std::path::{Path, PathBuf};

use bex_events::prof::cct::meta::{self, MetaRecord};

/// One row of the runs list (boundary-level; sessions surface only through
/// the `session_dir` join and liveness).
#[derive(Debug, Clone)]
pub struct RunRow {
    pub boundary_id: String,
    pub target: String,
    /// `cli` | `playground` | `sdk` | `test`.
    pub source: String,
    pub created_ms: u64,
    pub completed_ms: u64,
    /// `running` | `succeeded` | `failed` | `cancelled` | `crashed` | ...
    pub status: String,
    pub revision_id: String,
    pub project_id: String,
    /// Directory of the boundary (absolute).
    pub dir: PathBuf,
    /// Bound session dir name ("" until bound).
    pub session_dir: String,
    /// True when a sealed `cct.bamlcct` snapshot exists (self-contained).
    pub has_snapshot: bool,
}

/// A session row (§6.1): one engine's lifetime in one process.
#[derive(Debug, Clone)]
pub struct SessionRow {
    pub dir: PathBuf,
    pub engine_id: u64,
    pub pid: u32,
    pub started_epoch_ns: u64,
    pub revision_id: String,
    /// Last heartbeat wall clock (0 = none seen).
    pub last_heartbeat_ns: u64,
    /// SessionEnd seen — clean shutdown.
    pub ended: bool,
    /// Liveness verdict at scan time (see [`session_alive`]).
    pub alive: bool,
}

/// §6.4 crash detection: a session is alive if it ended cleanly never, and
/// its pid is alive, and its heartbeat is fresh enough (default 30 s).
const HEARTBEAT_FRESH_NS: u64 = 30_000_000_000;

fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // Signal 0: existence probe without side effects.
        #[expect(unsafe_code, reason = "libc kill(pid, 0) liveness probe")]
        // SAFETY: kill with signal 0 only error-checks the target pid.
        unsafe {
            libc_kill(pid) == 0
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

#[cfg(unix)]
#[expect(
    unsafe_code,
    reason = "minimal FFI shim; no libc dependency in this crate"
)]
unsafe fn libc_kill(pid: u32) -> i32 {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    // SAFETY: kill(pid, 0) probes existence; no state is modified.
    unsafe { kill(pid as i32, 0) }
}

/// Scan `<root>/sessions/*/session.bamlmeta`.
#[must_use]
pub fn list_sessions(root: &Path, now_epoch_ns: u64) -> Vec<SessionRow> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root.join("sessions")) else {
        return out;
    };
    for dir in entries.filter_map(Result::ok).map(|e| e.path()) {
        let Ok(bytes) = std::fs::read(dir.join("session.bamlmeta")) else {
            continue;
        };
        let Ok(contents) = meta::read_meta(&bytes) else {
            continue;
        };
        let mut row = SessionRow {
            dir: dir.clone(),
            engine_id: 0,
            pid: 0,
            started_epoch_ns: 0,
            revision_id: String::new(),
            last_heartbeat_ns: 0,
            ended: false,
            alive: false,
        };
        for record in &contents.records {
            match record {
                MetaRecord::SessionBegin {
                    engine_id,
                    pid,
                    started_epoch_ns,
                    revision_id,
                    ..
                } => {
                    row.engine_id = *engine_id;
                    row.pid = *pid;
                    row.started_epoch_ns = *started_epoch_ns;
                    row.revision_id.clone_from(revision_id);
                }
                MetaRecord::SessionHeartbeat { wall_epoch_ns } => {
                    row.last_heartbeat_ns = row.last_heartbeat_ns.max(*wall_epoch_ns);
                }
                MetaRecord::SessionEnd { .. } => row.ended = true,
                _ => {}
            }
        }
        row.alive = !row.ended
            && pid_alive(row.pid)
            && (row.last_heartbeat_ns == 0
                || now_epoch_ns.saturating_sub(row.last_heartbeat_ns) < HEARTBEAT_FRESH_NS);
        out.push(row);
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.started_epoch_ns));
    out
}

/// Scan `<root>/history/*/boundary.bamlmeta` into the §9.6 runs list.
/// `sessions` feeds the crashed verdict for begin-without-complete rows.
#[must_use]
pub fn list_runs(root: &Path, sessions: &[SessionRow]) -> Vec<RunRow> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root.join("history")) else {
        return out;
    };
    for dir in entries.filter_map(Result::ok).map(|e| e.path()) {
        if !dir.is_dir() || dir.file_name().is_some_and(|n| n == "_unbound") {
            continue;
        }
        let Ok(bytes) = std::fs::read(dir.join("boundary.bamlmeta")) else {
            continue;
        };
        let Ok(contents) = meta::read_meta(&bytes) else {
            continue;
        };
        let mut row = RunRow {
            boundary_id: String::new(),
            target: String::new(),
            source: String::new(),
            created_ms: 0,
            completed_ms: 0,
            status: "running".to_string(),
            revision_id: String::new(),
            project_id: String::new(),
            dir: dir.clone(),
            session_dir: String::new(),
            has_snapshot: dir.join("cct.bamlcct").exists(),
        };
        let mut completed = false;
        for record in &contents.records {
            match record {
                MetaRecord::BoundaryBegin {
                    boundary_id,
                    target,
                    source,
                    created_ms,
                    project_id,
                    revision_id,
                    ..
                } => {
                    row.boundary_id.clone_from(boundary_id);
                    row.target.clone_from(target);
                    row.source.clone_from(source);
                    row.created_ms = *created_ms;
                    row.project_id.clone_from(project_id);
                    row.revision_id.clone_from(revision_id);
                }
                MetaRecord::BoundaryBound { session_dir, .. } => {
                    row.session_dir.clone_from(session_dir);
                }
                MetaRecord::BoundaryComplete {
                    status,
                    completed_ms,
                    ..
                } => {
                    row.status.clone_from(status);
                    row.completed_ms = *completed_ms;
                    completed = true;
                }
                _ => {}
            }
        }
        if !completed {
            // Begin-without-complete: crashed iff the bound session (or,
            // unbound, every session) is dead.
            let session_alive = if row.session_dir.is_empty() {
                sessions.iter().any(|s| s.alive)
            } else {
                sessions
                    .iter()
                    .filter(|s| {
                        s.dir
                            .file_name()
                            .is_some_and(|n| n.to_string_lossy() == row.session_dir)
                    })
                    .any(|s| s.alive)
            };
            row.status = if session_alive { "running" } else { "crashed" }.to_string();
        }
        out.push(row);
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.created_ms));
    out
}
