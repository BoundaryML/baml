//! §9.2 `SegmentSource`: the sans-io byte plane.
//!
//! Queries never do IO. They read through a [`SegmentSource`]; a view that
//! cannot be satisfied from resident bytes surfaces as
//! [`Poll::NeedData`] with the exact ranges, and the host fetches and
//! retries. Native mmap satisfies everything immediately; wasm and HTTP
//! Range hosts fill a byte-range cache.

use std::path::{Path, PathBuf};

/// Identifies one file within an open run/root (dense, engine-assigned).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ByteRange {
    pub file: FileId,
    pub offset: u64,
    pub len: u64,
}

/// A query's result: complete, or the exact bytes it still needs.
#[derive(Debug)]
pub enum Poll<T> {
    Ready(T),
    /// Fetch these ranges into the source, then retry the same call.
    NeedData(Vec<ByteRange>),
}

impl<T> Poll<T> {
    /// Unwrap `Ready`, panicking on `NeedData` — for hosts (native mmap)
    /// whose sources are always fully resident.
    pub fn expect_ready(self, what: &str) -> T {
        match self {
            Poll::Ready(v) => v,
            Poll::NeedData(ranges) => {
                panic!(
                    "{what}: source not resident ({} ranges missing)",
                    ranges.len()
                )
            }
        }
    }
}

/// §9.2: the sans-io byte plane. `committed_len` is the readable watermark
/// (never beyond a torn tail for active files); `generation` bumps when a
/// tail grows or meta transitions so caches revalidate.
pub trait SegmentSource: Send + Sync {
    fn committed_len(&self, file: FileId) -> u64;
    fn view(&self, range: &ByteRange) -> Option<&[u8]>;
    fn generation(&self, file: FileId) -> u64;

    /// Whole-file convenience view (0..committed_len).
    fn view_all(&self, file: FileId) -> Option<&[u8]> {
        self.view(&ByteRange {
            file,
            offset: 0,
            len: self.committed_len(file),
        })
    }
}

/// Fully-resident in-memory source (tests, wasm after fetch, and the
/// building block for `LiveMirrorSource` snapshots).
#[derive(Default)]
pub struct SliceSource {
    files: Vec<(Vec<u8>, u64)>, // (bytes, generation)
}

impl SliceSource {
    #[must_use]
    pub fn new() -> SliceSource {
        SliceSource::default()
    }

    pub fn add(&mut self, bytes: Vec<u8>) -> FileId {
        self.files.push((bytes, 0));
        FileId(u32::try_from(self.files.len() - 1).unwrap_or(u32::MAX))
    }

    /// Replace a file's bytes (live tail growth), bumping its generation.
    pub fn replace(&mut self, file: FileId, bytes: Vec<u8>) {
        if let Some(slot) = self.files.get_mut(file.0 as usize) {
            slot.0 = bytes;
            slot.1 += 1;
        }
    }
}

impl SegmentSource for SliceSource {
    fn committed_len(&self, file: FileId) -> u64 {
        self.files
            .get(file.0 as usize)
            .map_or(0, |(b, _)| b.len() as u64)
    }

    fn view(&self, range: &ByteRange) -> Option<&[u8]> {
        let (bytes, _) = self.files.get(range.file.0 as usize)?;
        let start = usize::try_from(range.offset).ok()?;
        let end = start.checked_add(usize::try_from(range.len).ok()?)?;
        bytes.get(start..end)
    }

    fn generation(&self, file: FileId) -> u64 {
        self.files.get(file.0 as usize).map_or(0, |(_, g)| *g)
    }
}

/// Native mmap source (§9.2): maps files read-only and never exposes bytes
/// beyond the committed length captured at (re)open. Tail growth is picked
/// up by `refresh`, which re-stats and remaps only when the file grew.
#[cfg(feature = "native")]
pub struct MmapSource {
    files: Vec<MappedFile>,
}

#[cfg(feature = "native")]
struct MappedFile {
    path: PathBuf,
    map: Option<memmap2::Mmap>,
    committed: u64,
    generation: u64,
}

#[cfg(feature = "native")]
impl MmapSource {
    #[must_use]
    pub fn new() -> MmapSource {
        MmapSource { files: Vec::new() }
    }

    /// Open (map) one file; returns its id. Missing/empty files map to a
    /// zero-length view rather than an error — a crashed session may have
    /// an empty tail segment, and readers must shrug.
    pub fn open(&mut self, path: &Path) -> FileId {
        let (map, committed) = Self::map(path);
        self.files.push(MappedFile {
            path: path.to_path_buf(),
            map,
            committed,
            generation: 0,
        });
        FileId(u32::try_from(self.files.len() - 1).unwrap_or(u32::MAX))
    }

    fn map(path: &Path) -> (Option<memmap2::Mmap>, u64) {
        let Ok(file) = std::fs::File::open(path) else {
            return (None, 0);
        };
        let len = file.metadata().map_or(0, |m| m.len());
        if len == 0 {
            return (None, 0);
        }
        // SAFETY: read-only map of a file we never truncate; writers are
        // append-only (§6.3), so the committed prefix is stable.
        #[expect(unsafe_code, reason = "read-only mmap of append-only artifact files")]
        let map = unsafe { memmap2::Mmap::map(&file).ok() };
        (map, len)
    }

    /// Re-stat every file; remap the ones that grew. Returns true if any
    /// generation bumped (subscribers should re-run their queries).
    pub fn refresh(&mut self) -> bool {
        let mut changed = false;
        for f in &mut self.files {
            let len = std::fs::metadata(&f.path).map_or(0, |m| m.len());
            if len != f.committed {
                let (map, committed) = Self::map(&f.path);
                f.map = map;
                f.committed = committed;
                f.generation += 1;
                changed = true;
            }
        }
        changed
    }
}

#[cfg(feature = "native")]
impl Default for MmapSource {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "native")]
impl SegmentSource for MmapSource {
    fn committed_len(&self, file: FileId) -> u64 {
        self.files.get(file.0 as usize).map_or(0, |f| f.committed)
    }

    fn view(&self, range: &ByteRange) -> Option<&[u8]> {
        let f = self.files.get(range.file.0 as usize)?;
        let map = f.map.as_ref()?;
        let start = usize::try_from(range.offset).ok()?;
        let end = start.checked_add(usize::try_from(range.len).ok()?)?;
        if end as u64 > f.committed {
            return None;
        }
        map.get(start..end)
    }

    fn generation(&self, file: FileId) -> u64 {
        self.files.get(file.0 as usize).map_or(0, |f| f.generation)
    }
}

/// §9.2 `LiveMirrorSource`: one logical file whose bytes come from the
/// consumer's in-RAM live fold (`bex_events::prof::cct_live_segment`),
/// encoded in the identical BCCT block format — the query code cannot
/// tell. `refresh` swaps in newly fetched bytes and bumps the generation;
/// hosts call it between queries (same-process live views are then
/// ~0-latency instead of waiting on group commit).
pub struct LiveMirrorSource {
    fetch: Box<dyn Fn() -> Option<Vec<u8>> + Send + Sync>,
    bytes: Vec<u8>,
    generation: u64,
}

impl LiveMirrorSource {
    pub fn new(fetch: impl Fn() -> Option<Vec<u8>> + Send + Sync + 'static) -> LiveMirrorSource {
        LiveMirrorSource {
            fetch: Box::new(fetch),
            bytes: Vec::new(),
            generation: 0,
        }
    }

    /// Fetch the current live segment; true when the bytes changed.
    pub fn refresh(&mut self) -> bool {
        match (self.fetch)() {
            Some(bytes) if bytes != self.bytes => {
                self.bytes = bytes;
                self.generation += 1;
                true
            }
            _ => false,
        }
    }

    /// The single logical file this source serves.
    #[must_use]
    pub fn file(&self) -> FileId {
        FileId(0)
    }
}

impl SegmentSource for LiveMirrorSource {
    fn committed_len(&self, file: FileId) -> u64 {
        if file.0 == 0 {
            self.bytes.len() as u64
        } else {
            0
        }
    }

    fn view(&self, range: &ByteRange) -> Option<&[u8]> {
        if range.file.0 != 0 {
            return None;
        }
        let start = usize::try_from(range.offset).ok()?;
        let end = start.checked_add(usize::try_from(range.len).ok()?)?;
        self.bytes.get(start..end)
    }

    fn generation(&self, file: FileId) -> u64 {
        if file.0 == 0 { self.generation } else { 0 }
    }
}
