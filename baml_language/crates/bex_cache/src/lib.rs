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

use bex_vm_types::{CompilationUnit, Program};
use sha2::{Digest, Sha256};

/// Bump whenever the serialized `Program` layout or the entry header changes.
/// Part of both the cache key and the entry header.
///
/// This contract also covers auxiliary blobs stored under their own keys: bump
/// whenever the [`CompilationUnit`] wire format changes, **or** whenever the
/// stdlib-interface blob's wire format changes — i.e. any layout change to
/// `PackageInterface`/`ExportedType`/`ExportedFunction`/`FunctionThrowSets`/
/// `BuiltinKind` or a leaf of the `Ty` family. The compiler fingerprint already
/// invalidates on any binary change, so the version bump is belt-and-braces: it
/// also protects hand-copied blobs via the entry header's version check.
///
/// Version 1 is the initial shipped format: content-addressed entries with
/// the 80-byte validated header, independently content-addressed
/// [`CompilationUnit`] entries referenced by per-file `unit_key` pointers in
/// the manifest, per-file diagnostics blobs, the layout/signature hash pair,
/// `sig_referenced_names`, and the stdlib bytecode + typed-interface blobs.
/// Any bump turns every older entry into a miss via the header version check —
/// there is never a hand-written migration.
///
/// Version 2: `Function` gained the borsh-serialized `docstring` field
/// (BEP-062 `reflect.signature`).
///
/// Version 3: diagnostic cache blobs gained `message_highlights` fields.
///
/// Version 4: `DiagnosticId` dropped the legacy `TypeBuilder` variants
/// (E0040–E0043, BEP-066 removal), shifting the borsh discriminants of all
/// later variants.
///
/// Version 5: `FunctionMeta::Llm` removed the Borsh-serialized
/// `prompt_template` field.
///
/// Version 6: `Enum` and `InterfaceDef` gained a borsh-serialized `type_tag`,
/// so every declaration that can head a nominal type now carries its identity
/// (previously only `Class` did). `Class::type_tag` changed from `i64` to the
/// `TypeTag` newtype, which is wire-identical — that part needed no bump.
/// Also: type aliases became `Object::TypeAlias` declarations, so `Package` /
/// `ProgramPackage` replaced the inline `recursive_type_aliases` map with a
/// `type_aliases` map of references to them. And `InterfaceMethodDef` replaced
/// the (never-populated) `default_fqn: Option<String>` with `default:
/// Option<ObjectIndex>` — a default method's pooled body, relocated by the
/// linker like any other object operand and bound to a pointer at load, so
/// runtime implementors inherit defaults without naming anything.
///
/// Version 7: every type-bearing wire field is head-inline rather than
/// name-headed, and the bytecode gained `OpCode::Truthy` plus
/// `ConstValue::Literal`. Both landed while 6 was current — on different
/// branches — so a 6 image can be either shape and neither can be told from
/// the other. The bump is what makes the ambiguity unreachable.
///
/// Version 8: `Class.name` / `Enum.name` are `DeclarationName` (an enum over
/// declared and anonymous), so both gained a leading discriminant byte. A 7
/// image would decode that discriminant out of the old `TypeName` bytes.
///
/// Version 9: the bytecode gained `OpCode::MakeVirtualFunction` (appended, so
/// discriminants did not shift), which landed on canary while 8 was current on
/// a branch — the same both-sides-of-a-merge ambiguity version 7 records. An
/// 8 image without the opcode would decode fine, but one version must mean one
/// format.
///
/// Version 10: the `Ty` wire format retired the TIR-era compiler-only variants
/// — `EvolvingList`/`EvolvingMap` (31/32) and the `Unknown` error-recovery
/// sentinel (29). The slots are tombstoned rather than reused, so a 9 image
/// carrying one in a persisted throw fact would now fail to decode instead of
/// silently landing on whatever occupies the slot later.
///
/// Version 11: the interface-dispatch rework, described as one net change
/// against the format canary shipped (a bump only matters against that;
/// intermediate shapes that never left the branch are not versions).
/// Interface bodies became anonymous-but-slotted: `Function` gained
/// `is_interface_body` + `native_key` (wire fields), and `function_indices` /
/// `function_global_indices` stopped carrying impl-block methods and
/// interface default bodies. Impl-rule method tables became PROVIDED-ONLY
/// (an adopted interface default resolves at dispatch through the
/// interface's `default_fn`, never through a baked row), rule fragments
/// moved onto their declaring unit, and `ProgramMethodImplFrag` carries the
/// body's code-bucket offset in that unit rather than a name. A body has no
/// name-keyed coordinates on the PROGRAM wire — its spelling survives only
/// as the `CompilationUnit`'s link-internal export/import key; compile
/// boundaries read coordinates from the declaration-keyed placement
/// registry, with a Pass-1 slot replay only at the stdlib-splice boundary.
pub const FORMAT_VERSION: u32 = 11;

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
        hex(&self.0)
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
    /// `baml.toml` content, if the project has one.
    pub manifest: Option<&'a str>,
    /// `(project-root-relative path, content)` for every source file,
    /// **sorted by path**. Relative paths keep the key location-independent.
    pub files: &'a [(String, &'a str)],
}

/// Seed a hasher with the cache-wide header prefix (magic + format version)
/// and a `domain` separator, so entries of different kinds that share the same
/// keying inputs land under distinct keys.
fn keyed_hasher(domain: &[u8]) -> Sha256 {
    let mut h = Sha256::new();
    h.update(MAGIC);
    h.update(FORMAT_VERSION.to_le_bytes());
    h.update(domain);
    h
}

/// Hash `bytes` behind a u64 little-endian length prefix. The prefix is what
/// keeps adjacent variable-length fields from aliasing across their boundary.
fn update_framed(h: &mut Sha256, bytes: &[u8]) {
    h.update((bytes.len() as u64).to_le_bytes());
    h.update(bytes);
}

/// Hash an optional string as a 1-byte present/absent tag followed, when
/// present, by the length-framed bytes.
fn hash_opt_str(h: &mut Sha256, value: Option<&str>) {
    match value {
        Some(v) => {
            h.update([1u8]);
            update_framed(h, v.as_bytes());
        }
        None => h.update([0u8]),
    }
}

/// Cache key for an entry keyed solely by compiler build + opt level under
/// `domain`. The stdlib bytecode slice, typed interface, and builtin
/// diagnostics share this shape and are kept distinct by their `domain`.
fn fingerprint_opt_key(domain: &[u8], compiler_fingerprint: &[u8; 32], opt_level: u8) -> CacheKey {
    let mut h = keyed_hasher(domain);
    h.update(compiler_fingerprint);
    h.update([opt_level]);
    CacheKey(h.finalize().into())
}

pub fn compute_key(inputs: &KeyInputs<'_>) -> CacheKey {
    let mut h = Sha256::new();
    h.update(MAGIC);
    h.update(FORMAT_VERSION.to_le_bytes());
    h.update(inputs.compiler_fingerprint);
    h.update([inputs.opt_level]);
    hash_opt_str(&mut h, inputs.manifest);
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
        update_framed(&mut h, path.as_bytes());
        update_framed(&mut h, content.as_bytes());
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
    /// Hash of the file's type-*layout* surface: its class/enum/interface/alias
    /// and out-of-body `implements` declarations, with function bodies blanked
    /// (exactly the surface that can move a field offset, an enum discriminant,
    /// a type tag, or a vtable slot baked into *other* files' bytecode). A file
    /// with no such declarations hashes to a fixed constant. Function signatures
    /// never contribute, so a function-only signature edit in a type-defining
    /// file leaves this stable — the input the dirty-set pass uses to fire the
    /// layout sentinel only when a type's layout actually changed.
    pub layout_hash: [u8; 32],
    /// Last-segment names of symbols this file defines.
    pub defined_names: Vec<String>,
    /// Last-segment names this file's compiled bytecode references.
    pub referenced_names: Vec<String>,
    /// Last-segment type names named in this file's *signature* surface only —
    /// function/method params, returns, `throws`, generic bounds; class/interface
    /// field types; `implements`/`requires`/`for` targets; type-alias RHS — i.e.
    /// exactly the bodies-blanked source `signature_hash` hashes. A strict subset
    /// of `referenced_names` (which also folds in body-level bytecode references).
    /// The dirty-set pass uses it to cascade a type-identity change through a
    /// content-unchanged callee whose resolved signature moved: a file whose
    /// *signature* names a changed type has its own resolved signature meaning
    /// changed, so its defined names propagate to its callers, whereas a purely
    /// body-level reference does not (that can only move the file's throws, which
    /// the transitive throws-taint closure already covers).
    pub sig_referenced_names: Vec<String>,
    /// Throw-analysis facts for every function the file defines, exactly as
    /// `baml_compiler2_hir_ty::throw_facts::file_throw_facts` extracted
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
    /// Opaque borsh blob of this file's `CallableThrowsFragment` — a verbatim
    /// copy of the same bytes stored in the file's [`CompilationUnit`]. Kept
    /// in the manifest so the warm path can seed per-function
    /// `callable_throws` without reading any unit payloads (the units are
    /// only needed when bytecode is actually relinked). Empty when the file
    /// contributes no fragment.
    pub callable_throws_fragment: Vec<u8>,
    /// Content-addressed cache key of this file's serialized [`CompilationUnit`];
    /// the manifest is the sole mutable pointer table for the immutable per-file
    /// unit entries.
    pub unit_key: [u8; 32],
}

/// Fixed per-project key for the [`ProjectManifest`].
///
/// Keyed by compiler fingerprint + opt level + the project root path:
/// a different compiler build gets a fresh manifest (its previous Program
/// would not be relink-compatible), and two checkouts of the same project
/// don't fight over one entry.
pub fn manifest_key(
    compiler_fingerprint: &[u8; 32],
    opt_level: u8,
    project_root: &Path,
    project_manifest_toml: Option<&str>,
) -> CacheKey {
    let mut h = keyed_hasher(b"project-manifest");
    h.update(compiler_fingerprint);
    h.update([opt_level]);
    update_framed(&mut h, project_root.as_os_str().as_encoded_bytes());
    // baml.toml is a compile input of the program key, so it must gate the
    // manifest too: a config-only change must not let plan_reuse splice
    // every source file against stale project configuration.
    hash_opt_str(&mut h, project_manifest_toml);
    CacheKey(h.finalize().into())
}

/// Content-addressed key for one serialized [`CompilationUnit`].
///
/// The payload hash, rather than source inputs, is the identity: cross-file
/// layout state can change a unit's bytes without changing that file's source.
/// The manifest owns reuse policy and points to the exact value produced by the
/// last successful compile.
pub fn unit_key(payload: &[u8]) -> CacheKey {
    unit_key_from_digest(&Sha256::digest(payload).into())
}

/// [`unit_key`] from an already-computed payload digest (the same digest
/// entry validation produces), skipping the payload hash.
fn unit_key_from_digest(digest: &[u8; 32]) -> CacheKey {
    let mut h = keyed_hasher(b"unit");
    h.update(digest);
    CacheKey(h.finalize().into())
}

/// Key for the precompiled stdlib `Program` slice.
///
/// Depends only on the compiler build, opt level, and cache format — not on
/// any project. One entry per compiler build serves every project on the
/// machine (the Go model: the stdlib is compiled once per toolchain, ever).
pub fn stdlib_key(compiler_fingerprint: &[u8; 32], opt_level: u8) -> CacheKey {
    fingerprint_opt_key(b"stdlib-slice", compiler_fingerprint, opt_level)
}

/// Key for the cached stdlib **typed interface** blob ("export data").
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
    fingerprint_opt_key(b"stdlib-interface", compiler_fingerprint, opt_level)
}

/// Key for the cached stdlib **builtin diagnostics** blob — the diagnostics that
/// `check_file` produces for the builtin (stdlib) files.
///
/// Like [`stdlib_interface_key`] and [`stdlib_key`], it depends only on the
/// compiler build + opt level: the builtin diagnostic set is a build constant
/// (no user file contributes to a stdlib package, the same soundness basis as
/// the typed interface), so one entry per compiler build serves every project
/// on the machine.
/// For a valid stdlib the set is empty, but the blob caches whatever the honest
/// check produces. The `b"stdlib-diagnostics"` domain separator keeps it a
/// distinct entry from the bytecode slice ([`stdlib_key`]) and the typed
/// interface ([`stdlib_interface_key`]), which share the same `(fingerprint,
/// opt_level)` inputs. `opt_level` is included for parity even though the
/// builtin diagnostic set is opt-invariant.
///
/// The stored payload's typed form lives in the CLI (`borsh(Option<Vec<
/// CachedDiagnostic>>)`, the same opaque machinery as the per-file diagnostics
/// blob), so `bex_cache` does not depend on its shape; a change to that wire
/// format is covered by [`FORMAT_VERSION`], which is folded into this key and
/// the entry header.
pub fn stdlib_diagnostics_key(compiler_fingerprint: &[u8; 32], opt_level: u8) -> CacheKey {
    fingerprint_opt_key(b"stdlib-diagnostics", compiler_fingerprint, opt_level)
}

/// Key for the cached `baml test --list` **discovery output** — the flattened,
/// unfiltered test list (legacy function-attached tests + fully-expanded testset
/// leaf names) that a `--list` invocation renders.
///
/// Derived deterministically from the Program blob's own key: the discovery
/// output is a pure function of that Program (same sources + compiler build ⇒
/// same tests), so it shares the Program's invalidation for free (Go model:
/// different inputs → different program key → different discovery key → miss).
/// The `b"test-discovery"` domain separator keeps it a distinct entry from the
/// Program blob stored under that same program key, mirroring how the other
/// derived entries ([`unit_key`], [`stdlib_interface_key`]) separate themselves
/// from blobs that share their key inputs. A warm `--list` serves the rendered
/// list from this entry and skips engine boot + in-VM discovery entirely.
///
/// The stored payload's typed form lives in the CLI (`borsh`-encoded opaque
/// bytes, like the per-file diagnostics blob), so `bex_cache` does not depend on
/// its shape; a change to that wire format is covered by [`FORMAT_VERSION`],
/// which is folded into this key and the entry header.
pub fn test_discovery_key(program_key: &[u8; 32]) -> CacheKey {
    let mut h = keyed_hasher(b"test-discovery");
    h.update(program_key);
    CacheKey(h.finalize().into())
}

/// Identity of the running compiler build: SHA-256 of the current executable's
/// bytes, mixed with the stamped product version.
///
/// Hashing the binary itself (rather than trusting a version string) is what
/// makes dev builds safe: two `canary` checkouts both claim "0.13.0" but emit
/// incompatible bytecode. The hash is memoized on disk under `cache_dir`,
/// keyed by the exe's `(len, mtime, ctime)` — the `ctime` (inode-change time)
/// component is what defeats mtime-preserving restores (`cp -p`, tar, CI cache
/// restore) that would otherwise replay a stale hash for a different build.
/// Platforms without that identity hash the executable on every process start;
/// a stale compiler fingerprint is a correctness failure, while one extra file
/// read is only startup cost.
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

    #[cfg(unix)]
    let memo = {
        use std::os::unix::fs::MetadataExt as _;

        let meta = fs::metadata(&exe)?;
        let len = meta.len();
        let mtime_ns = meta
            .modified()?
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let ctime = format!("{}.{}", meta.ctime(), meta.ctime_nsec());
        let mut path_hasher = Sha256::new();
        path_hasher.update(exe.as_os_str().as_encoded_bytes());
        let memo_name: [u8; 32] = path_hasher.finalize().into();
        let path = cache_dir.join("fingerprints").join(hex(&memo_name[..8]));

        if let Ok(contents) = fs::read_to_string(&path) {
            let mut parts = contents.split_whitespace();
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
        (path, len, mtime_ns, ctime)
    };

    #[cfg(not(unix))]
    let _ = cache_dir;

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

    // Best-effort Unix memo write; failure just means re-hashing next run.
    #[cfg(unix)]
    if fs::create_dir_all(memo.0.parent().expect("memo path has parent")).is_ok() {
        let _ = write_atomic(
            &memo.0,
            format!("{} {} {} {}\n", memo.1, memo.2, memo.3, hex(&digest)).as_bytes(),
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
        freshen_entry(&path);
        Some(program)
    }

    /// Like [`Self::load`], but returns the raw borsh payload without
    /// deserializing. For byte-level verification against a fresh compile,
    /// and for non-Program entries (the project manifest).
    pub fn load_raw(&self, key: &CacheKey) -> Option<Vec<u8>> {
        let path = self.entry_path(key);
        let data = fs::read(&path).ok()?;
        let payload = check_entry(&data, key)?;
        freshen_entry(&path);
        Some(payload.to_vec())
    }

    /// Local/remote read-through for a raw content-addressed entry, also
    /// returning the payload's integrity digest (already computed by entry
    /// validation) so content-key checks derived from that digest need not
    /// re-hash the payload.
    fn load_raw_shared_with_digest(&self, key: &CacheKey) -> Option<(Vec<u8>, [u8; 32])> {
        let path = self.entry_path(key);
        if let Ok(data) = fs::read(&path)
            && let Some((payload, digest)) = check_entry_with_digest(&data, key)
        {
            freshen_entry(&path);
            return Some((payload.to_vec(), digest));
        }
        let remote = self.remote.as_ref()?;
        let entry = remote.get(key)?;
        let (payload, digest) = check_entry_with_digest(&entry, key)?;
        let payload = payload.to_vec();
        self.persist_from_remote(key, &entry);
        Some((payload, digest))
    }

    /// Load one content-addressed unit through the local/remote read-through
    /// path. Both the entry framing and the unit key's payload hash must agree;
    /// any mismatch is a cache miss.
    pub fn load_unit_shared(&self, key: &CacheKey) -> Option<CompilationUnit> {
        // `check_entry` already verified the header's key echo and the payload
        // integrity digest, and `unit_key` is derived from that same digest —
        // so the content-key check here reuses the digest instead of hashing
        // the (potentially large) payload a second time.
        let (payload, digest) = self.load_raw_shared_with_digest(key)?;
        if unit_key_from_digest(&digest) != *key {
            let _ = fs::remove_file(self.entry_path(key));
            return None;
        }
        match borsh::from_slice(&payload) {
            Ok(unit) => Some(unit),
            Err(_) => {
                let _ = fs::remove_file(self.entry_path(key));
                None
            }
        }
    }

    /// Store one immutable unit locally and publish it remotely, but only when
    /// a valid local entry for the same content key is not already present.
    /// Returns the key and whether this call wrote a new local entry.
    pub fn store_unit_shared(&self, unit: &CompilationUnit) -> io::Result<(CacheKey, bool)> {
        let payload = borsh::to_vec(unit)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let key = unit_key(&payload);
        // load_raw already returns only a checksum-valid entry whose key echo
        // matches, so byte equality with the new payload is a sufficient
        // "already stored" signal — no need to hash the stored bytes again.
        if self.load_raw(&key).is_some_and(|stored| stored == payload) {
            return Ok((key, false));
        }
        self.store_raw_shared(&key, &payload)?;
        Ok((key, true))
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
    check_entry_with_digest(data, key).map(|(payload, _)| payload)
}

/// [`check_entry`] that also hands back the payload digest it computed, so
/// callers whose content keys derive from that digest (see [`unit_key`]) can
/// avoid a second full pass over the payload.
fn check_entry_with_digest<'a>(data: &'a [u8], key: &CacheKey) -> Option<(&'a [u8], [u8; 32])> {
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
    Some((payload, digest))
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

fn freshen_entry(path: &Path) {
    if let Ok(meta) = fs::metadata(path)
        && let Ok(modified) = meta.modified()
        && modified
            .elapsed()
            .map(|elapsed| elapsed.as_secs() > 3600)
            .unwrap_or(true)
    {
        let _ = filetime_touch(path);
    }
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

/// SHA-256 of a file's full content. The CLI's manifest writer (`content_hash`
/// in each [`ManifestFile`]) and the LSP's whole-project-clean seed gate both
/// hash through here, so a file is compared against the manifest like-for-like
/// — one definition the two sides cannot drift apart on.
pub fn content_hash(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

/// Whether environment variable `name` is set to exactly `"1"` — the shared
/// spelling of every `BAML_*` cache opt-out flag.
pub fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| value == "1")
}

/// `path` relative to `root` as a display string, falling back to the full path
/// when it is not under `root`. Manifests and cache keys store this
/// project-root-relative form to stay location-independent; the CLI writer and
/// the LSP reader both derive it here so their rel-path keys agree.
pub fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_inputs<'a>(files: &'a [(String, &'a str)]) -> KeyInputs<'a> {
        KeyInputs {
            compiler_fingerprint: [7u8; 32],
            opt_level: 2,
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

        // A well-formed entry from another cache format is also a miss. This
        // exercises the cache header contract directly instead of relying on a
        // particular Borsh struct-layout failure.
        cache.store(&key, &program).expect("re-store");
        let mut bytes = std::fs::read(&path).expect("read entry");
        bytes[4..8].copy_from_slice(&(FORMAT_VERSION - 1).to_le_bytes());
        std::fs::write(&path, &bytes).expect("rewrite version");
        assert!(cache.load(&key).is_none(), "stale format rejected");

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
    fn unit_entry_round_trip_and_dedup() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = BytecodeCache::open(dir.path().to_path_buf());
        let unit = CompilationUnit {
            source_file: "nested/a.baml".to_string(),
            callable_throws_fragment: vec![1, 2, 3, 4],
            ..CompilationUnit::default()
        };

        let (key, wrote) = cache.store_unit_shared(&unit).expect("store unit");
        assert!(wrote, "first store writes the CAS entry");
        let serialized = borsh::to_vec(&unit).expect("serialize unit");
        assert_eq!(key, unit_key(&serialized));

        let path = cache.entry_path(&key);
        let old = SystemTime::now() - std::time::Duration::from_hours(2);
        fs::File::options()
            .append(true)
            .open(&path)
            .expect("open unit")
            .set_modified(old)
            .expect("age unit");
        let loaded = cache.load_unit_shared(&key).expect("load unit");
        assert_eq!(loaded.source_file, unit.source_file);
        assert_eq!(
            loaded.callable_throws_fragment,
            unit.callable_throws_fragment
        );
        assert!(
            fs::metadata(&path)
                .expect("unit metadata")
                .modified()
                .expect("unit mtime")
                > old,
            "loading a live unit freshens it for trim"
        );

        let (same_key, wrote) = cache.store_unit_shared(&unit).expect("store again");
        assert_eq!(same_key, key);
        assert!(!wrote, "identical unit is not rewritten");
    }

    #[test]
    fn stale_format_manifest_entry_is_a_miss() {
        #[derive(borsh::BorshSerialize)]
        struct LegacyManifestFile {
            rel_path: String,
            content_hash: [u8; 32],
            signature_hash: [u8; 32],
            layout_hash: [u8; 32],
            defined_names: Vec<String>,
            referenced_names: Vec<String>,
            sig_referenced_names: Vec<String>,
            throw_facts: Vec<baml_type::throw_facts::FunctionThrowFacts>,
            diagnostics: Vec<u8>,
        }
        #[derive(borsh::BorshSerialize)]
        struct LegacyManifest {
            program_key: [u8; 32],
            files: Vec<LegacyManifestFile>,
        }

        let dir = tempfile::tempdir().expect("tempdir");
        let cache = BytecodeCache::open(dir.path().to_path_buf());
        let key = manifest_key(&[7u8; 32], 2, Path::new("/project"), None);
        let payload = borsh::to_vec(&LegacyManifest {
            program_key: [8u8; 32],
            files: vec![LegacyManifestFile {
                rel_path: "a.baml".to_string(),
                content_hash: [0u8; 32],
                signature_hash: [0u8; 32],
                layout_hash: [0u8; 32],
                defined_names: Vec::new(),
                referenced_names: Vec::new(),
                sig_referenced_names: Vec::new(),
                throw_facts: Vec::new(),
                diagnostics: Vec::new(),
            }],
        })
        .expect("serialize legacy manifest");
        // Write a well-formed current-version entry, then rewind only its version
        // field so the legacy payload sits under a header the reader must reject.
        cache
            .store_raw(&key, &payload)
            .expect("store legacy payload");
        let path = cache.entry_path(&key);
        let mut bytes = fs::read(&path).expect("read entry");
        bytes[4..8].copy_from_slice(&(FORMAT_VERSION - 1).to_le_bytes());
        fs::write(&path, bytes).expect("rewrite version");

        assert!(
            cache.load_raw(&key).is_none(),
            "legacy manifest entry must degrade to a cache miss"
        );
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
    fn stdlib_diagnostics_key_is_distinct_and_input_sensitive() {
        let fp = [3u8; 32];
        let base = stdlib_diagnostics_key(&fp, 2);

        // Never collides with its two siblings under the same inputs (distinct
        // domain separators over the same `(fingerprint, opt_level)`).
        assert_ne!(
            base,
            stdlib_key(&fp, 2),
            "diagnostics key must not collide with the bytecode-slice key"
        );
        assert_ne!(
            base,
            stdlib_interface_key(&fp, 2),
            "diagnostics key must not collide with the typed-interface key"
        );
        // Sensitive to both keying inputs.
        assert_ne!(base, stdlib_diagnostics_key(&[4u8; 32], 2), "fingerprint");
        assert_ne!(base, stdlib_diagnostics_key(&fp, 0), "opt level");
    }

    #[test]
    fn test_discovery_key_is_distinct_and_derived_from_program_key() {
        let program_key = [5u8; 32];
        let base = test_discovery_key(&program_key);

        // Never collides with the Program blob or a sibling content-addressed
        // entry keyed off the same bytes (distinct domain separators).
        assert_ne!(
            base.0, program_key,
            "discovery key must not equal the raw program key"
        );
        assert_ne!(
            base,
            unit_key(&program_key),
            "discovery key must not collide with a unit key over the same bytes"
        );
        // A different Program yields a different discovery entry.
        assert_ne!(base, test_discovery_key(&[6u8; 32]), "program key");
    }

    #[test]
    fn test_discovery_entry_roundtrips_opaque_bytes() {
        // The discovery blob is stored via the payload-agnostic `store_raw` /
        // `load_raw` seam, so it inherits the header + checksum integrity that
        // `store_load_roundtrip_and_rejections` already pins for every entry.
        // This pins only what is specific to the discovery kind: CLI-owned
        // opaque bytes survive the raw round-trip byte-for-byte.
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = BytecodeCache::open(dir.path().to_path_buf());
        let key = test_discovery_key(&[1u8; 32]);

        assert!(cache.load_raw(&key).is_none(), "miss on empty cache");
        let payload = b"opaque-discovery-bytes".to_vec();
        cache.store_raw(&key, &payload).expect("store");
        assert_eq!(cache.load_raw(&key).as_deref(), Some(payload.as_slice()));
    }

    #[test]
    fn trim_removes_only_old_entries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = BytecodeCache::open(dir.path().to_path_buf());
        let files = vec![("a.baml".to_string(), "fn x {}")];
        let key = compute_key(&dummy_inputs(&files));
        cache.store(&key, &Program::default()).expect("store");

        cache
            .trim(std::time::Duration::from_hours(1))
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
/// bearer token). Only immutable entries — Program blobs, stdlib slices, and
/// content-addressed compilation units — are shared; the per-project manifest
/// is a mutable local-latest pointer and deliberately stays local.
///
/// The configured origin is a trust boundary because it supplies executable VM
/// programs. Non-loopback remotes therefore require HTTPS. Failures are bounded
/// by a short timeout and degrade to local-only behavior. Downloaded entries
/// still pass the local header/checksum validation and are persisted locally so
/// the remote is hit at most once per entry.
pub struct RemoteCache {
    base_url: String,
    token: Option<String>,
    client: reqwest::blocking::Client,
}

/// HTTPS authenticates a configured remote origin. Plain HTTP is accepted only
/// for an in-process or same-machine cache server.
fn remote_url_allowed(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        || (url.scheme() == "http"
            && matches!(
                url.host_str(),
                Some("localhost" | "127.0.0.1" | "::1" | "[::1]")
            ))
}

impl RemoteCache {
    /// Build from `BAML_CACHE_REMOTE` / `BAML_CACHE_REMOTE_TOKEN`; `None`
    /// when unset or the HTTP client cannot be constructed.
    pub fn from_env() -> Option<RemoteCache> {
        let base_url = std::env::var("BAML_CACHE_REMOTE").ok()?;
        if base_url.is_empty() {
            return None;
        }
        let parsed = reqwest::Url::parse(&base_url).ok()?;
        if !remote_url_allowed(&parsed) {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("warning: BAML_CACHE_REMOTE ignored — use https or a loopback http URL");
            }
            return None;
        }
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(std::time::Duration::from_millis(500))
            .timeout(std::time::Duration::from_secs(2))
            // Do not forward executable cache requests or bearer credentials
            // across redirects to a different origin or weaker scheme.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .ok()?;
        let base_url = base_url.trim_end_matches('/').to_string();
        let token = std::env::var("BAML_CACHE_REMOTE_TOKEN").ok();
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

    fn put(&self, key: &CacheKey, entry: Vec<u8>) {
        let mut request = self
            .client
            .put(self.entry_url(key))
            .header("content-type", "application/octet-stream")
            .body(entry);
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

    /// Publish the local entry for `key` to the remote backend, if one is
    /// configured. Best-effort: sharing is an optimization, never a failure.
    fn publish_remote(&self, key: &CacheKey) {
        if let Some(remote) = &self.remote
            && let Ok(entry) = fs::read(self.entry_path(key))
        {
            remote.put(key, entry);
        }
    }

    /// Persist a validated remote `entry` into the local store so the remote is
    /// hit at most once per key. Best-effort.
    fn persist_from_remote(&self, key: &CacheKey, entry: &[u8]) {
        let path = self.entry_path(key);
        if let Some(parent) = path.parent()
            && fs::create_dir_all(parent).is_ok()
        {
            let _ = write_atomic(&path, entry);
        }
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
        // The configured HTTPS origin (or local loopback server) is trusted to
        // supply executable VM programs. The entry framing additionally rejects
        // corruption, truncation, version skew, and responses for another key.
        let payload = check_entry(&entry, key)?;
        let program = borsh::from_slice::<Program>(payload).ok()?;
        self.persist_from_remote(key, &entry);
        Some(program)
    }

    /// Like [`Self::store`], but also publishes the entry to the remote
    /// backend (best-effort) so other machines can hit it.
    pub fn store_shared(&self, key: &CacheKey, program: &Program) -> io::Result<()> {
        self.store(key, program)?;
        self.publish_remote(key);
        Ok(())
    }

    /// Raw-payload counterpart to [`Self::store_shared`] for immutable
    /// content-addressed entries.
    pub fn store_raw_shared(&self, key: &CacheKey, payload: &[u8]) -> io::Result<()> {
        self.store_raw(key, payload)?;
        self.publish_remote(key);
        Ok(())
    }
}

#[cfg(test)]
mod remote_tests {
    use std::{
        collections::HashMap,
        io::{BufRead, BufReader, Read as _, Write as _},
        net::TcpListener,
        sync::{Arc, Mutex},
    };

    use super::*;

    /// Shared body store backing the in-process HTTP server, keyed by request
    /// path.
    type SharedStore = Arc<Mutex<HashMap<String, Vec<u8>>>>;

    /// Minimal in-process HTTP store: GET serves stored bodies, PUT stores
    /// them. Std-only so the test needs no async runtime.
    fn spawn_http_store() -> (String, SharedStore) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let store: SharedStore = Arc::default();
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
                            server_store.lock().expect("lock").insert(path, body);
                        }
                        let _ = stream.write_all(
                            b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                        );
                    }
                    "GET" => match server_store.lock().expect("lock").get(&path) {
                        Some(body) => {
                            let _ = stream.write_all(
                                format!(
                                    "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                                    body.len()
                                )
                                .as_bytes(),
                            );
                            let _ = stream.write_all(body);
                        }
                        None => {
                            let _ = stream
                                .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
                        }
                    },
                    _ => {
                        let _ = stream.write_all(
                            b"HTTP/1.1 405 Method Not Allowed\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                        );
                    }
                }
            }
        });
        (format!("http://{addr}"), store)
    }

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
            manifest: None,
            files: &files,
        });

        // Machine A: store publishes to the remote.
        let dir_a = tempfile::tempdir().expect("tempdir");
        let mut cache_a = BytecodeCache::open(dir_a.path().to_path_buf());
        cache_a.remote = Some(remote_for(&url));
        cache_a.store_shared(&key, &program).expect("store");
        let unit = CompilationUnit {
            source_file: "a.baml".to_string(),
            callable_throws_fragment: vec![4, 3, 2, 1],
            ..CompilationUnit::default()
        };
        let (unit_key, wrote) = cache_a.store_unit_shared(&unit).expect("store unit");
        assert!(wrote);
        assert_eq!(
            store.lock().expect("lock").len(),
            2,
            "program and unit entries published to remote"
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
        let loaded_unit = cache_b
            .load_unit_shared(&unit_key)
            .expect("unit served from remote");
        assert_eq!(loaded_unit.source_file, "a.baml");
        assert!(
            cache_b.load_raw(&unit_key).is_some(),
            "unit persisted locally after remote read-through"
        );

        // A corrupted remote entry is rejected by validation.
        let key2 = compute_key(&KeyInputs {
            compiler_fingerprint: [10u8; 32],
            opt_level: 2,
            manifest: None,
            files: &files,
        });
        store
            .lock()
            .expect("lock")
            .insert(format!("/{}", key2.hex()), b"garbage".to_vec());
        assert!(
            cache_b.load_shared(&key2).is_none(),
            "corrupt remote entry rejected"
        );
    }

    #[test]
    fn remote_requires_https_or_loopback() {
        let allowed = |url: &str| remote_url_allowed(&reqwest::Url::parse(url).expect("valid URL"));
        assert!(allowed("https://cache.example.com"));
        assert!(!allowed("http://cache.example.com"));
        assert!(allowed("http://localhost:8080"));
        assert!(allowed("http://127.0.0.1:8080"));
        assert!(allowed("http://[::1]:8080"));
        assert!(!allowed("http://localhost.evil.com"));
        assert!(!allowed("http://127.0.0.1@evil.com"));
    }
}
