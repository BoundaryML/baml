//! Content-addressed on-disk cache for compiled bytecode [`Program`]s.
//!
//! Shaped like the Go build cache: there is **no invalidation logic**. Every
//! input that can influence the compiled output — compiler binary, sources,
//! manifest, options — is folded into the [`CacheKey`], so any change simply
//! produces a different key and a miss. Old entries are never consulted again
//! and are eventually removed by [`BytecodeCache::trim`].
//!
//! Correctness therefore rests on two properties:
//! 1. **Key completeness** — every compile input reaches [`KeyInputs`]. Any
//!    new influence on emitted bytecode must be added there.
//! 2. **Emit determinism** — identical inputs produce byte-identical
//!    `Program`s (enforced by `baml_tests/tests/emit_determinism.rs`).
//!
//! A cache problem must never change observable behavior beyond speed: every
//! failure path in [`BytecodeCache::load`] returns `None` (recompile) and
//! [`BytecodeCache::store`] is best-effort.

use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use bex_vm_types::Program;
use sha2::{Digest, Sha256};

/// Bump whenever the serialized `Program` layout or the entry header changes.
/// Part of both the cache key and the entry header.
pub const FORMAT_VERSION: u32 = 1;

const MAGIC: [u8; 4] = *b"BEXC";

/// Fixed-size entry header preceding the borsh payload:
/// magic(4) + `format_version`(4) + key echo(32) + `payload_len`(8) + `payload_sha256`(32)
const HEADER_LEN: usize = 4 + 4 + 32 + 8 + 32;

/// A 256-bit content-addressed cache key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CacheKey([u8; 32]);

impl CacheKey {
    pub fn hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in self.0 {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
        }
        s
    }
}

/// Every input that can influence the compiled `Program`.
///
/// Fields are hashed with length-prefixed framing, so concatenation ambiguity
/// (e.g. path/content boundaries) cannot alias two different input sets.
pub struct KeyInputs<'a> {
    /// Identity of the compiler build — see [`compiler_fingerprint`].
    pub compiler_fingerprint: [u8; 32],
    /// `OptLevel` as a stable discriminant.
    pub opt_level: u8,
    /// `CompileOptions::emit_test_cases`.
    pub emit_test_cases: bool,
    /// `baml.toml` content, if the project has one.
    pub manifest: Option<&'a str>,
    /// `(project-root-relative path, content)` for every source file,
    /// **sorted by path**. Relative paths keep the key location-independent.
    pub files: &'a [(String, &'a str)],
}

pub fn compute_key(inputs: &KeyInputs<'_>) -> CacheKey {
    let mut h = Sha256::new();
    h.update(MAGIC);
    h.update(FORMAT_VERSION.to_le_bytes());
    h.update(inputs.compiler_fingerprint);
    h.update([inputs.opt_level, u8::from(inputs.emit_test_cases)]);
    match inputs.manifest {
        Some(m) => {
            h.update([1u8]);
            h.update((m.len() as u64).to_le_bytes());
            h.update(m.as_bytes());
        }
        None => h.update([0u8]),
    }
    h.update((inputs.files.len() as u64).to_le_bytes());
    debug_assert!(
        inputs.files.windows(2).all(|w| w[0].0 <= w[1].0),
        "KeyInputs::files must be sorted by path"
    );
    for (path, content) in inputs.files {
        h.update((path.len() as u64).to_le_bytes());
        h.update(path.as_bytes());
        h.update((content.len() as u64).to_le_bytes());
        h.update(content.as_bytes());
    }
    CacheKey(h.finalize().into())
}

/// Key for the precompiled stdlib `Program` slice.
///
/// Depends only on the compiler build, opt level, and cache format — not on
/// any project. One entry per compiler build serves every project on the
/// machine (the Go model: the stdlib is compiled once per toolchain, ever).
pub fn stdlib_key(compiler_fingerprint: &[u8; 32], opt_level: u8) -> CacheKey {
    let mut h = Sha256::new();
    h.update(MAGIC);
    h.update(FORMAT_VERSION.to_le_bytes());
    h.update(b"stdlib-slice");
    h.update(compiler_fingerprint);
    h.update([opt_level]);
    CacheKey(h.finalize().into())
}

/// Identity of the running compiler build: SHA-256 of the current executable's
/// bytes, mixed with the stamped product version.
///
/// Hashing the binary itself (rather than trusting a version string) is what
/// makes dev builds safe: two `canary` checkouts both claim "0.13.0" but emit
/// incompatible bytecode. The hash is memoized on disk under `cache_dir`,
/// keyed by the exe's `(len, mtime)`, so the full read happens once per
/// rebuild rather than once per run.
pub fn compiler_fingerprint(cache_dir: &Path) -> [u8; 32] {
    fingerprint_impl(cache_dir).unwrap_or_else(|_| {
        // Can't identify the binary (no exe path, unreadable, ...): return a
        // random-ish value derived from time so caching effectively disables
        // itself rather than serving blobs from an unknown compiler.
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut h = Sha256::new();
        h.update(b"unidentified-compiler");
        h.update(now.to_le_bytes());
        h.finalize().into()
    })
}

fn fingerprint_impl(cache_dir: &Path) -> io::Result<[u8; 32]> {
    let exe = std::env::current_exe()?;
    let meta = fs::metadata(&exe)?;
    let len = meta.len();
    let mtime_ns = meta
        .modified()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    // Memo file named by the hash of the exe path, holding "len mtime hex".
    let mut path_hasher = Sha256::new();
    path_hasher.update(exe.as_os_str().as_encoded_bytes());
    let memo_name: [u8; 32] = path_hasher.finalize().into();
    let memo_path = cache_dir.join("fingerprints").join(hex(&memo_name[..8]));

    if let Ok(memo) = fs::read_to_string(&memo_path) {
        let mut parts = memo.split_whitespace();
        if let (Some(l), Some(m), Some(hx)) = (parts.next(), parts.next(), parts.next())
            && l == len.to_string()
            && m == mtime_ns.to_string()
            && let Some(bytes) = unhex32(hx)
        {
            return Ok(bytes);
        }
    }

    let mut h = Sha256::new();
    h.update(baml_version::CANONICAL_VERSION.as_bytes());
    h.update(baml_version::CHANNEL.as_bytes());
    let mut f = fs::File::open(&exe)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    let digest: [u8; 32] = h.finalize().into();

    // Best-effort memo write; failure just means re-hashing next run.
    if fs::create_dir_all(memo_path.parent().expect("memo path has parent")).is_ok() {
        let _ = write_atomic(
            &memo_path,
            format!("{len} {mtime_ns} {}\n", hex(&digest)).as_bytes(),
        );
    }
    Ok(digest)
}

/// On-disk store: `<dir>/bytecode/<first-two-hex>/<key-hex>.bexc`.
///
/// Content addressing makes a shared directory safe across projects: equal
/// keys imply equal programs, and racing writers converge on identical bytes.
pub struct BytecodeCache {
    dir: PathBuf,
}

impl BytecodeCache {
    pub fn open(dir: PathBuf) -> Self {
        BytecodeCache { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn entry_path(&self, key: &CacheKey) -> PathBuf {
        let hexkey = key.hex();
        self.dir
            .join("bytecode")
            .join(&hexkey[..2])
            .join(format!("{hexkey}.bexc"))
    }

    /// Look up `key`. Any problem — missing entry, torn file, header mismatch,
    /// checksum failure, undecodable payload — is a silent `None`.
    pub fn load(&self, key: &CacheKey) -> Option<Program> {
        let path = self.entry_path(key);
        let data = fs::read(&path).ok()?;
        let payload = check_entry(&data, key)?;
        let program = borsh::from_slice::<Program>(payload).ok()?;
        // Freshen mtime for trim (≥1h granularity keeps inode churn low).
        if let Ok(meta) = fs::metadata(&path)
            && let Ok(modified) = meta.modified()
            && modified
                .elapsed()
                .map(|e| e.as_secs() > 3600)
                .unwrap_or(true)
        {
            let _ = filetime_touch(&path);
        }
        Some(program)
    }

    /// Like [`Self::load`], but returns the raw borsh payload without
    /// deserializing. For byte-level verification against a fresh compile.
    pub fn load_raw(&self, key: &CacheKey) -> Option<Vec<u8>> {
        let data = fs::read(self.entry_path(key)).ok()?;
        let payload = check_entry(&data, key)?;
        Some(payload.to_vec())
    }

    /// Serialize and store `program` under `key`. Best-effort: errors are
    /// returned for optional logging but callers should not fail on them.
    pub fn store(&self, key: &CacheKey, program: &Program) -> io::Result<()> {
        let payload =
            borsh::to_vec(program).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut entry = Vec::with_capacity(HEADER_LEN + payload.len());
        entry.extend_from_slice(&MAGIC);
        entry.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        entry.extend_from_slice(&key.0);
        entry.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        let payload_digest: [u8; 32] = Sha256::digest(&payload).into();
        entry.extend_from_slice(&payload_digest);
        entry.extend_from_slice(&payload);

        // Cargo-style: the cache directory ignores itself, so a project-local
        // cache never shows up as untracked noise or gets committed.
        let gitignore = self.dir.join(".gitignore");
        if !gitignore.exists() {
            let _ = fs::create_dir_all(&self.dir);
            let _ = fs::write(&gitignore, "*\n");
        }

        let path = self.entry_path(key);
        fs::create_dir_all(path.parent().expect("entry path has parent"))?;
        write_atomic(&path, &entry)
    }

    /// Run [`Self::trim`] with the default policy (drop entries unused for
    /// 5 days), at most once per day. Cheap no-op otherwise; call after a
    /// store. The interval marker lives in `<dir>/trim.txt`.
    pub fn maybe_trim(&self) {
        const TRIM_INTERVAL: std::time::Duration = std::time::Duration::from_hours(24);
        const TRIM_AGE: std::time::Duration = std::time::Duration::from_hours(5 * 24);
        let marker = self.dir.join("trim.txt");
        let due = match fs::metadata(&marker).and_then(|m| m.modified()) {
            Ok(modified) => SystemTime::now()
                .duration_since(modified)
                .map(|age| age > TRIM_INTERVAL)
                .unwrap_or(false),
            Err(_) => true,
        };
        if due && fs::create_dir_all(&self.dir).is_ok() && fs::write(&marker, b"").is_ok() {
            let _ = self.trim(TRIM_AGE);
        }
    }

    /// Delete entries not used for `max_age`. Callers should rate-limit
    /// (e.g. at most once per day); this scans the whole store.
    pub fn trim(&self, max_age: std::time::Duration) -> io::Result<()> {
        let bytecode_dir = self.dir.join("bytecode");
        let Ok(fanout) = fs::read_dir(&bytecode_dir) else {
            return Ok(());
        };
        let now = SystemTime::now();
        for sub in fanout.flatten() {
            let Ok(entries) = fs::read_dir(sub.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(meta) = entry.metadata() else { continue };
                let Ok(modified) = meta.modified() else {
                    continue;
                };
                if now
                    .duration_since(modified)
                    .map(|age| age > max_age)
                    .unwrap_or(false)
                {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }
}

/// Validate an entry's header against `key`; return the payload slice.
fn check_entry<'a>(data: &'a [u8], key: &CacheKey) -> Option<&'a [u8]> {
    if data.len() < HEADER_LEN || data[..4] != MAGIC {
        return None;
    }
    let version = u32::from_le_bytes(data[4..8].try_into().ok()?);
    if version != FORMAT_VERSION || data[8..40] != key.0 {
        return None;
    }
    let payload_len = usize::try_from(u64::from_le_bytes(data[40..48].try_into().ok()?)).ok()?;
    let payload = data.get(HEADER_LEN..)?;
    if payload.len() != payload_len {
        return None;
    }
    let digest: [u8; 32] = Sha256::digest(payload).into();
    if digest != data[48..80] {
        return None;
    }
    Some(payload)
}

/// Write via temp file + atomic rename: readers never observe a torn entry,
/// racing writers converge (identical content for identical keys).
fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.flush()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn filetime_touch(path: &Path) -> io::Result<()> {
    // Portable mtime bump without a filetime dep: open for append and set len
    // to itself is unreliable; re-setting times via File::set_modified (Rust
    // 1.75+) is the supported route.
    let f = fs::OpenOptions::new().append(true).open(path)?;
    f.set_modified(SystemTime::now())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn unhex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hi = (chunk[0] as char).to_digit(16)?;
        let lo = (chunk[1] as char).to_digit(16)?;
        out[i] = u8::try_from((hi << 4) | lo).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_inputs<'a>(files: &'a [(String, &'a str)]) -> KeyInputs<'a> {
        KeyInputs {
            compiler_fingerprint: [7u8; 32],
            opt_level: 2,
            emit_test_cases: false,
            manifest: None,
            files,
        }
    }

    #[test]
    fn key_changes_with_each_input() {
        let files = vec![("a.baml".to_string(), "fn x {}")];
        let base = compute_key(&dummy_inputs(&files));

        let files2 = vec![("a.baml".to_string(), "fn y {}")];
        assert_ne!(base, compute_key(&dummy_inputs(&files2)), "content");

        let files3 = vec![("b.baml".to_string(), "fn x {}")];
        assert_ne!(base, compute_key(&dummy_inputs(&files3)), "path");

        let mut inputs = dummy_inputs(&files);
        inputs.opt_level = 0;
        assert_ne!(base, compute_key(&inputs), "opt level");

        let mut inputs = dummy_inputs(&files);
        inputs.emit_test_cases = true;
        assert_ne!(base, compute_key(&inputs), "emit_test_cases");

        let mut inputs = dummy_inputs(&files);
        inputs.manifest = Some("[package]\nname = \"x\"");
        assert_ne!(base, compute_key(&inputs), "manifest");

        let mut inputs = dummy_inputs(&files);
        inputs.compiler_fingerprint = [8u8; 32];
        assert_ne!(base, compute_key(&inputs), "fingerprint");
    }

    #[test]
    fn framing_is_unambiguous() {
        // Same concatenated bytes, different path/content split.
        let a = vec![("ab".to_string(), "c")];
        let b = vec![("a".to_string(), "bc")];
        assert_ne!(
            compute_key(&dummy_inputs(&a)),
            compute_key(&dummy_inputs(&b))
        );
    }

    #[test]
    fn store_load_roundtrip_and_rejections() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = BytecodeCache::open(dir.path().to_path_buf());
        let files = vec![("a.baml".to_string(), "fn x {}")];
        let key = compute_key(&dummy_inputs(&files));

        assert!(cache.load(&key).is_none(), "miss on empty cache");

        let program = Program::default();
        cache.store(&key, &program).expect("store");
        assert!(cache.load(&key).is_some(), "hit after store");

        // Corrupt one payload byte: checksum must reject, silently.
        let path = cache.entry_path(&key);
        let mut bytes = std::fs::read(&path).expect("read entry");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        std::fs::write(&path, &bytes).expect("rewrite");
        assert!(cache.load(&key).is_none(), "corrupt entry rejected");

        // Entry stored under the wrong name: key echo must reject.
        cache.store(&key, &program).expect("re-store");
        let other_files = vec![("b.baml".to_string(), "fn x {}")];
        let other_key = compute_key(&dummy_inputs(&other_files));
        let other_path = cache.entry_path(&other_key);
        std::fs::create_dir_all(other_path.parent().expect("parent")).expect("mkdir");
        std::fs::copy(&path, &other_path).expect("copy");
        assert!(cache.load(&other_key).is_none(), "renamed entry rejected");
    }

    #[test]
    fn trim_removes_only_old_entries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = BytecodeCache::open(dir.path().to_path_buf());
        let files = vec![("a.baml".to_string(), "fn x {}")];
        let key = compute_key(&dummy_inputs(&files));
        cache.store(&key, &Program::default()).expect("store");

        cache
            .trim(std::time::Duration::from_secs(3600))
            .expect("trim");
        assert!(cache.load(&key).is_some(), "fresh entry survives trim");

        cache.trim(std::time::Duration::ZERO).expect("trim all");
        assert!(cache.load(&key).is_none(), "aged entry trimmed");
    }
}
