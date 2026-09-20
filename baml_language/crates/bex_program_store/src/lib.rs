//! Content-addressed, immutable store of compiled BEX programs.
//!
//! A program is identified by its program hash: the SHA-256 of the
//! Borsh-encoded `Program`. A snapshot header carries the same value
//! (`bex_snapshot::program_hash`), so a worker that resumes a run can fetch the
//! exact program the run was started with, without the project directory and
//! without compiling. The contract is `documents/durable-poc-contracts.md`,
//! section 9.5.
//!
//! This crate works on bytes only. It does not depend on the VM types, so a
//! process that only moves programs between sites does not link the runtime.
//!
//! # Layout
//!
//! ```text
//! <store>/<first two hex chars of the hash>/<hash>.bamlprog          entry
//! <store>/<first two hex chars of the hash>/<hash>.<build tag>.build  build marker
//! ```
//!
//! The two-character fan-out gives 256 shard directories. A lookup opens
//! paths that are computed from the hash and the build. It never lists a
//! directory, so its cost does not depend on the number of entries.
//!
//! # Entry file
//!
//! All integers are little-endian.
//!
//! | Offset | Size | Field |
//! |---|---|---|
//! | 0 | 8 | magic `BAMLPROG` |
//! | 8 | 4 | format version ([`FORMAT_VERSION`]) |
//! | 12 | 4 | `build_len`: length of the next field in bytes |
//! | 16 | `build_len` | runtime build of the writer, UTF-8, one line |
//! | 16 + `build_len` | 8 | `payload_len` |
//! | 24 + `build_len` | `payload_len` | payload: the Borsh bytes of the program |
//!
//! The file ends with the payload. The hash of an entry is the SHA-256 of the
//! payload and of nothing else, so the header never changes the identity of a
//! program.
//!
//! # Runtime builds
//!
//! The Borsh layout of a program and the meaning of its bytecode belong to one
//! runtime build. A reader uses an entry only when its own build stored these
//! bytes, and reports every other entry as missing ([`Missing::OtherBuild`]).
//! "Stored" means one of two things:
//!
//! - the header of the entry names the reader's build, or
//! - the build marker of the reader's build exists next to the entry.
//!
//! Every `put` creates the marker of its build. The marker is what makes a
//! store that several builds share correct without locks. During a rolling
//! deploy two builds compile one source to identical bytes and put the same
//! hash. With the header as the only record, each `put` would have to rewrite
//! the entry, and the build that renames last would erase the other. Markers
//! are separate files that are only ever created, so no writer can undo the
//! record of another. The tag in the marker's name is the first 16 hex
//! characters of the SHA-256 of the build name, and the marker's content is
//! the build name.
//!
//! # Concurrency
//!
//! - A writer writes a temporary file in the shard directory, flushes it to
//!   stable storage, renames it to its final path, and flushes the directory.
//!   A reader therefore sees no entry or a complete entry, and never a partial
//!   one. A crash leaves at most a temporary file behind, which
//!   [`ProgramStore::remove_stale_temp_files`] deletes.
//! - Any number of processes may put the same hash at the same time. A writer
//!   replaces the entry file only when the file is absent or fails
//!   verification. Every writer of one hash writes the same payload, so every
//!   interleaving of the renames ends with a valid entry.
//! - A reader verifies the header, the payload length, and the SHA-256 of the
//!   payload against the requested hash before it returns bytes. An entry that
//!   fails a check is reported as missing, and the next `put` of that hash
//!   replaces it.
//! - Readers never write. A store on a read-only file system serves lookups.
//!
//! # Collection
//!
//! The store never deletes an entry by itself. A paused run can sleep for
//! weeks and still needs its program, so age is not a safe criterion. A future
//! collector decides from the run stores which hashes are reachable, walks the
//! store with [`ProgramStore::entries`], and deletes the rest with
//! [`ProgramStore::remove`]. The collector is not part of this crate.
//!
//! # Cost
//!
//! The numbers come from the ignored test `measure_program_load` in
//! `baml_cli/tests/worker_program_store_e2e.rs`, which starts real
//! `baml-cli worker` processes. They are medians of seven runs of an optimized
//! build (profile `fasttest`) on an Apple silicon laptop. "x20" is a generated
//! project that holds the demo program twenty times.
//!
//! | Project | Path | `program_load_ms` | compile | read and verify | Borsh decode | engine build |
//! |---|---|---|---|---|---|---|
//! | demo | compile | 259.9 | 240.0 | | | 18.8 |
//! | demo | store | 23.4 | | 1.3 | 3.6 | 18.1 |
//! | demo x20 | compile | 277.1 | 255.7 | | | 18.3 |
//! | demo x20 | store | 23.1 | | 1.3 | 3.7 | 18.1 |
//!
//! A debug build shows the same ratio: 1199 ms with a compile and 87 ms from
//! the store. The first `put` of a program costs about 20 ms with
//! [`Durability::Full`], most of it in the two flushes.
//!
//! Reading, verifying, and decoding the 2.7 MB entry take 5 ms together. The
//! engine build takes 18 ms and is the same work on both paths: it attaches
//! the native functions, lowers every function to compact bytecode, freezes
//! the compile-time heap, and runs the package initializers, for the standard
//! library as much as for user code. Two later steps follow from that.
//!
//! - Start-up time. Almost all of the engine build is standard library work
//!   that is identical for every program of one runtime build. A long-lived
//!   worker that keeps built engines in memory, keyed by program hash, removes
//!   it. Memory-mapping the entry would not help, because the read is already
//!   1.3 ms.
//! - Size. An entry is 2.7 MB, and all but a few kilobytes of it are the
//!   standard library. A zstd patch of the demo entry against the entry of
//!   another program is 2,590 bytes (`zstd --patch-from`, level 3), and
//!   applying it takes 5 ms. A later format version can store an entry as a
//!   patch against a base entry that holds the standard library program of the
//!   runtime build and the optimization level. The base is a store entry with
//!   its own hash. The identity of a program stays the SHA-256 of its full
//!   Borsh bytes, and a reader verifies that hash after it applied the patch,
//!   so snapshots and the worker protocol do not change. This version stores
//!   full bytes, so that the stored payload is exactly what the hash covers.

use std::{
    fmt,
    fs::{self, File},
    io::{self, Write as _},
    ops::Range,
    path::{Path, PathBuf},
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};

use sha2::{Digest as _, Sha256};

/// First bytes of every entry file.
pub const MAGIC: [u8; 8] = *b"BAMLPROG";

/// Version of the entry file layout described in the crate documentation. It
/// does not describe the payload. The runtime build covers the payload.
pub const FORMAT_VERSION: u32 = 1;

/// File extension of an entry.
pub const ENTRY_EXTENSION: &str = "bamlprog";

/// File extension of a build marker.
pub const MARKER_EXTENSION: &str = "build";

/// Longest accepted runtime build name in bytes.
pub const MAX_BUILD_LEN: usize = 256;

/// Offset of the `build` field: magic, format version, `build_len`.
const FIXED_PREFIX_LEN: usize = MAGIC.len() + 4 + 4;

/// How often a `put` writes again after its write failed or was undone.
const PUT_ATTEMPTS: usize = 4;

const TEMP_PREFIX: &str = ".tmp-";
const TEMP_SUFFIX: &str = ".partial";

/// The identity of a program: SHA-256 of its Borsh bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProgramHash([u8; 32]);

impl ProgramHash {
    /// The hash of `program_bytes`. It equals
    /// `bex_snapshot::program_hash_of_bytes` of the same bytes.
    pub fn of(program_bytes: &[u8]) -> Self {
        Self(Sha256::digest(program_bytes).into())
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// 64 lowercase hexadecimal characters.
    pub fn to_hex(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for ProgramHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ProgramHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ProgramHash({self})")
    }
}

impl FromStr for ProgramHash {
    type Err = StoreError;

    /// Accepts exactly 64 lowercase hexadecimal characters. The text of a hash
    /// becomes a file name, so nothing else is accepted: no path separators,
    /// no dots, no upper case, and no surrounding white space.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let invalid = || StoreError::InvalidHash(text.chars().take(80).collect());
        if text.len() != 64 {
            return Err(invalid());
        }
        let mut out = [0u8; 32];
        let (pairs, _) = text.as_bytes().as_chunks::<2>();
        for (slot, [high, low]) in out.iter_mut().zip(pairs) {
            let high = hex_digit(*high).ok_or_else(invalid)?;
            let low = hex_digit(*low).ok_or_else(invalid)?;
            *slot = (high << 4) | low;
        }
        Ok(Self(out))
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("`{0}` is not a program hash (expected 64 lowercase hexadecimal characters)")]
    InvalidHash(String),
    #[error("the program bytes hash to {actual}, not to {expected}")]
    HashMismatch {
        expected: ProgramHash,
        actual: ProgramHash,
    },
    #[error("`{0}` is not a usable runtime build name (empty, too long, or contains a line break)")]
    InvalidBuild(String),
    #[error("program store: cannot {action} {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Why a lookup did not produce a program. A caller treats every variant as
/// "the store has no entry for this hash". The variant says what to report.
#[derive(Debug)]
pub enum Missing {
    /// No file exists at the entry path.
    NotFound,
    /// A valid entry exists, and the reader's build never stored these bytes.
    OtherBuild {
        /// The build named in the entry header.
        written_by: String,
    },
    /// The entry was written in a file layout that this reader does not know.
    UnsupportedFormat(u32),
    /// The file is torn, truncated, altered, or stored under a wrong name.
    Corrupt(String),
    /// The file exists and cannot be read.
    Unreadable(io::Error),
}

impl fmt::Display for Missing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("no entry"),
            Self::OtherBuild { written_by } => {
                write!(
                    f,
                    "the entry was stored by runtime build `{written_by}` and not by this one"
                )
            }
            Self::UnsupportedFormat(version) => {
                write!(
                    f,
                    "the entry uses store format {version}, this reader knows {FORMAT_VERSION}"
                )
            }
            Self::Corrupt(reason) => write!(f, "the entry is corrupt: {reason}"),
            Self::Unreadable(error) => write!(f, "the entry cannot be read: {error}"),
        }
    }
}

/// Result of [`ProgramStore::get`].
#[derive(Debug)]
pub enum Lookup {
    Found(StoredProgram),
    Missing(Missing),
}

impl Lookup {
    pub fn found(self) -> Option<StoredProgram> {
        match self {
            Self::Found(program) => Some(program),
            Self::Missing(_) => None,
        }
    }
}

/// A verified entry. The SHA-256 of [`Self::bytes`] equals [`Self::hash`].
pub struct StoredProgram {
    hash: ProgramHash,
    written_by: String,
    file: Vec<u8>,
    payload: Range<usize>,
}

impl StoredProgram {
    pub fn hash(&self) -> ProgramHash {
        self.hash
    }

    /// The runtime build named in the entry header: the build that wrote the
    /// file. Other builds may have stored the same bytes since.
    pub fn written_by(&self) -> &str {
        &self.written_by
    }

    /// The Borsh bytes of the program.
    pub fn bytes(&self) -> &[u8] {
        &self.file[self.payload.clone()]
    }

    pub fn into_bytes(mut self) -> Vec<u8> {
        self.file.truncate(self.payload.end);
        self.file.drain(..self.payload.start);
        self.file
    }
}

impl fmt::Debug for StoredProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The bytes are left out: a program is megabytes long.
        f.debug_struct("StoredProgram")
            .field("hash", &self.hash)
            .field("written_by", &self.written_by)
            .field("len", &self.payload.len())
            .finish_non_exhaustive()
    }
}

/// Result of a `put`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PutOutcome {
    pub hash: ProgramHash,
    /// False when the entry and the build's record of it already existed, so
    /// nothing was written.
    pub written: bool,
}

/// Whether a `put` waits for stable storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Durability {
    /// Flush a file before its rename and the directory after it. A run that
    /// was started from an entry can then still find it after a power loss.
    /// This is the default.
    Full,
    /// Skip the flushes. The rename is still atomic for running processes.
    /// For tests and for stores that can be rebuilt.
    None,
}

/// One entry file seen by [`ProgramStore::entries`]. The entry is not verified.
#[derive(Clone, Debug)]
pub struct EntryInfo {
    pub hash: ProgramHash,
    pub path: PathBuf,
    pub file_len: u64,
    pub modified: Option<SystemTime>,
}

/// Handle to a store directory. Opening performs no I/O, and the directory is
/// created by the first `put`.
#[derive(Clone, Debug)]
pub struct ProgramStore {
    root: PathBuf,
    runtime_build: String,
    durability: Durability,
}

impl ProgramStore {
    /// `runtime_build` identifies the running build, for example
    /// `bex_snapshot::runtime_build()`. Entries that this build did not store
    /// are not served, and every `put` records the build.
    pub fn open(
        root: impl Into<PathBuf>,
        runtime_build: impl Into<String>,
    ) -> Result<Self, StoreError> {
        let runtime_build = runtime_build.into();
        check_build(&runtime_build)?;
        Ok(Self {
            root: root.into(),
            runtime_build,
            durability: Durability::Full,
        })
    }

    #[must_use]
    pub fn with_durability(mut self, durability: Durability) -> Self {
        self.durability = durability;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn runtime_build(&self) -> &str {
        &self.runtime_build
    }

    /// Path of the entry for `hash`. The file may not exist.
    pub fn entry_path(&self, hash: ProgramHash) -> PathBuf {
        let hex = hash.to_hex();
        self.root
            .join(&hex[..2])
            .join(format!("{hex}.{ENTRY_EXTENSION}"))
    }

    /// Path of the marker that records that `build` stored `hash`.
    pub fn marker_path(&self, hash: ProgramHash, build: &str) -> PathBuf {
        let hex = hash.to_hex();
        let tag = ProgramHash::of(build.as_bytes()).to_hex();
        self.root
            .join(&hex[..2])
            .join(format!("{hex}.{}.{MARKER_EXTENSION}", &tag[..16]))
    }

    /// Look up `hash` for this runtime build. The returned bytes are verified.
    pub fn get(&self, hash: ProgramHash) -> Lookup {
        self.read(hash, Some(&self.runtime_build))
    }

    /// Look up `hash` without the build check. For a process that forwards
    /// programs to other sites, and for a collector that verifies entries. A
    /// runtime must not decode the result.
    pub fn get_any_build(&self, hash: ProgramHash) -> Lookup {
        self.read(hash, None)
    }

    fn read(&self, hash: ProgramHash, require_build: Option<&str>) -> Lookup {
        let file = match fs::read(self.entry_path(hash)) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Lookup::Missing(Missing::NotFound);
            }
            Err(error) => return Lookup::Missing(Missing::Unreadable(error)),
        };
        let header = match parse_header(&file) {
            Ok(header) => header,
            Err(missing) => return Lookup::Missing(missing),
        };
        let payload_len = file.len() - header.payload_start;
        if u64::try_from(payload_len).ok() != Some(header.payload_len) {
            return Lookup::Missing(Missing::Corrupt(format!(
                "the header announces {} payload bytes, the file holds {payload_len}",
                header.payload_len
            )));
        }
        let payload = header.payload_start..file.len();
        let actual = ProgramHash::of(&file[payload.clone()]);
        if actual != hash {
            return Lookup::Missing(Missing::Corrupt(format!("the payload hashes to {actual}")));
        }
        if let Some(build) = require_build
            && header.build != build
            && !self.marker_path(hash, build).is_file()
        {
            return Lookup::Missing(Missing::OtherBuild {
                written_by: header.build,
            });
        }
        Lookup::Found(StoredProgram {
            hash,
            written_by: header.build,
            file,
            payload,
        })
    }

    /// Store `program_bytes` under their own hash for this runtime build.
    pub fn put(&self, program_bytes: &[u8]) -> Result<PutOutcome, StoreError> {
        let hash = ProgramHash::of(program_bytes);
        self.put_verified(hash, program_bytes, &self.runtime_build)
    }

    /// Store bytes that are expected to be the program `expected`, for example
    /// bytes received from another site. Bytes with another hash are refused
    /// and nothing is written.
    ///
    /// `runtime_build` is the build that produced the bytes, as the sender
    /// reported it. When it differs from this store's build, the entry is
    /// recorded for the sender's build, and [`Self::get`] keeps reporting it
    /// as missing: bytes of another build must not be decoded by this one.
    pub fn put_received(
        &self,
        expected: ProgramHash,
        program_bytes: &[u8],
        runtime_build: &str,
    ) -> Result<PutOutcome, StoreError> {
        check_build(runtime_build)?;
        let actual = ProgramHash::of(program_bytes);
        if actual != expected {
            return Err(StoreError::HashMismatch { expected, actual });
        }
        self.put_verified(expected, program_bytes, runtime_build)
    }

    fn put_verified(
        &self,
        hash: ProgramHash,
        program_bytes: &[u8],
        build: &str,
    ) -> Result<PutOutcome, StoreError> {
        let entry_path = self.entry_path(hash);
        let marker_path = self.marker_path(hash, build);
        // An entry that this build can already use needs nothing: its header
        // names the build, or the build's marker exists. Such an entry is
        // also accepted on a store that cannot be written, for example a
        // read-only volume that a site server has filled.
        if matches!(self.read(hash, Some(build)), Lookup::Found(_)) {
            return Ok(PutOutcome {
                hash,
                written: false,
            });
        }
        let mut written = false;
        let mut last_error = None;
        for _ in 0..PUT_ATTEMPTS {
            // The marker first. It is only ever created, so the record that
            // this build stored the bytes survives whatever other writers do
            // to the entry afterwards.
            if !marker_path.is_file() {
                match self.write_file(hash, &marker_path, build.as_bytes()) {
                    Ok(()) => written = true,
                    Err(error) => {
                        last_error = Some(error);
                        continue;
                    }
                }
            }
            // A valid entry holds the identical payload, because it hashes to
            // the same value. It is never rewritten.
            if matches!(self.read(hash, None), Lookup::Found(_)) {
                return Ok(PutOutcome { hash, written });
            }
            // Absent or failed verification. A collector that removes an empty
            // shard directory between its creation and the rename, or a rename
            // over a file that another process holds open on Windows, fails
            // this write. The next round checks the entry again.
            match self.write_file(hash, &entry_path, &encode_entry(build, program_bytes)) {
                Ok(()) => written = true,
                Err(error) => last_error = Some(error),
            }
        }
        if marker_path.is_file() && matches!(self.read(hash, None), Lookup::Found(_)) {
            return Ok(PutOutcome { hash, written });
        }
        Err(last_error.unwrap_or_else(|| StoreError::Io {
            action: "keep",
            path: entry_path,
            source: io::Error::other("the entry did not verify after it was written"),
        }))
    }

    /// Write `content` to `path` through a temporary file in the same
    /// directory and a rename.
    fn write_file(&self, hash: ProgramHash, path: &Path, content: &[u8]) -> Result<(), StoreError> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let io_error = |action: &'static str, path: &Path| {
            let path = path.to_path_buf();
            move |source| StoreError::Io {
                action,
                path,
                source,
            }
        };
        let shard = path.parent().expect("a store path has a shard directory");
        fs::create_dir_all(shard).map_err(io_error("create", shard))?;
        // Unique per process and per call. `create_new` turns the remaining
        // collision (a recycled pid that meets a stale file) into an error
        // instead of two writers sharing one file.
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.subsec_nanos());
        let temp = shard.join(format!(
            "{TEMP_PREFIX}{hash}.{}.{}.{nanos}{TEMP_SUFFIX}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
        ));
        let result = (|| {
            let mut file = File::options()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(io_error("create", &temp))?;
            file.write_all(content).map_err(io_error("write", &temp))?;
            if self.durability == Durability::Full {
                // Without this flush a crash after the rename can leave a
                // file whose name exists and whose data does not.
                file.sync_all().map_err(io_error("flush", &temp))?;
            }
            drop(file);
            fs::rename(&temp, path).map_err(io_error("rename to", path))?;
            if self.durability == Durability::Full {
                sync_directory(shard);
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    /// Walk every entry file, shard by shard. The walk holds one directory
    /// handle at a time and no list of entries, so it works on a store of any
    /// size. It is meant for a collector and for diagnostics. No lookup calls
    /// it.
    ///
    /// Build markers, temporary files, files that are not named
    /// `<hash>.bamlprog`, and entries that sit in the wrong shard are skipped.
    /// Entries are not verified. Use [`Self::get_any_build`] for that.
    pub fn entries(&self) -> Entries<'_> {
        Entries {
            store: self,
            next_shard: 0,
            current: None,
        }
    }

    /// Delete the entry for `hash` and its build markers. Returns whether an
    /// entry file was deleted. A reader that already opened the file keeps
    /// reading it.
    pub fn remove(&self, hash: ProgramHash) -> Result<bool, StoreError> {
        let path = self.entry_path(hash);
        let removed = match fs::remove_file(&path) {
            Ok(()) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(source) => {
                return Err(StoreError::Io {
                    action: "remove",
                    path,
                    source,
                });
            }
        };
        // The markers are found by listing the one shard directory. A marker
        // without an entry is harmless, so failures here are not reported.
        let prefix = format!("{hash}.");
        let shard = path.parent().expect("an entry path has a shard directory");
        for file in fs::read_dir(shard).into_iter().flatten().flatten() {
            let name = file.file_name();
            let is_marker = name.to_str().is_some_and(|name| {
                name.starts_with(&prefix) && name.ends_with(&format!(".{MARKER_EXTENSION}"))
            });
            if is_marker {
                let _ = fs::remove_file(file.path());
            }
        }
        Ok(removed)
    }

    /// Delete temporary files of writers that died, when they are older than
    /// `older_than`. Returns the number of deleted files. The age protects the
    /// file of a writer that is still working.
    pub fn remove_stale_temp_files(&self, older_than: Duration) -> usize {
        let now = SystemTime::now();
        let mut removed = 0;
        for shard in 0..=u8::MAX {
            let dir = self.root.join(format!("{shard:02x}"));
            let Ok(files) = fs::read_dir(&dir) else {
                continue;
            };
            for file in files.flatten() {
                let name = file.file_name();
                let Some(name) = name.to_str() else { continue };
                if !(name.starts_with(TEMP_PREFIX) && name.ends_with(TEMP_SUFFIX)) {
                    continue;
                }
                let old = file
                    .metadata()
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|modified| now.duration_since(modified).ok())
                    .is_some_and(|age| age >= older_than);
                if old && fs::remove_file(file.path()).is_ok() {
                    removed += 1;
                }
            }
        }
        removed
    }
}

/// Iterator of [`ProgramStore::entries`].
pub struct Entries<'a> {
    store: &'a ProgramStore,
    next_shard: u16,
    current: Option<(String, fs::ReadDir)>,
}

impl Iterator for Entries<'_> {
    type Item = io::Result<EntryInfo>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let Some((shard, files)) = self.current.as_mut() else {
                if self.next_shard > u16::from(u8::MAX) {
                    return None;
                }
                let shard = format!("{:02x}", self.next_shard);
                self.next_shard += 1;
                match fs::read_dir(self.store.root.join(&shard)) {
                    Ok(files) => self.current = Some((shard, files)),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Some(Err(error)),
                }
                continue;
            };
            let Some(file) = files.next() else {
                self.current = None;
                continue;
            };
            let file = match file {
                Ok(file) => file,
                Err(error) => return Some(Err(error)),
            };
            let path = file.path();
            let hash = path
                .extension()
                .is_some_and(|extension| extension == ENTRY_EXTENSION)
                .then(|| path.file_stem()?.to_str()?.parse::<ProgramHash>().ok())
                .flatten();
            let Some(hash) = hash else { continue };
            if !hash.to_hex().starts_with(shard.as_str()) {
                continue;
            }
            let meta = match file.metadata() {
                Ok(meta) => meta,
                // Removed between the listing and the stat.
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Some(Err(error)),
            };
            return Some(Ok(EntryInfo {
                hash,
                path,
                file_len: meta.len(),
                modified: meta.modified().ok(),
            }));
        }
    }
}

struct Header {
    build: String,
    payload_len: u64,
    payload_start: usize,
}

fn parse_header(file: &[u8]) -> Result<Header, Missing> {
    let corrupt = |reason: &str| Missing::Corrupt(reason.to_string());
    if file.len() < FIXED_PREFIX_LEN {
        return Err(corrupt("the file is shorter than the header"));
    }
    if file[..MAGIC.len()] != MAGIC {
        return Err(corrupt("the file does not start with the store magic"));
    }
    let version = u32::from_le_bytes(file[8..12].try_into().expect("four bytes"));
    if version != FORMAT_VERSION {
        return Err(Missing::UnsupportedFormat(version));
    }
    let build_len = u32::from_le_bytes(file[12..16].try_into().expect("four bytes")) as usize;
    // The bound also keeps the offsets below from overflowing.
    if build_len > MAX_BUILD_LEN {
        return Err(corrupt("the runtime build name is implausibly long"));
    }
    let build_end = FIXED_PREFIX_LEN + build_len;
    let payload_start = build_end + 8;
    if file.len() < payload_start {
        return Err(corrupt("the file ends inside the header"));
    }
    let build = std::str::from_utf8(&file[FIXED_PREFIX_LEN..build_end])
        .map_err(|_| corrupt("the runtime build name is not UTF-8"))?
        .to_string();
    let payload_len = u64::from_le_bytes(
        file[build_end..payload_start]
            .try_into()
            .expect("eight bytes"),
    );
    Ok(Header {
        build,
        payload_len,
        payload_start,
    })
}

fn encode_entry(build: &str, payload: &[u8]) -> Vec<u8> {
    let mut entry = Vec::with_capacity(FIXED_PREFIX_LEN + build.len() + 8 + payload.len());
    entry.extend_from_slice(&MAGIC);
    entry.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    let build_len = u32::try_from(build.len()).expect("bounded by check_build");
    entry.extend_from_slice(&build_len.to_le_bytes());
    entry.extend_from_slice(build.as_bytes());
    entry.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    entry.extend_from_slice(payload);
    entry
}

fn check_build(build: &str) -> Result<(), StoreError> {
    let usable = !build.is_empty() && build.len() <= MAX_BUILD_LEN && !build.contains(['\n', '\r']);
    if usable {
        Ok(())
    } else {
        Err(StoreError::InvalidBuild(build.chars().take(80).collect()))
    }
}

/// Flush a directory so that a rename inside it survives a power loss. Best
/// effort: some file systems refuse it, and Windows has no equivalent that
/// the standard library reaches.
fn sync_directory(dir: &Path) {
    #[cfg(unix)]
    if let Ok(handle) = File::open(dir) {
        let _ = handle.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}
