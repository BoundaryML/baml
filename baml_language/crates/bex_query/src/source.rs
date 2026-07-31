use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    hash::Hash,
    ops::Deref,
    sync::{Arc, Mutex},
};

#[cfg(feature = "native")]
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::QueryError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ByteRange {
    pub file: FileId,
    pub start: u64,
    pub end: u64,
}

impl ByteRange {
    pub fn new(file: FileId, start: u64, end: u64) -> Result<Self, QueryError> {
        if end < start {
            return Err(QueryError::invalid_request(
                "byte range end must not precede start",
            ));
        }
        Ok(Self { file, start, end })
    }

    #[must_use]
    pub fn len(self) -> u64 {
        self.end - self.start
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

#[derive(Clone, Debug)]
pub struct ByteView {
    bytes: Arc<[u8]>,
    start: usize,
    end: usize,
}

impl ByteView {
    fn new(bytes: Arc<[u8]>, start: usize, end: usize) -> Self {
        Self { bytes, start, end }
    }
}

impl Deref for ByteView {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.bytes[self.start..self.end]
    }
}

impl AsRef<[u8]> for ByteView {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

/// Resident-byte interface used by the query core.
///
/// Implementations never block from `view`: a missing view causes the engine
/// to return [`crate::QueryPoll::NeedData`] with the exact range to supply.
pub trait ByteSource: Send + Sync {
    fn committed_len(&self, file: FileId) -> u64;
    fn generation(&self, file: FileId) -> u64;
    fn view(&self, range: &ByteRange) -> Option<ByteView>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceSnapshot {
    pub committed_len: u64,
    pub generation: u64,
}

#[derive(Clone)]
struct ResidentFile {
    bytes: Arc<[u8]>,
    committed_len: u64,
    generation: u64,
}

#[derive(Default)]
pub struct MemorySource {
    files: Mutex<HashMap<FileId, ResidentFile>>,
}

impl MemorySource {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, file: FileId, bytes: impl Into<Arc<[u8]>>) {
        let bytes = bytes.into();
        let mut files = self.files.lock().expect("memory source mutex poisoned");
        let generation = files
            .get(&file)
            .map_or(1, |entry| entry.generation.saturating_add(1));
        files.insert(
            file,
            ResidentFile {
                committed_len: bytes.len() as u64,
                bytes,
                generation,
            },
        );
    }

    pub fn insert_with_snapshot(
        &self,
        file: FileId,
        bytes: impl Into<Arc<[u8]>>,
        snapshot: SourceSnapshot,
    ) -> Result<(), QueryError> {
        let bytes = bytes.into();
        if snapshot.committed_len > bytes.len() as u64 {
            return Err(QueryError::invalid_request(
                "committed length exceeds resident bytes",
            ));
        }
        self.files
            .lock()
            .expect("memory source mutex poisoned")
            .insert(
                file,
                ResidentFile {
                    bytes,
                    committed_len: snapshot.committed_len,
                    generation: snapshot.generation,
                },
            );
        Ok(())
    }

    pub fn append(&self, file: FileId, suffix: &[u8]) -> Result<SourceSnapshot, QueryError> {
        let mut files = self.files.lock().expect("memory source mutex poisoned");
        let entry = files
            .get_mut(&file)
            .ok_or_else(|| QueryError::NotFound(format!("file id {}", file.0)))?;
        let mut bytes = Vec::with_capacity(entry.bytes.len().saturating_add(suffix.len()));
        bytes.extend_from_slice(&entry.bytes);
        bytes.extend_from_slice(suffix);
        entry.bytes = Arc::from(bytes);
        entry.committed_len = entry.bytes.len() as u64;
        entry.generation = entry.generation.saturating_add(1);
        Ok(SourceSnapshot {
            committed_len: entry.committed_len,
            generation: entry.generation,
        })
    }

    #[must_use]
    pub fn snapshot(&self, file: FileId) -> SourceSnapshot {
        let files = self.files.lock().expect("memory source mutex poisoned");
        files
            .get(&file)
            .map_or(SourceSnapshot::default(), |entry| SourceSnapshot {
                committed_len: entry.committed_len,
                generation: entry.generation,
            })
    }
}

impl ByteSource for MemorySource {
    fn committed_len(&self, file: FileId) -> u64 {
        self.snapshot(file).committed_len
    }

    fn generation(&self, file: FileId) -> u64 {
        self.snapshot(file).generation
    }

    fn view(&self, range: &ByteRange) -> Option<ByteView> {
        let files = self.files.lock().expect("memory source mutex poisoned");
        let entry = files.get(&range.file)?;
        if range.end > entry.committed_len {
            return None;
        }
        let start = usize::try_from(range.start).ok()?;
        let end = usize::try_from(range.end).ok()?;
        (end <= entry.bytes.len()).then(|| ByteView::new(Arc::clone(&entry.bytes), start, end))
    }
}

/// Same-process live source for consumer-produced BCCT blocks and the bounded
/// exact-recency sidecar.
///
/// The byte plane is deliberately identical to [`MemorySource`]: the consumer
/// publishes the same committed BCCT bytes it writes to disk. The timeline
/// sidecar carries RAM-only recent calls, which have no durable BCCT rows.
#[derive(Default)]
pub struct LiveMirrorSource {
    bytes: MemorySource,
    timeline: Mutex<HashMap<FileId, crate::TimelineOverlay>>,
}

impl LiveMirrorSource {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, file: FileId, bytes: impl Into<Arc<[u8]>>) {
        self.bytes.insert(file, bytes);
    }

    pub fn insert_with_snapshot(
        &self,
        file: FileId,
        bytes: impl Into<Arc<[u8]>>,
        snapshot: SourceSnapshot,
    ) -> Result<(), QueryError> {
        self.bytes.insert_with_snapshot(file, bytes, snapshot)
    }

    pub fn append(&self, file: FileId, suffix: &[u8]) -> Result<SourceSnapshot, QueryError> {
        self.bytes.append(file, suffix)
    }

    #[must_use]
    pub fn snapshot(&self, file: FileId) -> SourceSnapshot {
        self.bytes.snapshot(file)
    }

    /// Atomically replaces the latest exact timeline snapshot for a live
    /// stream. Its memory remains bounded by the CCT recent-call contract.
    pub fn publish_timeline(&self, file: FileId, overlay: crate::TimelineOverlay) {
        self.timeline
            .lock()
            .expect("live mirror timeline mutex poisoned")
            .insert(file, overlay);
    }

    #[must_use]
    pub fn timeline(&self, file: FileId) -> Option<crate::TimelineOverlay> {
        self.timeline
            .lock()
            .expect("live mirror timeline mutex poisoned")
            .get(&file)
            .cloned()
    }

    pub fn remove(&self, file: FileId) {
        self.timeline
            .lock()
            .expect("live mirror timeline mutex poisoned")
            .remove(&file);
    }
}

impl ByteSource for LiveMirrorSource {
    fn committed_len(&self, file: FileId) -> u64 {
        self.bytes.committed_len(file)
    }

    fn generation(&self, file: FileId) -> u64 {
        self.bytes.generation(file)
    }

    fn view(&self, range: &ByteRange) -> Option<ByteView> {
        self.bytes.view(range)
    }
}

#[cfg(feature = "native")]
#[derive(Default)]
pub struct FileSource {
    paths: Mutex<HashMap<FileId, OpenedPath>>,
}

#[cfg(feature = "native")]
#[derive(Clone)]
struct OpenedPath {
    path: PathBuf,
    file: Arc<fs::File>,
    physical_len: u64,
    committed_len: u64,
    generation: u64,
    modified: Option<std::time::SystemTime>,
    pinned: bool,
}

#[cfg(feature = "native")]
impl FileSource {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&self, file: FileId, path: impl AsRef<Path>) -> Result<SourceSnapshot, QueryError> {
        let path = path.as_ref().to_path_buf();
        let opened_file = Arc::new(fs::File::open(&path)?);
        let metadata = opened_file.metadata()?;
        let snapshot = SourceSnapshot {
            committed_len: metadata.len(),
            generation: file_generation(&metadata),
        };
        self.paths
            .lock()
            .expect("file source mutex poisoned")
            .insert(
                file,
                OpenedPath {
                    path,
                    file: opened_file,
                    physical_len: metadata.len(),
                    committed_len: snapshot.committed_len,
                    generation: snapshot.generation,
                    modified: metadata.modified().ok(),
                    pinned: false,
                },
            );
        Ok(snapshot)
    }

    /// Opens only the committed prefix named by a query snapshot. Appends
    /// after that watermark are intentionally invisible, making pinned BQL
    /// reruns byte-identical while still rejecting truncated/replaced files.
    pub fn open_pinned(
        &self,
        file: FileId,
        path: impl AsRef<Path>,
        snapshot: SourceSnapshot,
    ) -> Result<SourceSnapshot, QueryError> {
        let path = path.as_ref().to_path_buf();
        let opened_file = Arc::new(fs::File::open(&path)?);
        let metadata = opened_file.metadata()?;
        if snapshot.committed_len > metadata.len() {
            return Err(QueryError::invalid_request(
                "pinned committed length exceeds the current file",
            ));
        }
        if snapshot.generation != file_generation(&metadata) {
            return Err(QueryError::invalid_request(
                "pinned source was replaced after the snapshot",
            ));
        }
        self.paths
            .lock()
            .expect("file source mutex poisoned")
            .insert(
                file,
                OpenedPath {
                    path,
                    file: opened_file,
                    physical_len: metadata.len(),
                    committed_len: snapshot.committed_len,
                    generation: snapshot.generation,
                    modified: metadata.modified().ok(),
                    pinned: true,
                },
            );
        Ok(snapshot)
    }

    pub fn refresh(&self, file: FileId) -> Result<SourceSnapshot, QueryError> {
        let opened = self
            .paths
            .lock()
            .expect("file source mutex poisoned")
            .get(&file)
            .cloned()
            .ok_or_else(|| QueryError::NotFound(format!("file id {}", file.0)))?;
        if opened.pinned {
            return Ok(SourceSnapshot {
                committed_len: opened.committed_len,
                generation: opened.generation,
            });
        }
        let metadata = fs::metadata(&opened.path)?;
        let modified = metadata.modified().ok();
        if metadata.len() == opened.physical_len
            && modified == opened.modified
            && file_generation(&metadata) == opened.generation
        {
            return Ok(SourceSnapshot {
                committed_len: opened.committed_len,
                generation: opened.generation,
            });
        }
        let next_file = Arc::new(fs::File::open(&opened.path)?);
        let next_metadata = next_file.metadata()?;
        let next_generation = file_generation(&next_metadata);
        let next_len = next_metadata.len();
        let modified = next_metadata.modified().ok();
        self.paths
            .lock()
            .expect("file source mutex poisoned")
            .insert(
                file,
                OpenedPath {
                    path: opened.path,
                    file: next_file,
                    physical_len: next_len,
                    committed_len: next_len,
                    generation: next_generation,
                    modified,
                    pinned: false,
                },
            );
        Ok(SourceSnapshot {
            committed_len: next_len,
            generation: next_generation,
        })
    }

    #[must_use]
    pub fn is_open(&self, file: FileId) -> bool {
        self.paths
            .lock()
            .expect("file source mutex poisoned")
            .contains_key(&file)
    }

    #[must_use]
    pub fn path(&self, file: FileId) -> Option<PathBuf> {
        self.paths
            .lock()
            .expect("file source mutex poisoned")
            .get(&file)
            .map(|opened| opened.path.clone())
    }
}

#[cfg(feature = "native")]
impl ByteSource for FileSource {
    fn committed_len(&self, file: FileId) -> u64 {
        self.paths
            .lock()
            .expect("file source mutex poisoned")
            .get(&file)
            .map_or(0, |opened| opened.committed_len)
    }

    fn generation(&self, file: FileId) -> u64 {
        self.paths
            .lock()
            .expect("file source mutex poisoned")
            .get(&file)
            .map_or(0, |opened| opened.generation)
    }

    fn view(&self, range: &ByteRange) -> Option<ByteView> {
        let opened = self
            .paths
            .lock()
            .expect("file source mutex poisoned")
            .get(&range.file)
            .cloned()?;
        if range.end > opened.committed_len {
            return None;
        }
        let len = usize::try_from(range.len()).ok()?;
        let mut bytes = vec![0_u8; len];
        if len != 0 {
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileExt as _;
                opened.file.read_exact_at(&mut bytes, range.start).ok()?;
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::FileExt as _;
                opened.file.seek_read(&mut bytes, range.start).ok()?;
            }
            #[cfg(not(any(unix, windows)))]
            {
                use std::io::{Read as _, Seek as _, SeekFrom};
                let mut file = opened.file.try_clone().ok()?;
                file.seek(SeekFrom::Start(range.start)).ok()?;
                file.read_exact(&mut bytes).ok()?;
            }
        }
        Some(ByteView::new(Arc::from(bytes), 0, len))
    }
}

#[derive(Debug)]
struct SizedEntry<V> {
    value: V,
    bytes: usize,
}

/// LRU cache whose invariant is retained bytes, never entry count.
#[derive(Debug)]
pub struct ByteBudgetCache<K, V> {
    max_bytes: usize,
    retained_bytes: usize,
    entries: HashMap<K, SizedEntry<V>>,
    lru: VecDeque<K>,
}

impl<K: Clone + Eq + Hash, V> ByteBudgetCache<K, V> {
    #[must_use]
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            retained_bytes: 0,
            entries: HashMap::new(),
            lru: VecDeque::new(),
        }
    }

    #[must_use]
    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn set_max_bytes(&mut self, max_bytes: usize) {
        self.max_bytes = max_bytes;
        self.evict_to_budget();
    }

    pub fn insert(&mut self, key: K, value: V, bytes: usize) -> bool {
        self.remove(&key);
        if bytes > self.max_bytes {
            return false;
        }
        self.retained_bytes = self.retained_bytes.saturating_add(bytes);
        self.entries
            .insert(key.clone(), SizedEntry { value, bytes });
        self.lru.push_back(key);
        self.evict_to_budget();
        true
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        if !self.entries.contains_key(key) {
            return None;
        }
        if let Some(position) = self.lru.iter().position(|candidate| candidate == key) {
            self.lru.remove(position);
        }
        self.lru.push_back(key.clone());
        self.entries.get(key).map(|entry| &entry.value)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if let Some(position) = self.lru.iter().position(|candidate| candidate == key) {
            self.lru.remove(position);
        }
        let entry = self.entries.remove(key)?;
        self.retained_bytes = self.retained_bytes.saturating_sub(entry.bytes);
        Some(entry.value)
    }

    fn evict_to_budget(&mut self) {
        while self.retained_bytes > self.max_bytes {
            let Some(key) = self.lru.pop_front() else {
                self.entries.clear();
                self.retained_bytes = 0;
                break;
            };
            if let Some(entry) = self.entries.remove(&key) {
                self.retained_bytes = self.retained_bytes.saturating_sub(entry.bytes);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct CachedRange {
    file: FileId,
    generation: u64,
    start: u64,
}

#[derive(Clone, Debug)]
struct RangeEntry {
    bytes: Arc<[u8]>,
}

#[derive(Debug)]
struct RangeCacheState {
    snapshots: HashMap<FileId, SourceSnapshot>,
    entries: BTreeMap<CachedRange, RangeEntry>,
    lru: VecDeque<CachedRange>,
    retained_bytes: usize,
    max_bytes: usize,
}

/// Host-filled range cache for WASM and HTTP Range hosts.
///
/// `view` only consults resident bytes. The host inserts ranges requested by
/// `NeedData`, then retries the unchanged query.
#[derive(Debug)]
pub struct RangeCacheSource {
    state: Mutex<RangeCacheState>,
}

impl RangeCacheSource {
    #[must_use]
    pub fn new(max_bytes: usize) -> Self {
        Self {
            state: Mutex::new(RangeCacheState {
                snapshots: HashMap::new(),
                entries: BTreeMap::new(),
                lru: VecDeque::new(),
                retained_bytes: 0,
                max_bytes,
            }),
        }
    }

    pub fn set_snapshot(&self, file: FileId, snapshot: SourceSnapshot) {
        let mut state = self.state.lock().expect("range cache mutex poisoned");
        let old_generation = state.snapshots.get(&file).map(|value| value.generation);
        state.snapshots.insert(file, snapshot);
        if old_generation.is_some_and(|generation| generation != snapshot.generation) {
            let stale = state
                .entries
                .keys()
                .filter(|key| key.file == file && key.generation != snapshot.generation)
                .copied()
                .collect::<Vec<_>>();
            for key in stale {
                remove_range_entry(&mut state, key);
            }
        }
    }

    pub fn insert(
        &self,
        file: FileId,
        generation: u64,
        start: u64,
        bytes: impl Into<Arc<[u8]>>,
    ) -> bool {
        let bytes = bytes.into();
        let key = CachedRange {
            file,
            generation,
            start,
        };
        let mut state = self.state.lock().expect("range cache mutex poisoned");
        remove_range_entry(&mut state, key);
        if bytes.len() > state.max_bytes {
            return false;
        }
        state.retained_bytes = state.retained_bytes.saturating_add(bytes.len());
        state.entries.insert(key, RangeEntry { bytes });
        state.lru.push_back(key);
        while state.retained_bytes > state.max_bytes {
            let Some(oldest) = state.lru.pop_front() else {
                break;
            };
            if let Some(entry) = state.entries.remove(&oldest) {
                state.retained_bytes = state.retained_bytes.saturating_sub(entry.bytes.len());
            }
        }
        true
    }

    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.state
            .lock()
            .expect("range cache mutex poisoned")
            .retained_bytes
    }
}

impl ByteSource for RangeCacheSource {
    fn committed_len(&self, file: FileId) -> u64 {
        self.state
            .lock()
            .expect("range cache mutex poisoned")
            .snapshots
            .get(&file)
            .map_or(0, |snapshot| snapshot.committed_len)
    }

    fn generation(&self, file: FileId) -> u64 {
        self.state
            .lock()
            .expect("range cache mutex poisoned")
            .snapshots
            .get(&file)
            .map_or(0, |snapshot| snapshot.generation)
    }

    fn view(&self, range: &ByteRange) -> Option<ByteView> {
        let mut state = self.state.lock().expect("range cache mutex poisoned");
        let snapshot = *state.snapshots.get(&range.file)?;
        if range.end > snapshot.committed_len {
            return None;
        }
        let probe = CachedRange {
            file: range.file,
            generation: snapshot.generation,
            start: range.start,
        };
        let (key, entry) = state.entries.range(..=probe).next_back()?;
        if key.file != range.file || key.generation != snapshot.generation {
            return None;
        }
        let relative_start = range.start.checked_sub(key.start)?;
        let relative_end = range.end.checked_sub(key.start)?;
        let start = usize::try_from(relative_start).ok()?;
        let end = usize::try_from(relative_end).ok()?;
        if end > entry.bytes.len() {
            return None;
        }
        let key = *key;
        let bytes = Arc::clone(&entry.bytes);
        if let Some(position) = state.lru.iter().position(|candidate| *candidate == key) {
            state.lru.remove(position);
        }
        state.lru.push_back(key);
        Some(ByteView::new(bytes, start, end))
    }
}

fn remove_range_entry(state: &mut RangeCacheState, key: CachedRange) {
    if let Some(position) = state.lru.iter().position(|candidate| *candidate == key) {
        state.lru.remove(position);
    }
    if let Some(entry) = state.entries.remove(&key) {
        state.retained_bytes = state.retained_bytes.saturating_sub(entry.bytes.len());
    }
}

#[cfg(all(feature = "native", unix))]
fn file_generation(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt as _;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for value in [metadata.dev(), metadata.ino()] {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

#[cfg(all(feature = "native", not(unix)))]
fn file_generation(metadata: &fs::Metadata) -> u64 {
    metadata
        .created()
        .or_else(|_| metadata.modified())
        .ok()
        .and_then(|created| created.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| {
            u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
        })
}

#[cfg(all(test, feature = "native"))]
mod native_tests {
    use std::io::{Seek as _, SeekFrom, Write as _};

    use super::{ByteRange, ByteSource, FileId, FileSource};

    #[test]
    fn file_source_reads_requested_ranges_without_resident_whole_file() {
        let path = std::env::temp_dir().join(format!(
            "baml-query-sparse-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let logical_len = 10_u64 * 1024 * 1024 * 1024;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.set_len(logical_len).unwrap();
        file.write_all(b"BCCT").unwrap();
        file.seek(SeekFrom::Start(logical_len - 4)).unwrap();
        file.write_all(b"FOOT").unwrap();
        file.sync_data().unwrap();
        drop(file);

        let source = FileSource::new();
        let file_id = FileId(7);
        let snapshot = source.open(file_id, &path).unwrap();
        assert_eq!(snapshot.committed_len, logical_len);
        assert_eq!(
            &*source
                .view(&ByteRange::new(file_id, 0, 4).unwrap())
                .unwrap(),
            b"BCCT"
        );
        assert_eq!(
            &*source
                .view(&ByteRange::new(file_id, logical_len - 4, logical_len).unwrap())
                .unwrap(),
            b"FOOT"
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn pinned_file_source_sees_append_prefix_and_rejects_replacement() {
        let path = std::env::temp_dir().join(format!(
            "baml-query-pinned-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, b"ABC").unwrap();
        let file_id = FileId(11);
        let original = FileSource::new();
        let snapshot = original.open(file_id, &path).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"DEF")
            .unwrap();

        let pinned = FileSource::new();
        pinned.open_pinned(file_id, &path, snapshot).unwrap();
        assert_eq!(pinned.committed_len(file_id), 3);
        assert_eq!(
            &*pinned
                .view(&ByteRange::new(file_id, 0, 3).unwrap())
                .unwrap(),
            b"ABC"
        );
        assert!(
            pinned
                .view(&ByteRange::new(file_id, 3, 4).unwrap())
                .is_none()
        );

        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"ABCDEF").unwrap();
        assert!(
            FileSource::new()
                .open_pinned(file_id, &path, snapshot)
                .is_err()
        );
        std::fs::remove_file(path).unwrap();
    }
}
