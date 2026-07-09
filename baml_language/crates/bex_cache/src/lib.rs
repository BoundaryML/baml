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
///
/// This contract also covers the two auxiliary blobs stored under their own
/// keys: bump whenever the [`LinkableImage`](bex_vm_types::LinkableImage) wire
/// format changes, **or** whenever the stdlib-interface blob's wire format
/// changes — i.e. any layout change to
/// `PackageInterface`/`ExportedType`/`ExportedFunction`/`FunctionThrowSets`/
/// `BuiltinKind` or a leaf of the `Ty` family. The compiler fingerprint already
/// invalidates on any binary change, so the version bump is belt-and-braces: it
/// also protects hand-copied blobs via the entry header's version check.
///
/// v4 (B-693 Stage 3): alongside the linked `Program` blob, a compile now also
/// stores a symbolic [`LinkableImage`](bex_vm_types::LinkableImage) under
/// [`image_key`] so an incremental compile can reuse clean files' units. The
/// bump turns every pre-v4 entry into a universal miss (no image blob existed),
/// so `plan_reuse` never finds a manifest without a matching image.
///
/// v5 (per-file diagnostics cache): each [`ManifestFile`] now carries the
/// opaque borsh blob of the diagnostics `check_file` produced for that file, so
/// an incremental compile can serve a clean file's diagnostics without
/// re-checking it. The bump turns every pre-v5 manifest into a miss (the blob
/// did not exist), so `plan_reuse` never reads a manifest lacking the field.
///
/// v6 (Phase 2 interface fragments): every
/// [`CompilationUnit`](bex_vm_types::CompilationUnit) in the
/// [`LinkableImage`](bex_vm_types::LinkableImage) now carries an opaque
/// borsh `interface_fragment` blob (a `FileInterfaceFragment`) beside its
/// `throw_facts`, so a warm compile can project a per-file `callable_throws`
/// seed. The added field changes the image wire format, making every pre-v6
/// image undecodable (`plan_reuse`'s `LinkableImage` decode fails → `None` →
/// full compile), so no pre-v6 entry is ever reused. No hand migration.
pub const FORMAT_VERSION: u32 = 6;

const MAGIC: [u8; 4] = *b"BEXC";

/// Fixed-size entry header preceding the borsh payload:
/// magic(4) + `format_version`(4) + key echo(32) + `payload_len`(8) + `payload_sha256`(32)
const HEADER_LEN: usize = 4 + 4 + 32 + 8 + 32;

/// A 256-bit content-addressed cache key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CacheKey([u8; 32]);

impl CacheKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        CacheKey(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

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
    // Sort defensively rather than trusting the documented precondition: an
    // unsorted caller in a release build would silently produce
    // discovery-order-dependent keys (pure hit-rate loss, never
    // incorrectness — the content is still fully hashed — but with no
    // signal). The sort is O(n log n) over references, negligible next to
    // hashing the file contents.
    let mut ordered: Vec<&(String, &str)> = inputs.files.iter().collect();
    ordered.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    for (path, content) in ordered {
        h.update((path.len() as u64).to_le_bytes());
        h.update(path.as_bytes());
        h.update((content.len() as u64).to_le_bytes());
        h.update(content.as_bytes());
    }
    CacheKey(h.finalize().into())
}

/// Per-project compile manifest: the invalidation state for per-file
/// bytecode reuse, describing the *latest* successful compile.
///
/// Unlike Program blobs (content-addressed, immutable), the manifest lives
/// under a fixed per-project key ([`manifest_key`]) and is overwritten on
/// every successful compile. It records, per file, everything the next
/// compile needs to decide which files are dirty: content hash (did it
/// change at all), signature hash (can the change be observed by other
/// files), defined symbol names, and referenced symbol names (extracted from
/// the compiled bytecode, so desugared references — a `for` loop's
/// `next()` — are included).
#[derive(Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ProjectManifest {
    /// Cache key of the Program blob this manifest describes.
    pub program_key: [u8; 32],
    /// One entry per user file, sorted by `rel_path`.
    pub files: Vec<ManifestFile>,
}

#[derive(Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ManifestFile {
    /// Project-root-relative path (the `Function::source_file` form).
    pub rel_path: String,
    /// Hash of the file's full content.
    pub content_hash: [u8; 32],
    /// Hash of the file's signature (body regions excluded).
    pub signature_hash: [u8; 32],
    /// Last-segment names of symbols this file defines.
    pub defined_names: Vec<String>,
    /// Last-segment names this file's compiled bytecode references.
    pub referenced_names: Vec<String>,
    /// Throw-analysis facts for every function the file defines, exactly as
    /// `baml_compiler2_tir::throw_inference::file_throw_facts` extracted
    /// them. Re-seeded into the next compile's database so unchanged files
    /// never re-walk their bodies just to answer "what does the package
    /// throw" — the package-level solve then runs from facts alone.
    pub throw_facts: Vec<baml_type::throw_facts::FunctionThrowFacts>,
    /// Opaque borsh blob of the diagnostics `check_file` produced for this
    /// file on the compile that wrote the manifest (`borsh(Vec<CachedDiagnostic>)`,
    /// the typed form living in the CLI). Kept opaque here so `bex_cache` does
    /// not depend on the diagnostics crate — same pattern as the stdlib
    /// interface blob. Because the manifest is written only after a passing
    /// error gate, a clean file's blob holds warnings / info only.
    pub diagnostics: Vec<u8>,
}

/// Fixed per-project key for the [`ProjectManifest`].
///
/// Keyed by compiler fingerprint + opt + options + the project root path:
/// a different compiler build gets a fresh manifest (its previous Program
/// would not be relink-compatible), and two checkouts of the same project
/// don't fight over one entry.
pub fn manifest_key(
    compiler_fingerprint: &[u8; 32],
    opt_level: u8,
    emit_test_cases: bool,
    project_root: &Path,
    project_manifest_toml: Option<&str>,
) -> CacheKey {
    let mut h = Sha256::new();
    h.update(MAGIC);
    h.update(FORMAT_VERSION.to_le_bytes());
    h.update(b"project-manifest");
    h.update(compiler_fingerprint);
    h.update([opt_level, u8::from(emit_test_cases)]);
    let root = project_root.as_os_str().as_encoded_bytes();
    h.update((root.len() as u64).to_le_bytes());
    h.update(root);
    // baml.toml is a compile input of the program key, so it must gate the
    // manifest too: a config-only change must not let plan_reuse splice
    // every source file against stale project configuration.
    match project_manifest_toml {
        Some(m) => {
            h.update([1u8]);
            h.update((m.len() as u64).to_le_bytes());
            h.update(m.as_bytes());
        }
        None => h.update([0u8]),
    }
    CacheKey(h.finalize().into())
}

/// Key for the symbolic [`LinkableImage`](bex_vm_types::LinkableImage) that
/// accompanies a `Program` blob (B-693 Stage 3).
///
/// Derived deterministically from the Program blob's own key, so the store side
/// (which knows `self.key`) and the reuse side (which reads `program_key` from
/// the previous manifest) agree without threading a second key through the
/// manifest. Distinct from the `Program` key so the two payloads never collide.
pub fn image_key(program_key: &[u8; 32]) -> CacheKey {
    let mut h = Sha256::new();
    h.update(MAGIC);
    h.update(FORMAT_VERSION.to_le_bytes());
    h.update(b"linkable-image");
    h.update(program_key);
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

/// Key for the cached stdlib **typed interface** blob (B-694 "export data").
///
/// The stored value is `borsh(BTreeMap<package-name, borsh(PackageInterface)>)`
/// for the six stdlib packages. Like [`stdlib_key`], it depends only on the
/// compiler build + opt level: the stdlib is a build constant (no user file
/// contributes to a stdlib package), so one entry per compiler build serves
/// every project on the machine. The `b"stdlib-interface"` domain separator
/// prevents collision with the stdlib bytecode slice ([`stdlib_key`]), which
/// shares the same `(fingerprint, opt_level)` inputs. `opt_level` is included
/// for parity even though the interface itself is opt-invariant.
pub fn stdlib_interface_key(compiler_fingerprint: &[u8; 32], opt_level: u8) -> CacheKey {
    let mut h = Sha256::new();
    h.update(MAGIC);
    h.update(FORMAT_VERSION.to_le_bytes());
    h.update(b"stdlib-interface");
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
/// keyed by the exe's `(len, mtime, ctime)` — the `ctime` (inode-change time)
/// component is unix-only and is what defeats mtime-preserving restores
/// (`cp -p`, tar, CI cache restore) that would otherwise replay a stale hash
/// for a different build. The full read still happens only once per rebuild.
/// Residual gap: non-unix targets fall back to the `(len, mtime)` key.
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
    // ctime (inode-change time) is set to "now" by a copy even when the copier
    // preserves mtime (`cp -p`, tar, CI cache restore), so folding it into the
    // memo key stops a restored-but-different exe with a matching (len, mtime)
    // from replaying a stale fingerprint. Unix-only; see `exe_ctime`.
    let ctime = exe_ctime(&meta);

    // Memo file named by the hash of the exe path, holding
    // "len mtime ctime hex".
    let mut path_hasher = Sha256::new();
    path_hasher.update(exe.as_os_str().as_encoded_bytes());
    let memo_name: [u8; 32] = path_hasher.finalize().into();
    let memo_path = cache_dir.join("fingerprints").join(hex(&memo_name[..8]));

    if let Ok(memo) = fs::read_to_string(&memo_path) {
        let mut parts = memo.split_whitespace();
        if let (Some(l), Some(m), Some(c), Some(hx)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
            && l == len.to_string()
            && m == mtime_ns.to_string()
            && c == ctime
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
            format!("{len} {mtime_ns} {ctime} {}\n", hex(&digest)).as_bytes(),
        );
    }
    Ok(digest)
}

/// The exe's inode-change time as a `"secs.nsecs"` token for the fingerprint
/// memo key. On unix this reads `MetadataExt::ctime` / `ctime_nsec`; other
/// targets return a fixed placeholder so the memo degrades to the
/// `(len, mtime)` key.
fn exe_ctime(meta: &fs::Metadata) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        format!("{}.{}", meta.ctime(), meta.ctime_nsec())
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        String::from("na")
    }
}

/// On-disk store: `<dir>/bytecode/<first-two-hex>/<key-hex>.bexc`.
///
/// Content addressing makes a shared directory safe across projects: equal
/// keys imply equal programs, and racing writers converge on identical bytes.
pub struct BytecodeCache {
    dir: PathBuf,
    /// Optional shared backend for immutable entries (see [`RemoteCache`]).
    remote: Option<RemoteCache>,
}

impl BytecodeCache {
    pub fn open(dir: PathBuf) -> Self {
        BytecodeCache { dir, remote: None }
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
    /// deserializing. For byte-level verification against a fresh compile,
    /// and for non-Program entries (the project manifest).
    pub fn load_raw(&self, key: &CacheKey) -> Option<Vec<u8>> {
        let data = fs::read(self.entry_path(key)).ok()?;
        let payload = check_entry(&data, key)?;
        Some(payload.to_vec())
    }

    /// Store an arbitrary payload under `key` with the standard entry header
    /// (same integrity checks as Program entries). Used for the project
    /// manifest, which is overwritten in place on every successful compile.
    pub fn store_raw(&self, key: &CacheKey, payload: &[u8]) -> io::Result<()> {
        // Cargo-style: the cache directory ignores itself, so a project-local
        // cache never shows up as untracked noise or gets committed.
        let gitignore = self.dir.join(".gitignore");
        if !gitignore.exists() {
            let _ = fs::create_dir_all(&self.dir);
            let _ = fs::write(&gitignore, "*\n");
        }

        let mut entry = Vec::with_capacity(HEADER_LEN + payload.len());
        entry.extend_from_slice(&MAGIC);
        entry.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        entry.extend_from_slice(&key.0);
        entry.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        let payload_digest: [u8; 32] = Sha256::digest(payload).into();
        entry.extend_from_slice(&payload_digest);
        entry.extend_from_slice(payload);

        let path = self.entry_path(key);
        fs::create_dir_all(path.parent().expect("entry path has parent"))?;
        write_atomic(&path, &entry)
    }

    /// Serialize and store `program` under `key`. Best-effort: errors are
    /// returned for optional logging but callers should not fail on them.
    pub fn store(&self, key: &CacheKey, program: &Program) -> io::Result<()> {
        let payload =
            borsh::to_vec(program).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        self.store_raw(key, &payload)
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
    // Unique per process AND per call: a pid-only suffix would let two
    // same-process stores (e.g. concurrent compiles in a threaded host)
    // truncate each other's temp file mid-write. `create_new` guards the
    // residual cross-process collision case by failing instead of clobbering.
    static WRITE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = WRITE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp.{}.{unique}", std::process::id()));
    let result = (|| {
        let mut f = fs::File::options()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
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
    fn stdlib_interface_key_is_distinct_and_input_sensitive() {
        let fp = [3u8; 32];
        let base = stdlib_interface_key(&fp, 2);

        // Never collides with the bytecode slice under the same inputs (distinct
        // domain separator).
        assert_ne!(
            base,
            stdlib_key(&fp, 2),
            "interface key must not collide with the bytecode-slice key"
        );
        // Sensitive to both keying inputs.
        assert_ne!(base, stdlib_interface_key(&[4u8; 32], 2), "fingerprint");
        assert_ne!(base, stdlib_interface_key(&fp, 0), "opt level");
    }

    #[test]
    fn stdlib_interface_blob_round_trips_through_store_raw() {
        use std::collections::BTreeMap;

        let dir = tempfile::tempdir().expect("tempdir");
        let cache = BytecodeCache::open(dir.path().to_path_buf());
        let key = stdlib_interface_key(&[5u8; 32], 2);

        // Shape matches the CLI blob: package-name -> opaque per-package bytes.
        let mut blob: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        blob.insert("baml".to_string(), vec![1, 2, 3, 4]);
        blob.insert("log".to_string(), vec![9, 8, 7]);
        let payload = borsh::to_vec(&blob).expect("serialize blob");

        assert!(cache.load_raw(&key).is_none(), "miss on empty cache");
        cache.store_raw(&key, &payload).expect("store");
        let loaded = cache.load_raw(&key).expect("hit after store");
        let decoded: BTreeMap<String, Vec<u8>> = borsh::from_slice(&loaded).expect("decode blob");
        assert_eq!(decoded, blob, "interface blob survives the store/load path");
    }

    #[test]
    fn manifest_with_diagnostics_blob_round_trips_through_store_raw() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = BytecodeCache::open(dir.path().to_path_buf());
        let key = manifest_key(&[6u8; 32], 2, false, Path::new("/proj"), None);

        let manifest = ProjectManifest {
            program_key: [1u8; 32],
            files: vec![ManifestFile {
                rel_path: "a.baml".to_string(),
                content_hash: [2u8; 32],
                signature_hash: [3u8; 32],
                defined_names: vec!["foo".to_string()],
                referenced_names: vec!["Bar".to_string()],
                throw_facts: Vec::new(),
                // Opaque bytes: bex_cache never interprets this blob.
                diagnostics: vec![9, 8, 7, 6],
            }],
        };
        let payload = borsh::to_vec(&manifest).expect("serialize manifest");

        cache.store_raw(&key, &payload).expect("store");
        let loaded = cache.load_raw(&key).expect("hit after store");
        let decoded: ProjectManifest = borsh::from_slice(&loaded).expect("decode manifest");
        assert_eq!(decoded.files.len(), 1);
        assert_eq!(decoded.files[0].diagnostics, vec![9, 8, 7, 6]);
        assert_eq!(decoded.files[0].rel_path, "a.baml");
    }

    #[test]
    fn pre_v5_manifest_payload_is_a_miss() {
        // A manifest serialized without the `diagnostics` field (the pre-v5
        // layout) must not decode into the current `ProjectManifest`, so an old
        // cache degrades to a full compile rather than a stale-diagnostics
        // serve. Emulate the old layout by borsh-encoding a struct missing the
        // trailing field and confirming it fails to round-trip.
        #[derive(borsh::BorshSerialize)]
        struct LegacyManifestFile {
            rel_path: String,
            content_hash: [u8; 32],
            signature_hash: [u8; 32],
            defined_names: Vec<String>,
            referenced_names: Vec<String>,
            throw_facts: Vec<baml_type::throw_facts::FunctionThrowFacts>,
        }
        #[derive(borsh::BorshSerialize)]
        struct LegacyManifest {
            program_key: [u8; 32],
            files: Vec<LegacyManifestFile>,
        }
        let legacy = LegacyManifest {
            program_key: [0u8; 32],
            files: vec![LegacyManifestFile {
                rel_path: "a.baml".to_string(),
                content_hash: [0u8; 32],
                signature_hash: [0u8; 32],
                defined_names: Vec::new(),
                referenced_names: Vec::new(),
                throw_facts: Vec::new(),
            }],
        };
        let bytes = borsh::to_vec(&legacy).expect("serialize legacy");
        // Borsh is not self-describing: the missing trailing `diagnostics` Vec
        // makes the decode run off the end of the buffer.
        assert!(
            borsh::from_slice::<ProjectManifest>(&bytes).is_err(),
            "a pre-v5 manifest layout must fail to decode into the v5 struct"
        );
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

// ─── Remote (distributed) cache ─────────────────────────────────────────────

/// Read-through / write-through remote backend for shared caches.
///
/// Content addressing is what makes distribution trivial: every shared entry
/// is an immutable blob under a key that already encodes the compiler build
/// and all inputs, so the remote needs no invalidation protocol, no locking,
/// and no coordination — any HTTP store with GET/PUT semantics works (an S3
/// proxy, nginx + `WebDAV`, a CI artifact shim). Typical deployment: CI fills
/// the cache, developer machines read it. This mirrors Go's `GOCACHEPROG`
/// split of "cache policy in the tool, storage anywhere".
///
/// Configured via `BAML_CACHE_REMOTE=<base-url>` (entries live at
/// `<base-url>/<key-hex>`) and optional `BAML_CACHE_REMOTE_TOKEN` (sent as a
/// bearer token). Only immutable entries — Program blobs and stdlib slices —
/// are shared; the per-project manifest is a mutable local-latest pointer and
/// deliberately stays local (it only accelerates the local edit loop).
///
/// Strictly best-effort: a slow or broken remote degrades to local-only
/// behavior. Downloaded entries pass the same header/checksum validation as
/// local ones before use, and are re-persisted locally so the remote is hit
/// at most once per entry.
pub struct RemoteCache {
    base_url: String,
    token: Option<String>,
    client: reqwest::blocking::Client,
}

/// Whether a bearer token may be attached to `base_url`: true over https, or
/// to a real http loopback host (`localhost`, `127.0.0.1`, `::1`). The host is
/// taken from a parsed URL, so `host_str` drops any userinfo and port — a
/// look-alike authority such as `localhost.evil.com` or `127.0.0.1@evil.com`
/// resolves to a non-loopback host and is correctly rejected, closing the
/// cleartext-token leak those inputs would otherwise cause.
fn token_allowed(base_url: &str) -> bool {
    if base_url.starts_with("https://") {
        return true;
    }
    reqwest::Url::parse(base_url).is_ok_and(|u| {
        u.scheme() == "http" && matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
    })
}

impl RemoteCache {
    /// Build from `BAML_CACHE_REMOTE` / `BAML_CACHE_REMOTE_TOKEN`; `None`
    /// when unset or the HTTP client cannot be constructed.
    pub fn from_env() -> Option<RemoteCache> {
        let base_url = std::env::var("BAML_CACHE_REMOTE").ok()?;
        if base_url.is_empty() {
            return None;
        }
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(2))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .ok()?;
        let base_url = base_url.trim_end_matches('/').to_string();
        let mut token = std::env::var("BAML_CACHE_REMOTE_TOKEN").ok();
        // Never send a bearer token in cleartext: attach it only over https or
        // to a genuine http loopback host. `token_allowed` matches the host of
        // a parsed URL, so look-alikes (`http://localhost.evil.com`) and
        // userinfo tricks (`http://127.0.0.1@evil.com`) are treated as remote
        // and drop the token.
        if token.is_some() && !token_allowed(&base_url) {
            #[allow(clippy::print_stderr)] // security misconfiguration warning
            {
                eprintln!(
                    "warning: BAML_CACHE_REMOTE_TOKEN ignored — remote cache URL is not \
                     https, refusing to send the token in cleartext"
                );
            }
            token = None;
        }
        Some(RemoteCache {
            base_url,
            token,
            client,
        })
    }

    fn entry_url(&self, key: &CacheKey) -> String {
        format!("{}/{}", self.base_url, key.hex())
    }

    /// Hard ceiling on a downloaded entry body. Without it, reading the body of
    /// a hostile or broken remote could stream unbounded and OOM the compiler.
    /// Real bytecode blobs are far smaller than this; 256 MiB.
    const REMOTE_MAX_BYTES: u64 = 256 * 1024 * 1024;

    fn get(&self, key: &CacheKey) -> Option<Vec<u8>> {
        let mut request = self.client.get(self.entry_url(key));
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request.send().ok()?;
        if !response.status().is_success() {
            return None;
        }
        // Reject early if the advertised length already exceeds the cap.
        if response
            .content_length()
            .is_some_and(|n| n > Self::REMOTE_MAX_BYTES)
        {
            return None;
        }
        // Then bound the actual read: `Content-Length` is advisory, so a lying
        // or chunked response is still capped here. Read one byte past the cap
        // so an over-limit body is rejected rather than silently truncated.
        let mut body = Vec::new();
        if response
            .take(Self::REMOTE_MAX_BYTES + 1)
            .read_to_end(&mut body)
            .is_err()
        {
            return None;
        }
        if body.len() as u64 > Self::REMOTE_MAX_BYTES {
            return None;
        }
        Some(body)
    }

    fn put(&self, key: &CacheKey, entry: &[u8]) {
        let mut request = self
            .client
            .put(self.entry_url(key))
            .header("content-type", "application/octet-stream")
            .body(entry.to_vec());
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        // Best-effort: sharing is an optimization, never a failure mode.
        let _ = request.send();
    }
}

impl BytecodeCache {
    /// Attach the env-configured remote backend, if any.
    #[must_use]
    pub fn with_remote_from_env(mut self) -> Self {
        self.remote = RemoteCache::from_env();
        self
    }

    /// Like [`Self::load`], but on a local miss consults the remote backend:
    /// a downloaded entry is validated exactly like a local one (magic,
    /// version, key echo, payload checksum) and persisted locally before use.
    pub fn load_shared(&self, key: &CacheKey) -> Option<Program> {
        if let Some(program) = self.load(key) {
            return Some(program);
        }
        let remote = self.remote.as_ref()?;
        let entry = remote.get(key)?;
        // SECURITY: `check_entry` is integrity-only — the payload SHA lives in
        // the entry itself, so it catches corruption/truncation but not a
        // forged blob. A remote that chooses the bytes (a malicious origin, or
        // a MITM on an untrusted mirror) can serve a valid-looking executable
        // `Program`. We therefore assume a trusted remote reached over https.
        // Authenticating entries — e.g. an HMAC over the payload keyed by the
        // shared token, verified here — is a follow-up (see report).
        let payload = check_entry(&entry, key)?;
        let program = borsh::from_slice::<Program>(payload).ok()?;
        let path = self.entry_path(key);
        if let Some(parent) = path.parent()
            && fs::create_dir_all(parent).is_ok()
        {
            let _ = write_atomic(&path, &entry);
        }
        Some(program)
    }

    /// Like [`Self::store`], but also publishes the entry to the remote
    /// backend (best-effort) so other machines can hit it.
    pub fn store_shared(&self, key: &CacheKey, program: &Program) -> io::Result<()> {
        self.store(key, program)?;
        if let Some(remote) = &self.remote
            && let Ok(entry) = fs::read(self.entry_path(key))
        {
            remote.put(key, &entry);
        }
        Ok(())
    }
}

#[cfg(test)]
mod remote_tests {
    use std::{
        io::{BufRead, BufReader, Read as _, Write as _},
        net::TcpListener,
        sync::{Arc, Mutex},
    };

    use super::*;

    /// Minimal in-process HTTP store: GET serves stored bodies, PUT stores
    /// them. Std-only so the test needs no async runtime.
    fn spawn_http_store() -> (String, Arc<Mutex<HashMapStore>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let store: Arc<Mutex<HashMapStore>> = Arc::default();
        let server_store = Arc::clone(&store);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() {
                    continue;
                }
                let mut parts = request_line.split_whitespace();
                let (method, path) = (
                    parts.next().unwrap_or("").to_string(),
                    parts.next().unwrap_or("").to_string(),
                );
                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        content_length = v.trim().parse().unwrap_or(0);
                    }
                }
                match method.as_str() {
                    "PUT" => {
                        let mut body = vec![0u8; content_length];
                        if reader.read_exact(&mut body).is_ok() {
                            server_store.lock().expect("lock").0.insert(path, body);
                        }
                        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n");
                    }
                    "GET" => match server_store.lock().expect("lock").0.get(&path) {
                        Some(body) => {
                            let _ = stream.write_all(
                                format!(
                                    "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n",
                                    body.len()
                                )
                                .as_bytes(),
                            );
                            let _ = stream.write_all(body);
                        }
                        None => {
                            let _ = stream
                                .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n");
                        }
                    },
                    _ => {
                        let _ = stream.write_all(
                            b"HTTP/1.1 405 Method Not Allowed\r\ncontent-length: 0\r\n\r\n",
                        );
                    }
                }
            }
        });
        (format!("http://{addr}"), store)
    }

    #[derive(Default)]
    struct HashMapStore(std::collections::HashMap<String, Vec<u8>>);

    fn remote_for(base_url: &str) -> RemoteCache {
        RemoteCache {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: None,
            client: reqwest::blocking::Client::new(),
        }
    }

    #[test]
    fn shared_store_publishes_and_shared_load_populates_local() {
        let (url, store) = spawn_http_store();
        let program = Program::default();
        let files = vec![("a.baml".to_string(), "fn x {}")];
        let key = compute_key(&KeyInputs {
            compiler_fingerprint: [9u8; 32],
            opt_level: 2,
            emit_test_cases: false,
            manifest: None,
            files: &files,
        });

        // Machine A: store publishes to the remote.
        let dir_a = tempfile::tempdir().expect("tempdir");
        let mut cache_a = BytecodeCache::open(dir_a.path().to_path_buf());
        cache_a.remote = Some(remote_for(&url));
        cache_a.store_shared(&key, &program).expect("store");
        assert_eq!(
            store.lock().expect("lock").0.len(),
            1,
            "entry published to remote"
        );

        // Machine B: local miss, remote hit, local populated.
        let dir_b = tempfile::tempdir().expect("tempdir");
        let mut cache_b = BytecodeCache::open(dir_b.path().to_path_buf());
        assert!(cache_b.load(&key).is_none(), "no local entry on machine B");
        cache_b.remote = Some(remote_for(&url));
        assert!(cache_b.load_shared(&key).is_some(), "served from remote");
        assert!(
            cache_b.load(&key).is_some(),
            "local cache populated from remote"
        );

        // A corrupted remote entry is rejected by validation.
        let key2 = compute_key(&KeyInputs {
            compiler_fingerprint: [10u8; 32],
            opt_level: 2,
            emit_test_cases: false,
            manifest: None,
            files: &files,
        });
        store
            .lock()
            .expect("lock")
            .0
            .insert(format!("/{}", key2.hex()), b"garbage".to_vec());
        assert!(
            cache_b.load_shared(&key2).is_none(),
            "corrupt remote entry rejected"
        );
    }

    #[test]
    fn token_attached_only_over_https_or_loopback() {
        assert!(
            token_allowed("https://cache.example.com"),
            "https keeps token"
        );
        assert!(
            !token_allowed("http://cache.example.com"),
            "plain http drops token"
        );
        assert!(
            token_allowed("http://localhost:8080"),
            "http loopback keeps token"
        );
        assert!(
            !token_allowed("http://localhost.evil.com"),
            "look-alike host drops token"
        );
        assert!(
            !token_allowed("http://127.0.0.1@evil.com"),
            "userinfo trick drops token"
        );
    }
}
