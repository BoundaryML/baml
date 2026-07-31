use std::sync::{Arc, Mutex};

#[cfg(feature = "native")]
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    BcctScan, ByteBudgetCache, ByteSource, FileId, FoldedCct, LeftHeavyRequest, LeftHeavyResponse,
    QueryError, QueryPoll, TimelineResponse, Viewport, fold_bcct, left_heavy, scan_bcct, timeline,
};

#[cfg(feature = "native")]
use crate::{FileSource, RunMeta};

pub const NATIVE_CACHE_BYTES: usize = 256 * 1024 * 1024;
pub const WASM_CACHE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FoldCacheKey {
    files: Vec<(FileId, u64, u64)>,
    partition_id: Option<u32>,
}

impl QueryEngine<crate::LiveMirrorSource> {
    pub fn timeline_live(
        &self,
        files: &[FileId],
        partition_id: Option<u32>,
        viewport: Viewport,
    ) -> Result<QueryPoll<TimelineResponse>, QueryError> {
        match self.open_run(files, partition_id)? {
            QueryPoll::Ready(cct) => {
                let mut overlay = crate::TimelineOverlay::default();
                for file in files {
                    if let Some(mut file_overlay) = self.source.timeline(*file) {
                        overlay.exact_calls.append(&mut file_overlay.exact_calls);
                        overlay.evicted_recent_calls = overlay
                            .evicted_recent_calls
                            .saturating_add(file_overlay.evicted_recent_calls);
                    }
                }
                Ok(QueryPoll::Ready(crate::timeline_with_overlay(
                    &cct, viewport, &overlay,
                )?))
            }
            QueryPoll::NeedData { ranges } => Ok(QueryPoll::NeedData { ranges }),
        }
    }
}

pub struct QueryEngine<S> {
    source: S,
    folds: Mutex<ByteBudgetCache<FoldCacheKey, Arc<FoldedCct>>>,
}

impl<S: ByteSource> QueryEngine<S> {
    #[must_use]
    pub fn new(source: S) -> Self {
        Self::with_cache_budget(source, default_cache_bytes())
    }

    #[must_use]
    pub fn with_cache_budget(source: S, max_cache_bytes: usize) -> Self {
        Self {
            source,
            folds: Mutex::new(ByteBudgetCache::new(max_cache_bytes)),
        }
    }

    #[must_use]
    pub fn source(&self) -> &S {
        &self.source
    }

    #[must_use]
    pub fn cache_retained_bytes(&self) -> usize {
        self.folds
            .lock()
            .expect("query fold cache mutex poisoned")
            .retained_bytes()
    }

    #[must_use]
    pub fn cache_max_bytes(&self) -> usize {
        self.folds
            .lock()
            .expect("query fold cache mutex poisoned")
            .max_bytes()
    }

    pub fn set_cache_max_bytes(&self, max_bytes: usize) {
        self.folds
            .lock()
            .expect("query fold cache mutex poisoned")
            .set_max_bytes(max_bytes);
    }

    pub fn open_run(
        &self,
        files: &[FileId],
        partition_id: Option<u32>,
    ) -> Result<QueryPoll<Arc<FoldedCct>>, QueryError> {
        if files.is_empty() {
            return Err(QueryError::invalid_request(
                "open_run requires at least one BCCT file",
            ));
        }
        let key = FoldCacheKey {
            files: files
                .iter()
                .map(|file| {
                    (
                        *file,
                        self.source.generation(*file),
                        self.source.committed_len(*file),
                    )
                })
                .collect(),
            partition_id,
        };
        if let Some(cached) = self
            .folds
            .lock()
            .expect("query fold cache mutex poisoned")
            .get(&key)
            .cloned()
        {
            return Ok(QueryPoll::Ready(cached));
        }
        let mut scans = Vec::<BcctScan>::with_capacity(files.len());
        let mut missing = Vec::new();
        for file in files {
            match scan_bcct(&self.source, *file)? {
                QueryPoll::Ready(scan) => scans.push(scan),
                QueryPoll::NeedData { ranges } => missing.extend(ranges),
            }
        }
        if !missing.is_empty() {
            missing.sort_by_key(|range| (range.file, range.start, range.end));
            missing.dedup();
            return Ok(QueryPoll::NeedData { ranges: missing });
        }
        let folded = Arc::new(fold_bcct(&scans, partition_id)?);
        let bytes = folded.estimated_bytes();
        self.folds
            .lock()
            .expect("query fold cache mutex poisoned")
            .insert(key, Arc::clone(&folded), bytes);
        Ok(QueryPoll::Ready(folded))
    }

    pub fn left_heavy(
        &self,
        files: &[FileId],
        partition_id: Option<u32>,
        request: LeftHeavyRequest,
    ) -> Result<QueryPoll<LeftHeavyResponse>, QueryError> {
        match self.open_run(files, partition_id)? {
            QueryPoll::Ready(cct) => Ok(QueryPoll::Ready(left_heavy(&cct, request)?)),
            QueryPoll::NeedData { ranges } => Ok(QueryPoll::NeedData { ranges }),
        }
    }

    pub fn timeline(
        &self,
        files: &[FileId],
        partition_id: Option<u32>,
        viewport: Viewport,
    ) -> Result<QueryPoll<TimelineResponse>, QueryError> {
        match self.open_run(files, partition_id)? {
            QueryPoll::Ready(cct) => Ok(QueryPoll::Ready(timeline(&cct, viewport)?)),
            QueryPoll::NeedData { ranges } => Ok(QueryPoll::NeedData { ranges }),
        }
    }
}

#[cfg(feature = "native")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeRun {
    pub files: Vec<FileId>,
    pub paths: Vec<PathBuf>,
    pub partition_id: Option<u32>,
}

#[cfg(feature = "native")]
impl QueryEngine<FileSource> {
    pub fn register_native_run(&self, meta: &RunMeta) -> Result<NativeRun, QueryError> {
        let paths = resolve_run_paths(meta)?;
        let mut files = Vec::with_capacity(paths.len());
        for path in &paths {
            let file = FileId(stable_path_id(path));
            if let Some(existing) = self.source.path(file) {
                if existing != *path {
                    return Err(QueryError::invalid_data(
                        "native FileId collision between observability paths",
                    ));
                }
            } else {
                self.source.open(file, path)?;
            }
            files.push(file);
        }
        Ok(NativeRun {
            files,
            paths,
            partition_id: meta.summary.partition_id,
        })
    }

    pub fn refresh_native_run(&self, run: &NativeRun) -> Result<(), QueryError> {
        for file in &run.files {
            self.source.refresh(*file)?;
        }
        Ok(())
    }

    pub fn open_native_run(&self, meta: &RunMeta) -> Result<QueryPoll<Arc<FoldedCct>>, QueryError> {
        let run = self.register_native_run(meta)?;
        self.open_run(&run.files, run.partition_id)
    }

    pub fn left_heavy_native(
        &self,
        meta: &RunMeta,
        request: LeftHeavyRequest,
    ) -> Result<QueryPoll<LeftHeavyResponse>, QueryError> {
        let run = self.register_native_run(meta)?;
        self.left_heavy(&run.files, run.partition_id, request)
    }

    pub fn timeline_native(
        &self,
        meta: &RunMeta,
        viewport: Viewport,
    ) -> Result<QueryPoll<TimelineResponse>, QueryError> {
        let run = self.register_native_run(meta)?;
        self.timeline(&run.files, run.partition_id, viewport)
    }
}

#[cfg(feature = "native")]
fn resolve_run_paths(meta: &RunMeta) -> Result<Vec<PathBuf>, QueryError> {
    let snapshot = meta.boundary_dir.join("cct.bamlcct");
    if snapshot.is_file() {
        return Ok(vec![snapshot]);
    }
    let session_meta = meta.session.as_ref().ok_or_else(|| {
        QueryError::NotFound(
            "run has neither a boundary snapshot nor a resolvable bound session".to_owned(),
        )
    })?;
    let session_dir = session_meta
        .path
        .parent()
        .ok_or_else(|| QueryError::invalid_data("session metadata path has no parent"))?;
    let cct_dir = session_dir.join("cct");
    let first_sequence = meta.bound.as_ref().map_or(1, |bound| bound.first_seg_seq);
    let last_sequence = meta.complete.as_ref().map(|complete| complete.last_seg_seq);
    let mut paths = fs::read_dir(&cct_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter_map(|path| segment_sequence(&path).map(|sequence| (sequence, path)))
        .filter(|(sequence, _)| {
            *sequence >= first_sequence
                && last_sequence.is_none_or(|last_sequence| *sequence <= last_sequence)
        })
        .collect::<Vec<_>>();
    paths.sort_by_key(|(sequence, _)| *sequence);
    if paths.is_empty() {
        return Err(QueryError::NotFound(format!(
            "no BCCT segments found in {} for sequence {first_sequence}..={}",
            cct_dir.display(),
            last_sequence.map_or_else(|| "active".to_owned(), |value| value.to_string())
        )));
    }
    Ok(paths.into_iter().map(|(_, path)| path).collect())
}

#[cfg(feature = "native")]
fn segment_sequence(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    name.strip_prefix("seg-")?
        .strip_suffix(".bamlseg")?
        .parse()
        .ok()
}

#[cfg(feature = "native")]
fn stable_path_id(path: &Path) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in path.as_os_str().as_encoded_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

const fn default_cache_bytes() -> usize {
    if cfg!(feature = "wasm") {
        WASM_CACHE_BYTES
    } else {
        NATIVE_CACHE_BYTES
    }
}
