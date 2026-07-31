//! Durable boundary lifecycle owned jointly by hosts and the native profile
//! consumer.
//!
//! Hosts author the `begin` milestone before execution. The consumer authors
//! `bound` and `complete`, because it alone can map the root thread to a CCT
//! partition and name the exact session segment range.

use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    ids::{BoundaryId, ThreadRef},
    prof::{
        ProfConfig, ProfilePipeline,
        storage::{
            BoundaryBeginMeta, TypedBoundaryMeta, append_meta_d2, encode_typed_boundary_meta,
        },
    },
};

/// Host-owned fields in the first `boundary.bamlmeta` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundaryBegin {
    pub boundary_id: BoundaryId,
    pub target: String,
    pub source: String,
    pub project_id: String,
    pub revision_id: [u8; 32],
    pub capture_defaults: u32,
    pub project_root: PathBuf,
    /// `None` uses the current wall clock.
    pub created_ms: Option<u64>,
}

impl BoundaryBegin {
    #[must_use]
    pub fn new(
        boundary_id: BoundaryId,
        target: impl Into<String>,
        revision_id: [u8; 32],
        project_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            boundary_id,
            target: target.into(),
            source: "sdk".to_owned(),
            project_id: String::new(),
            revision_id,
            capture_defaults: 0,
            project_root: project_root.into(),
            created_ms: None,
        }
    }
}

/// Consumer-authored result of `ControlMsg::BindBoundary`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundaryBinding {
    pub session_dir: PathBuf,
    pub first_seg_seq: u32,
    pub partition_id: u32,
    pub boundary_local_id: u32,
}

/// Completion information supplied by the host after the root call settles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundaryCompletion {
    pub status: String,
    pub diagnostics: Vec<String>,
    pub dump_refs: Vec<String>,
    /// Optional host trigger (`manual:<label>` or `latency:<elapsed>=<limit>`).
    /// Error completions arm the error trigger automatically.
    pub trigger: Option<String>,
}

impl BoundaryCompletion {
    #[must_use]
    pub fn new(status: impl Into<String>) -> Self {
        Self {
            status: status.into(),
            diagnostics: Vec::new(),
            dump_refs: Vec::new(),
            trigger: None,
        }
    }
}

/// A begun boundary. An inactive handle is returned when durable history is
/// opted out or the CCT pipeline/layout is not selected; its methods are safe
/// no-ops, preserving `BAML_HISTORY=0` session-only behavior.
#[derive(Clone, Debug)]
pub struct BoundaryLifecycle {
    boundary_id: BoundaryId,
    active: bool,
}

impl BoundaryLifecycle {
    pub fn begin(request: BoundaryBegin) -> io::Result<Self> {
        let boundary_id = request.boundary_id;
        let config = ProfConfig::global();
        let active = history_enabled()
            && config.is_enabled()
            && config.obs_layout.writes_v2()
            && matches!(
                config.pipeline,
                ProfilePipeline::Dual | ProfilePipeline::Cct
            );
        if active {
            register_begin(request)?;
        }
        Ok(Self {
            boundary_id,
            active,
        })
    }

    #[must_use]
    pub fn boundary_id(&self) -> BoundaryId {
        self.boundary_id
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Durable location and wall start for value/flight services that share
    /// the boundary completion barrier.
    #[must_use]
    pub fn storage_registration(&self) -> Option<(PathBuf, u64)> {
        self.active
            .then(|| registration(self.boundary_id))
            .flatten()
            .map(|registration| (registration.boundary_dir, registration.created_ms))
    }

    /// Waits until the consumer has drained the root `StartThread`, then
    /// durably records the structural partition binding.
    pub fn bind(
        &self,
        root_thread: ThreadRef,
        timeout: Duration,
    ) -> io::Result<Option<BoundaryBinding>> {
        if !self.active {
            return Ok(None);
        }
        #[cfg(all(not(target_arch = "wasm32"), not(baml_loom)))]
        {
            super::consumer::bind_boundary(self.boundary_id, root_thread, timeout).map(Some)
        }
        #[cfg(any(target_arch = "wasm32", baml_loom))]
        {
            let _ = (root_thread, timeout);
            Ok(None)
        }
    }

    /// Completion is a durability barrier: final ring drain, final CCT
    /// window, session sync, sealed boundary snapshot, then BMET `complete`.
    pub fn complete(&self, completion: BoundaryCompletion, timeout: Duration) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        #[cfg(all(not(target_arch = "wasm32"), not(baml_loom)))]
        {
            super::consumer::complete_boundary(self.boundary_id, completion, timeout)
        }
        #[cfg(any(target_arch = "wasm32", baml_loom))]
        {
            let _ = (completion, timeout);
            Ok(())
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BoundaryRegistration {
    pub boundary_dir: PathBuf,
    pub created_ms: u64,
}

static REGISTRATIONS: OnceLock<Mutex<HashMap<BoundaryId, BoundaryRegistration>>> = OnceLock::new();

pub(crate) fn registration(boundary_id: BoundaryId) -> Option<BoundaryRegistration> {
    registrations()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&boundary_id)
        .cloned()
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn finish_registration(boundary_id: BoundaryId) {
    registrations()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&boundary_id);
}

pub(crate) fn register_begin(request: BoundaryBegin) -> io::Result<BoundaryLifecycle> {
    let mut registrations = registrations()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if registrations.contains_key(&request.boundary_id) {
        return Ok(BoundaryLifecycle {
            boundary_id: request.boundary_id,
            active: true,
        });
    }

    let created_ms = request.created_ms.unwrap_or_else(now_ms);
    let boundary_dir = boundary_directory(
        &request.project_root,
        created_ms,
        &request.target,
        request.boundary_id,
    );
    let begin = TypedBoundaryMeta::Begin(BoundaryBeginMeta {
        boundary_id: request.boundary_id.as_bytes(),
        target: request.target,
        source: request.source,
        created_ms,
        project_id: request.project_id,
        revision_id: request.revision_id,
        capture_defaults: request.capture_defaults,
    });
    append_typed_boundary_d2(&boundary_dir.join("boundary.bamlmeta"), &begin)?;
    registrations.insert(
        request.boundary_id,
        BoundaryRegistration {
            boundary_dir,
            created_ms,
        },
    );
    Ok(BoundaryLifecycle {
        boundary_id: request.boundary_id,
        active: true,
    })
}

fn registrations() -> &'static Mutex<HashMap<BoundaryId, BoundaryRegistration>> {
    REGISTRATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn append_typed_boundary_d2(path: &Path, value: &TypedBoundaryMeta) -> io::Result<u64> {
    let (kind, payload) = encode_typed_boundary_meta(value)?;
    append_meta_d2(path, kind, &payload)
}

#[must_use]
pub fn history_enabled() -> bool {
    !std::env::var("BAML_HISTORY").is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "no"
        )
    })
}

fn boundary_directory(
    project_root: &Path,
    created_ms: u64,
    target: &str,
    boundary_id: BoundaryId,
) -> PathBuf {
    let target = target
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let target = target.trim_matches('-');
    let target = if target.is_empty() { "run" } else { target };
    project_root.join(".baml").join("history").join(format!(
        "{created_ms}-{target}-{}",
        boundary_id.to_wire_string()
    ))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_directory_is_stable_and_suffix_resolvable() {
        let boundary = BoundaryId::from_bytes([7; 16]);
        let path = boundary_directory(Path::new("/project"), 42, "pkg.main()", boundary);
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            format!("42-pkg-main-{}", boundary.to_wire_string())
        );
    }
}
