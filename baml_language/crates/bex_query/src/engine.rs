//! §9.2 `ObserveEngine`: the host-facing API. Named methods are the
//! recognized-query fast paths the UI uses; every one returns a BQF1
//! frame. Native hosts hand it a `.baml` root; folds cache under one byte
//! budget (§9.2: byte-budgeted, not entry-capped).

use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;

use crate::bqf1::{self, Col, FrameKind};
use crate::cct::{self, CctFold};
use crate::runs;
use crate::source::{Poll, SliceSource};

/// Bounded-size request contract (§9.2).
pub const MAX_PIXEL_WIDTH: u32 = 8192;
pub const MAX_LANES: u32 = 256;
pub const DEFAULT_MAX_BYTES: usize = 4 << 20;
pub const HARD_MAX_BYTES: usize = 16 << 20;
/// Native decoded-cache byte budget (§9.2).
pub const NATIVE_CACHE_BUDGET: usize = 256 << 20;

/// One open run's fold + identity, cached.
struct OpenRun {
    fold: CctFold,
    /// function id → fqn (from the revision dictionary; empty if missing).
    names: FxHashMap<u32, String>,
    /// Data epoch the fold was computed at (source generations summed).
    epoch: u64,
    approx_bytes: usize,
    last_used: u64,
}

/// Host-facing engine over one `.baml` root.
pub struct ObserveEngine {
    root: PathBuf,
    open: FxHashMap<String, OpenRun>,
    clock: u64,
    cache_budget: usize,
}

impl ObserveEngine {
    #[must_use]
    pub fn new(root: PathBuf) -> ObserveEngine {
        ObserveEngine {
            root,
            open: FxHashMap::default(),
            clock: 0,
            cache_budget: NATIVE_CACHE_BUDGET,
        }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// §9.6 runs list — one BQF1 frame, no segment reads.
    #[must_use]
    pub fn runs_frame(&self, request_id: u64, now_epoch_ns: u64) -> Vec<u8> {
        let sessions = runs::list_sessions(&self.root, now_epoch_ns);
        let rows = runs::list_runs(&self.root, &sessions);
        let key = |r: &runs::RunRow| -> String {
            r.dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let run_keys: Vec<String> = rows.iter().map(key).collect();
        let boundary: Vec<String> = rows.iter().map(|r| r.boundary_id.clone()).collect();
        let target: Vec<String> = rows.iter().map(|r| r.target.clone()).collect();
        let source_col: Vec<String> = rows.iter().map(|r| r.source.clone()).collect();
        let status: Vec<String> = rows.iter().map(|r| r.status.clone()).collect();
        let revision: Vec<String> = rows.iter().map(|r| r.revision_id.clone()).collect();
        let created: Vec<u64> = rows.iter().map(|r| r.created_ms).collect();
        let completed: Vec<u64> = rows.iter().map(|r| r.completed_ms).collect();
        let snapshot: Vec<u32> = rows.iter().map(|r| u32::from(r.has_snapshot)).collect();
        bqf1::encode_frame(
            FrameKind::RunsList,
            0,
            request_id,
            0,
            &[
                Col::Str(&run_keys),
                Col::Str(&boundary),
                Col::Str(&target),
                Col::Str(&source_col),
                Col::Str(&status),
                Col::Str(&revision),
                Col::U64(&created),
                Col::U64(&completed),
                Col::U32(&snapshot),
            ],
        )
    }

    /// Open (or re-validate) one run by its `history/` dir name, folding
    /// its CCT. Prefers the sealed snapshot; falls back to session
    /// segments for live/crashed runs. Session dir names (`sessions/...`
    /// join keys) open the whole session instead.
    pub fn open_run(&mut self, run_key: &str) -> Result<(), String> {
        self.clock += 1;
        let (files, names, epoch) = self.load_run_bytes(run_key)?;
        let mut source = SliceSource::new();
        let ids: Vec<_> = files.into_iter().map(|b| source.add(b)).collect();
        let fold = match cct::fold_segments(&source, &ids) {
            Poll::Ready(fold) => fold,
            Poll::NeedData(_) => unreachable!("SliceSource is fully resident"),
        };
        let approx_bytes = fold.len() * 96 + fold.bands.len() * 56;
        self.open.insert(
            run_key.to_string(),
            OpenRun {
                fold,
                names,
                epoch,
                approx_bytes,
                last_used: self.clock,
            },
        );
        self.evict_to_budget();
        Ok(())
    }

    /// Drop cached folds beyond the byte budget, LRU first.
    fn evict_to_budget(&mut self) {
        let mut total: usize = self.open.values().map(|r| r.approx_bytes).sum();
        while total > self.cache_budget && self.open.len() > 1 {
            let Some(oldest) = self
                .open
                .iter()
                .min_by_key(|(_, r)| r.last_used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(run) = self.open.remove(&oldest) {
                total -= run.approx_bytes;
            }
        }
    }

    /// Reads a run's segment bytes + dictionary. Boundary dirs prefer the
    /// sealed `cct.bamlcct`; otherwise the bound session's segments.
    fn load_run_bytes(
        &self,
        run_key: &str,
    ) -> Result<(Vec<Vec<u8>>, FxHashMap<u32, String>, u64), String> {
        let (seg_paths, revision) = self.resolve_run_paths(run_key)?;
        let mut files = Vec::new();
        let mut epoch = 0u64;
        for path in seg_paths {
            match std::fs::read(&path) {
                Ok(bytes) => {
                    epoch += bytes.len() as u64;
                    files.push(bytes);
                }
                Err(err) => return Err(format!("reading {}: {err}", path.display())),
            }
        }
        Ok((files, self.load_names(&revision), epoch))
    }

    fn resolve_run_paths(&self, run_key: &str) -> Result<(Vec<PathBuf>, String), String> {
        let boundary_dir = self.root.join("history").join(run_key);
        if boundary_dir.is_dir() {
            let snapshot = boundary_dir.join("cct.bamlcct");
            let revision = self.boundary_revision(&boundary_dir);
            if snapshot.exists() {
                return Ok((vec![snapshot], revision));
            }
            // Live/crashed: filter+fold over the bound session's segments.
            let bound = self.boundary_session(&boundary_dir);
            if let Some(session_dir) = bound {
                return Ok((
                    session_segments(&self.root.join("sessions").join(session_dir)),
                    revision,
                ));
            }
            return Err(format!("run {run_key}: no snapshot and no bound session"));
        }
        let session_dir = self.root.join("sessions").join(run_key);
        if session_dir.is_dir() {
            let revision = session_revision(&session_dir);
            return Ok((session_segments(&session_dir), revision));
        }
        Err(format!("unknown run key {run_key}"))
    }

    fn boundary_revision(&self, dir: &Path) -> String {
        read_meta_field(dir.join("boundary.bamlmeta"), |r| match r {
            bex_events::prof::cct::meta::MetaRecord::BoundaryBegin { revision_id, .. } => {
                Some(revision_id.clone())
            }
            _ => None,
        })
    }

    fn boundary_session(&self, dir: &Path) -> Option<String> {
        let s = read_meta_field(dir.join("boundary.bamlmeta"), |r| match r {
            bex_events::prof::cct::meta::MetaRecord::BoundaryBound { session_dir, .. } => {
                Some(session_dir.clone())
            }
            _ => None,
        });
        if s.is_empty() { None } else { Some(s) }
    }

    fn load_names(&self, revision: &str) -> FxHashMap<u32, String> {
        let mut names = FxHashMap::default();
        if revision.is_empty() {
            return names;
        }
        let path = self.root.join("dict").join(format!("{revision}.bamldict"));
        let Ok(bytes) = std::fs::read(&path) else {
            return names;
        };
        let Ok(dict) = bex_events::dict::read_dict(&bytes) else {
            return names;
        };
        if let Some(section) = dict.functions {
            for row in section.functions {
                names.insert(row.function_id, row.fqn);
            }
        }
        names
    }

    fn run(&mut self, run_key: &str) -> Option<&OpenRun> {
        self.clock += 1;
        let clock = self.clock;
        let run = self.open.get_mut(run_key)?;
        run.last_used = clock;
        Some(&*run)
    }

    /// RunMeta frame: the function dictionary (id ↔ fqn) + fold facts.
    /// Sent once per open; later frames reference functions by id.
    #[must_use]
    pub fn run_meta_frame(&mut self, run_key: &str, request_id: u64) -> Vec<u8> {
        let Some(run) = self.run(run_key) else {
            return bqf1::status_frame(request_id, 404, "run not open");
        };
        let mut ids: Vec<u32> = run.names.keys().copied().collect();
        ids.sort_unstable();
        let fqns: Vec<String> = ids
            .iter()
            .map(|id| run.names.get(id).cloned().unwrap_or_default())
            .collect();
        let flags = frame_flags(&run.fold);
        bqf1::encode_frame(
            FrameKind::RunMeta,
            flags,
            request_id,
            run.epoch,
            &[Col::U32(&ids), Col::Str(&fqns)],
        )
    }

    /// §9.4 aggregate tier: activity bands. §9.3 wire bound: the frame
    /// never exceeds [`DEFAULT_MAX_BYTES`] — over-budget band sets climb
    /// LOD (adjacent windows merge in powers of two) with `lod_degraded`
    /// set, so a month-long session costs the same viewport frame as a
    /// short one.
    #[must_use]
    pub fn timeline_frame(&mut self, run_key: &str, request_id: u64) -> Vec<u8> {
        let Some(run) = self.run(run_key) else {
            return bqf1::status_frame(request_id, 404, "run not open");
        };
        let base_flags = frame_flags(&run.fold);
        let epoch = run.epoch;
        // ~58 B/row across the seven columns.
        let budget_rows = DEFAULT_MAX_BYTES / 64;
        let mut factor = 1usize;
        let mut bands: Vec<cct::BandRow> = run.fold.bands.clone();
        while bands.len() > budget_rows {
            factor *= 2;
            bands = cct::coarsen_bands(&run.fold.bands, factor);
        }
        let flags = base_flags
            | if factor > 1 {
                bqf1::FLAG_LOD_DEGRADED
            } else {
                0
            };
        let thread: Vec<u64> = bands.iter().map(|b| b.thread).collect();
        let first: Vec<u64> = bands.iter().map(|b| b.first_ts_ns).collect();
        let last: Vec<u64> = bands.iter().map(|b| b.last_ts_ns).collect();
        let busy: Vec<u64> = bands.iter().map(|b| b.busy_ns).collect();
        let awaiting: Vec<u64> = bands.iter().map(|b| b.await_ns).collect();
        let dominant: Vec<u32> = bands.iter().map(|b| b.dominant_function).collect();
        let errors: Vec<u64> = bands.iter().map(|b| b.errors).collect();
        bqf1::encode_frame(
            FrameKind::Timeline,
            flags,
            request_id,
            epoch,
            &[
                Col::U64(&thread),
                Col::U64(&first),
                Col::U64(&last),
                Col::U64(&busy),
                Col::U64(&awaiting),
                Col::U32(&dominant),
                Col::U64(&errors),
            ],
        )
    }

    /// §9.6 Left Heavy preorder rows at a pixel width. §9.3 wire bound:
    /// over-budget row sets halve the effective pixel width (deep trees
    /// multiply rows past the per-level extent floor) with `lod_degraded`.
    #[must_use]
    pub fn left_heavy_frame(
        &mut self,
        run_key: &str,
        pixel_width: u32,
        request_id: u64,
    ) -> Vec<u8> {
        let Some(run) = self.run(run_key) else {
            return bqf1::status_frame(request_id, 404, "run not open");
        };
        let budget_rows = DEFAULT_MAX_BYTES / 48;
        let mut width = pixel_width.clamp(1, MAX_PIXEL_WIDTH);
        let mut rows = cct::left_heavy(&run.fold, width);
        let mut degraded = false;
        while rows.function.len() > budget_rows && width > 64 {
            width /= 2;
            degraded = true;
            rows = cct::left_heavy(&run.fold, width);
        }
        let flags = frame_flags(&run.fold) | if degraded { bqf1::FLAG_LOD_DEGRADED } else { 0 };
        bqf1::encode_frame(
            FrameKind::LeftHeavy,
            flags,
            request_id,
            run.epoch,
            &[
                Col::U32(&rows.depth),
                Col::U32(&rows.function),
                Col::U64(&rows.total_ns),
                Col::U64(&rows.self_ns),
                Col::U64(&rows.enters),
                Col::U64(&rows.errors),
                Col::U32(&rows.folded),
            ],
        )
    }

    /// §9.6 top-functions table.
    #[must_use]
    pub fn top_functions_frame(&mut self, run_key: &str, limit: u32, request_id: u64) -> Vec<u8> {
        let Some(run) = self.run(run_key) else {
            return bqf1::status_frame(request_id, 404, "run not open");
        };
        let rows = cct::top_functions(&run.fold, limit.clamp(1, 10_000) as usize);
        let flags = frame_flags(&run.fold);
        bqf1::encode_frame(
            FrameKind::TopFunctions,
            flags,
            request_id,
            run.epoch,
            &[
                Col::U32(&rows.function),
                Col::U64(&rows.calls),
                Col::U64(&rows.total_ns),
                Col::U64(&rows.self_ns),
                Col::U64(&rows.errors),
            ],
        )
    }

    /// Data epoch of an open run (bumps when reopened over grown tails).
    #[must_use]
    pub fn run_epoch(&self, run_key: &str) -> Option<u64> {
        self.open.get(run_key).map(|r| r.epoch)
    }

    /// BQL read access: the cached fold of an open run (bumps its LRU
    /// slot). `None` until [`ObserveEngine::open_run`] succeeded.
    #[must_use]
    pub fn fold(&mut self, run_key: &str) -> Option<&CctFold> {
        self.run(run_key).map(|r| &r.fold)
    }

    /// BQL read access: an open run's function id → fqn dictionary names
    /// (empty map when the revision dictionary is missing).
    #[must_use]
    pub fn names(&self, run_key: &str) -> Option<&FxHashMap<u32, String>> {
        self.open.get(run_key).map(|r| &r.names)
    }

    /// §9.2 live mirror: open (or refresh) a run from externally fetched
    /// live-segment bytes (`bex_events::prof::cct_live_segment`) instead
    /// of disk. `revision` joins dictionary names as usual.
    pub fn open_live(&mut self, run_key: &str, segment_bytes: Vec<u8>, revision: &str) {
        self.clock += 1;
        // Content-sensitive epoch: a live engine with a fixed node
        // population re-encodes to the SAME length every window — length
        // alone would freeze subscriptions.
        let epoch = (segment_bytes.len() as u64) << 32
            | u64::from(bex_events::prof::cct::crc32c::crc32c(&segment_bytes));
        let names = self.load_names(revision);
        let mut source = SliceSource::new();
        let id = source.add(segment_bytes);
        let fold = match cct::fold_segments(&source, &[id]) {
            Poll::Ready(fold) => fold,
            Poll::NeedData(_) => unreachable!("SliceSource is fully resident"),
        };
        let approx_bytes = fold.len() * 96 + fold.bands.len() * 56;
        self.open.insert(
            run_key.to_string(),
            OpenRun {
                fold,
                names,
                epoch,
                approx_bytes,
                last_used: self.clock,
            },
        );
        self.evict_to_budget();
    }
}

fn frame_flags(fold: &CctFold) -> u16 {
    let mut flags = 0;
    if fold.torn || !fold.sealed {
        flags |= bqf1::FLAG_PARTIAL_TAIL;
    }
    flags
}

fn session_segments(session_dir: &Path) -> Vec<PathBuf> {
    let mut segs: Vec<PathBuf> = std::fs::read_dir(session_dir.join("cct"))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "bamlseg"))
                .collect()
        })
        .unwrap_or_default();
    segs.sort();
    segs
}

fn session_revision(session_dir: &Path) -> String {
    read_meta_field(session_dir.join("session.bamlmeta"), |r| match r {
        bex_events::prof::cct::meta::MetaRecord::SessionBegin { revision_id, .. } => {
            Some(revision_id.clone())
        }
        _ => None,
    })
}

fn read_meta_field(
    path: PathBuf,
    pick: impl Fn(&bex_events::prof::cct::meta::MetaRecord) -> Option<String>,
) -> String {
    let Ok(bytes) = std::fs::read(&path) else {
        return String::new();
    };
    let Ok(contents) = bex_events::prof::cct::meta::read_meta(&bytes) else {
        return String::new();
    };
    contents.records.iter().find_map(pick).unwrap_or_default()
}
