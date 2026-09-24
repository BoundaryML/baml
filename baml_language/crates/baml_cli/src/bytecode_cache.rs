//! CLI wiring for the content-addressed bytecode cache (`bex_cache`).
//!
//! Three tiers, each keyed by every input its value is a function of (the Go
//! model: no invalidation logic, only keys):
//!
//! - The whole-program `Program` blob, keyed by every compile input
//!   ([`compute_key`]) — the warm hit for `run`/`test`/`pack`, which skips
//!   the database entirely.
//! - One output per PACKAGE ([`EmittedPackage`]: unit, record, tail), keyed
//!   by the package's own sources and its dependencies' interface digests
//!   ([`package_key`]). A compile emits only the packages the cache does not
//!   hold and links the rest: the stdlib's packages are served on every
//!   compile after the first on a toolchain, whatever project is being built,
//!   and the user package whenever its sources are unchanged.
//! - The check layer's per-file rows (diagnostics blobs, throw facts,
//!   `callable_throws` fragments) in the project manifest, served only when
//!   the project is byte-identical to the compile that wrote them — a file's
//!   diagnostics depend on every declaration it resolves, not on its own
//!   bytes alone, so nothing short of an identical program proves a row
//!   current.
//!
//! Knobs:
//! - `BAML_NO_BYTECODE_CACHE=1` — disable lookups and writes entirely.
//! - `BAML_CACHE_DIR=<path>` — cache location override (default:
//!   `<project>/.baml/cache`). Content addressing makes a shared directory
//!   safe across projects.
//! - `BAML_CACHE_VERIFY=1` — tripwire mode: never serve from the cache;
//!   compile, then hard-fail if the fresh bytecode differs from a cached
//!   entry under the same key (catches emit nondeterminism and missing
//!   cache-key inputs). Also runs the stdlib-interface and per-file
//!   diagnostics oracles. Too expensive to leave always-on.
//! - `BAML_CACHE_SAMPLED_VERIFY` — sampled field verification, the always-on
//!   complement to full verify (rustc's "1-in-N compiles verifies one
//!   artifact" hardening). On a warm compile that *serves* the check rows,
//!   ~1 run in 32 picks one served file and checks its served diagnostics
//!   blob and `callable_throws` fragment against an honest, un-seeded
//!   re-derivation — after the compile result is already produced, so the
//!   added latency is one file's honest work. A mismatch is a hard error
//!   (silent staleness must be LOUD). `=0` disables; `=1` forces every warm
//!   compile (tests); default is 1/32. See [`sampled_pick_from_key`] for the
//!   key-derived, RNG-free sampling decision.
//! - `BAML_NO_DIAGNOSTICS_CACHE=1` — check every file instead of serving the
//!   check rows, to isolate that feature's win. Also forces the builtin
//!   (stdlib) files to be checked honestly instead of served from the
//!   per-toolchain stdlib-diagnostics blob — one knob governs all
//!   diagnostics serving.
//! - `BAML_NO_CALLABLE_THROWS_CACHE=1` — empty the per-function `callable_throws`
//!   seed so every function infers its throws honestly (diagnostics serving
//!   unaffected), to isolate the fragment seed's win.
//! - `BAML_NO_DISCOVERY_CACHE=1` — never serve `baml test --list` from the
//!   cached discovery output (the flattened test list), forcing the honest
//!   engine-boot + in-VM discovery every time, to A/B the discovery cache's win.
//!   The rest of the cache is unaffected.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use baml_db::{
    CompileProgramError, PackageCache, ProjectDatabase, SourceFile, SourceRoot,
    baml_compiler2_emit::OptLevel, baml_compiler2_hir::package::edge_table,
    baml_compiler2_hir_ty::package_interface::interface_digest,
};
use baml_linker_types::EmittedPackage;
use bex_cache::{
    BytecodeCache, CacheKey, KeyInputs, ManifestFile, PackageKeyInputs, ProjectManifest,
    compiler_fingerprint, compute_key, env_flag, manifest_key, package_key, rel_path,
    stdlib_diagnostics_key, stdlib_interface_key, test_discovery_key,
};
use bex_vm_types::Program;

use crate::{project_load::ResolvedProject, project_session::ProjectSession};

/// The optimization level every CLI compile uses (the emit default).
const CLI_OPT_LEVEL: OptLevel = OptLevel::Two;

/// An opened cache plus the keys for one resolved project + compile config.
pub(crate) struct CacheContext {
    cache: BytecodeCache,
    /// Whole-project Program, keyed by sources + options + compiler build.
    key: CacheKey,
    /// Cached stdlib typed-interface blob (B-694), keyed by compiler build +
    /// opt level only — the stdlib is a build constant.
    stdlib_interface_key: CacheKey,
    /// Cached stdlib **builtin diagnostics** blob, keyed by compiler build + opt
    /// level only — the builtin diagnostic set is a build constant (empty for a
    /// valid stdlib). Served on the warm path so builtins drop out of the
    /// per-file diagnostics check.
    stdlib_diagnostics_key: CacheKey,
    /// Latest-compile manifest, fixed per (project root, options, build).
    manifest_key: CacheKey,
    /// Cached `baml test --list` discovery output (flattened, unfiltered test
    /// list), derived from `key` — a warm `--list` serves it and skips engine
    /// boot + in-VM discovery entirely.
    test_discovery_key: CacheKey,
    /// Identity of the compiler build: one half of every package key.
    fingerprint: [u8; 32],
    /// Interface digests of the packages this context's package keys depend
    /// on, memoized per context — a digest serializes the whole interface.
    digests: Mutex<HashMap<SourceRoot, [u8; 32]>>,
    /// How many packages this context served from the cache, and how many it
    /// saw emitted fresh — the warm-path evidence the debug channel prints
    /// and the tests assert.
    packages_served: AtomicUsize,
    packages_emitted: AtomicUsize,
}

impl CacheContext {
    /// `None` when caching is disabled via `BAML_NO_BYTECODE_CACHE=1`.
    pub(crate) fn open(resolved: &ResolvedProject) -> Option<Self> {
        if env_flag("BAML_NO_BYTECODE_CACHE") {
            return None;
        }
        let dir = std::env::var_os("BAML_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| resolved.root.join(".baml").join("cache"));
        let fingerprint = compiler_fingerprint(&dir);

        // Root-relative paths keep the key location-independent. Discovery
        // order is sorted by full path; stripping the shared root prefix
        // preserves that order.
        let files: Vec<(String, &str)> = resolved
            .files
            .iter()
            .map(|(path, content)| {
                let rel = path.strip_prefix(&resolved.root).unwrap_or(path);
                (rel.to_string_lossy().into_owned(), content.as_str())
            })
            .collect();

        let key = compute_key(&KeyInputs {
            compiler_fingerprint: fingerprint,
            opt_level: CLI_OPT_LEVEL as u8,
            manifest: resolved.manifest.as_deref(),
            files: &files,
        });

        Some(CacheContext {
            cache: BytecodeCache::open(dir).with_remote_from_env(),
            key,
            stdlib_interface_key: stdlib_interface_key(&fingerprint, CLI_OPT_LEVEL as u8),
            stdlib_diagnostics_key: stdlib_diagnostics_key(&fingerprint, CLI_OPT_LEVEL as u8),
            manifest_key: manifest_key(
                &fingerprint,
                CLI_OPT_LEVEL as u8,
                &resolved.root,
                resolved.manifest.as_deref(),
            ),
            test_discovery_key: test_discovery_key(key.as_bytes()),
            fingerprint,
            digests: Mutex::new(HashMap::new()),
            packages_served: AtomicUsize::new(0),
            packages_emitted: AtomicUsize::new(0),
        })
    }

    /// Tripwire mode: force a real compile even on a hit, then byte-compare.
    pub(crate) fn verify_enabled() -> bool {
        env_flag("BAML_CACHE_VERIFY")
    }

    /// Load and borsh-decode a raw cache entry under `key`. `None` on a miss or
    /// a decode failure — both fall through to honest recomputation, the decode
    /// failure logged (labelled `what`) under `BAML_CACHE_DEBUG`. Callers own any
    /// disable-knob gating before this.
    fn load_decoded<T: borsh::BorshDeserialize>(&self, key: &CacheKey, what: &str) -> Option<T> {
        let bytes = self.cache.load_raw(key)?;
        match borsh::from_slice::<T>(&bytes) {
            Ok(value) => Some(value),
            Err(e) => {
                cache_debug(format_args!("{what} undecodable: {e}"));
                None
            }
        }
    }

    /// Serialize `value` and store it under `key`, best-effort: a serialize or
    /// store failure is logged (labelled `what`) under `BAML_CACHE_DEBUG` and
    /// otherwise ignored — the entry is simply re-derived next run.
    fn store_encoded<T: borsh::BorshSerialize>(&self, key: &CacheKey, value: &T, what: &str) {
        match borsh::to_vec(value) {
            Ok(payload) => {
                if let Err(e) = self.cache.store_raw(key, &payload) {
                    cache_debug(format_args!("{what} store failed: {e}"));
                }
            }
            Err(e) => cache_debug(format_args!("{what} serialize failed: {e}")),
        }
    }

    pub(crate) fn load(&self) -> Option<Program> {
        self.cache.load_shared(&self.key)
    }

    /// The `BAML_CACHE_VERIFY` tripwire: byte-compare a fresh compile against
    /// any existing entry under the same key. A mismatch is a hard error —
    /// it means emit is nondeterministic or a compile input is missing from
    /// the cache key.
    pub(crate) fn verify_against(&self, program: &Program) -> anyhow::Result<()> {
        if !Self::verify_enabled() {
            return Ok(());
        }
        if let Some(cached) = self.cache.load_raw(&self.key) {
            let fresh = borsh::to_vec(program)?;
            if fresh != cached {
                anyhow::bail!(
                    "BAML_CACHE_VERIFY: cached bytecode for key {} differs from a fresh \
                     compile ({} vs {} bytes). This means emit is nondeterministic or a \
                     compile input is missing from the cache key — please report this.",
                    self.key.hex(),
                    cached.len(),
                    fresh.len(),
                );
            }
        }
        Ok(())
    }

    /// Write-through after a successful compile. Best-effort: a cache write
    /// problem must never fail the run.
    pub(crate) fn store(&self, program: &Program) -> std::io::Result<()> {
        self.cache.store_shared(&self.key, program)?;
        self.cache.maybe_trim();
        Ok(())
    }

    /// Isolation toggle for measuring the stdlib-interface cache's win:
    /// `BAML_NO_STDLIB_INTERFACE_CACHE=1` disables *only* the interface seed
    /// (leaving the package and check-row tiers intact) so a with/without
    /// timing comparison isolates B-694. `BAML_NO_BYTECODE_CACHE` already
    /// disables the whole cache, so this is the finer-grained knob.
    fn stdlib_interface_cache_disabled() -> bool {
        env_flag("BAML_NO_STDLIB_INTERFACE_CACHE")
    }

    /// Isolation toggle for measuring the check-row cache's win:
    /// `BAML_NO_DIAGNOSTICS_CACHE=1` drops the served rows so every file is
    /// re-checked. `BAML_NO_BYTECODE_CACHE` already disables the whole cache,
    /// so this is the finer-grained knob.
    fn diagnostics_cache_disabled() -> bool {
        env_flag("BAML_NO_DIAGNOSTICS_CACHE")
    }

    /// Isolation toggle for measuring the `callable_throws` seed's win:
    /// `BAML_NO_CALLABLE_THROWS_CACHE=1` empties the seed so every function
    /// infers its throws honestly, leaving diagnostics serving intact.
    /// `BAML_NO_BYTECODE_CACHE` already disables the whole cache, so this is
    /// the finer-grained knob.
    fn callable_throws_cache_disabled() -> bool {
        env_flag("BAML_NO_CALLABLE_THROWS_CACHE")
    }

    /// Load the cached stdlib typed-interface blob (B-694), if present:
    /// `load_raw` + borsh-decode into `package-name -> borsh(PackageInterface)`.
    /// `None` on a miss, a decode failure, or when the interface cache is
    /// disabled — every case falls through to honest stdlib derivation.
    pub(crate) fn load_stdlib_interface(&self) -> Option<BTreeMap<String, Vec<u8>>> {
        if Self::stdlib_interface_cache_disabled() {
            return None;
        }
        self.load_decoded(&self.stdlib_interface_key, "stdlib interface")
    }

    /// Extract and store the stdlib typed-interface blob after a successful
    /// (interface cache-miss) compile. Best-effort, like every cache write; a
    /// failed write just means re-deriving next run. Skipped when the interface
    /// cache is disabled.
    pub(crate) fn store_stdlib_interface(&self, db: &ProjectDatabase) {
        if Self::stdlib_interface_cache_disabled() {
            return;
        }
        let blob = baml_db::stdlib_prefix::stdlib_interfaces(db);
        self.store_encoded(&self.stdlib_interface_key, &blob, "stdlib interface");
    }

    /// Localized B-694 verify oracle (analog of `gocacheverify`): under
    /// `BAML_CACHE_VERIFY` the stdlib seed is *not* applied, so `db` derives every
    /// stdlib interface honestly; this compares that derivation byte-for-byte
    /// against any cached blob. A mismatch means the cached "export data" is a
    /// stale substitute that would change typecheck results — a hard error, and a
    /// tighter signal than the whole-`Program` byte-compare (it names the drifted
    /// package).
    pub(crate) fn verify_stdlib_interface(&self, db: &ProjectDatabase) -> anyhow::Result<()> {
        if !Self::verify_enabled() {
            return Ok(());
        }
        let Some(cached) = self.load_raw_stdlib_interface_for_verify() else {
            return Ok(());
        };
        let derived = baml_db::stdlib_prefix::stdlib_interfaces(db);
        for (name, derived_bytes) in &derived {
            if let Some(cached_bytes) = cached.get(name) {
                if cached_bytes != derived_bytes {
                    anyhow::bail!(
                        "BAML_CACHE_VERIFY: cached stdlib interface for package `{name}` differs \
                         from a fresh derivation ({} vs {} bytes). The cached typed interface is a \
                         stale or incomplete substitute — please report this.",
                        cached_bytes.len(),
                        derived_bytes.len(),
                    );
                }
            }
        }
        Ok(())
    }

    /// Read the cached interface blob for the verify oracle, bypassing the
    /// `BAML_NO_STDLIB_INTERFACE_CACHE` disable (verify must still compare
    /// against whatever is on disk).
    fn load_raw_stdlib_interface_for_verify(&self) -> Option<BTreeMap<String, Vec<u8>>> {
        self.load_decoded(&self.stdlib_interface_key, "stdlib interface")
    }

    /// Load the cached stdlib **builtin diagnostics** blob, if present: the
    /// opaque payload that `collect_diagnostics_incremental` rehydrates onto
    /// current-process builtin `FileId`s. `None` on a miss or when the
    /// diagnostics cache is disabled (`BAML_NO_DIAGNOSTICS_CACHE=1`) — both fall
    /// through to the honest builtin check, the one knob that governs all
    /// diagnostics serving.
    pub(crate) fn load_stdlib_diagnostics(&self) -> Option<Vec<u8>> {
        if Self::diagnostics_cache_disabled() {
            return None;
        }
        self.cache.load_raw(&self.stdlib_diagnostics_key)
    }

    /// Materialize the stdlib builtin-diagnostics blob on a miss (write-through
    /// after a passing compile), mirroring [`Self::store_stdlib_interface`]. The
    /// builtin set is a per-toolchain build constant, so this self-gates on blob
    /// presence: it writes only when no entry exists yet (a genuine miss) and
    /// never rewrites a served blob. Deriving the blob re-checks the builtins,
    /// but on the just-compiled database every builtin scope is Salsa-memoized,
    /// so no fresh inference is pulled. Best-effort; skipped when the diagnostics
    /// cache is disabled.
    pub(crate) fn store_stdlib_diagnostics(&self, db: &ProjectDatabase) {
        if Self::diagnostics_cache_disabled() {
            return;
        }
        if self.cache.load_raw(&self.stdlib_diagnostics_key).is_some() {
            return;
        }
        let blob = crate::diagnostics_cache::serialize_builtin_diagnostics(db);
        if let Err(e) = self.cache.store_raw(&self.stdlib_diagnostics_key, &blob) {
            cache_debug(format_args!("stdlib diagnostics store failed: {e}"));
        }
    }

    /// Localized builtin-diagnostics verify oracle (analog of
    /// [`Self::verify_stdlib_interface`] / [`Self::verify_diagnostics`]): under
    /// `BAML_CACHE_VERIFY` the builtin serve is disabled, so
    /// `collect_diagnostics_incremental` checks the builtins honestly and this
    /// compares any cached blob against a fresh builtin check. A mismatch means
    /// the cached builtin diagnostics are a stale substitute that would change
    /// what a warm run reports — a hard error.
    pub(crate) fn verify_stdlib_diagnostics(&self, db: &ProjectDatabase) -> anyhow::Result<()> {
        if !Self::verify_enabled() {
            return Ok(());
        }
        // Read the on-disk blob directly, bypassing the disable knob (verify must
        // still compare against whatever is on disk).
        let Some(cached_blob) = self.cache.load_raw(&self.stdlib_diagnostics_key) else {
            return Ok(());
        };
        let honest = crate::diagnostics_cache::collect_builtin_diagnostics(db);
        Self::compare_stdlib_diagnostics(db, &cached_blob, &honest)
    }

    /// The env-independent core of [`Self::verify_stdlib_diagnostics`], so the
    /// oracle's discriminating power (pass on a faithful blob, bail on a stale
    /// one) is unit-testable without mutating the process environment. An
    /// undecodable blob is *not* a violation — the warm path degrades to the
    /// honest builtin check, never serving a partial set — so it passes.
    pub(crate) fn compare_stdlib_diagnostics(
        db: &ProjectDatabase,
        cached_blob: &[u8],
        honest: &[baml_db::baml_compiler_diagnostics::Diagnostic],
    ) -> anyhow::Result<()> {
        let Some(cached) = crate::diagnostics_cache::rehydrate_builtin_blob(db, cached_blob) else {
            return Ok(());
        };
        if !diagnostic_sets_equal(&cached, honest) {
            anyhow::bail!(
                "BAML_CACHE_VERIFY: cached stdlib (builtin) diagnostics differ from a fresh \
                 check ({} cached vs {} honest). The cached builtin-diagnostics blob is a stale \
                 substitute — please report this.",
                cached.len(),
                honest.len(),
            );
        }
        Ok(())
    }
}

/// The cached `baml test --list` **discovery output**: everything a `--list`
/// invocation renders, in the exact order it renders it, *unfiltered* so any
/// `-i`/`-x` selection is served from one entry (the filter is re-applied live
/// in Rust via `TestFilter`, which mirrors `testing.leaf_selected`).
///
/// This is a pure function of the compiled Program (same sources + compiler
/// build ⇒ same tests), so it is keyed by the Program's own cache key
/// ([`test_discovery_key`]). A warm hit renders directly from this and skips
/// engine boot, `$init`/`$init_test`, and in-VM testset expansion entirely.
///
/// R1 (design §5): a testset generator running IO/LLM could make discovery
/// depend on state outside the cache key. Posture 1 mitigations: the
/// `BAML_CACHE_VERIFY` tripwire ([`CacheContext::verify_test_discovery`]) and
/// the `BAML_NO_DISCOVERY_CACHE` opt-out. The datum is only ever written from a
/// discovery that completed without error.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct TestDiscovery {
    /// Fully-expanded testset leaf names (canonical `root...::...` ids), unfiltered, in
    /// `collect_leaf_names` order.
    pub(crate) testset_leaf_names: Vec<String>,
}

impl CacheContext {
    /// Isolation / A-B toggle for the `--list` discovery cache:
    /// `BAML_NO_DISCOVERY_CACHE=1` disables *only* serving/writing the flattened
    /// test list, so `--list` always boots the engine and discovers honestly,
    /// leaving the other tiers intact. `BAML_NO_BYTECODE_CACHE` already
    /// disables the whole cache, so this is the finer-grained knob.
    fn discovery_cache_disabled() -> bool {
        env_flag("BAML_NO_DISCOVERY_CACHE")
    }

    /// Load the cached `--list` discovery output, if present: `load_raw` + borsh
    /// decode. `None` on a miss, a decode failure, or when the discovery cache is
    /// disabled — every case falls through to honest engine-boot discovery.
    pub(crate) fn load_test_discovery(&self) -> Option<TestDiscovery> {
        if Self::discovery_cache_disabled() {
            return None;
        }
        self.load_decoded(&self.test_discovery_key, "test discovery")
    }

    /// Write-through the discovery output after a successful honest discovery.
    /// Best-effort, like every cache write; a failed write just means
    /// re-discovering next run. Skipped when the discovery cache is disabled.
    /// Callers must only pass a discovery that completed without error
    /// (never-save-on-error, design §6).
    pub(crate) fn store_test_discovery(&self, disco: &TestDiscovery) {
        if Self::discovery_cache_disabled() {
            return;
        }
        self.store_encoded(&self.test_discovery_key, disco, "test discovery");
    }

    /// The `BAML_CACHE_VERIFY` tripwire for `--list` discovery (design §6): under
    /// verify the discovery cache is *not* served (the caller ran the honest
    /// engine-boot discovery), so this compares that honest result byte-for-byte
    /// against any cached blob on disk. A mismatch means discovery is
    /// nondeterministic or reads uncached state (an impure testset generator) —
    /// a hard error, our `gocacheverify` for tests.
    pub(crate) fn verify_test_discovery(&self, honest: &TestDiscovery) -> anyhow::Result<()> {
        if !Self::verify_enabled() {
            return Ok(());
        }
        // Read the on-disk blob directly, bypassing the `BAML_NO_DISCOVERY_CACHE`
        // disable (verify must still compare against whatever is on disk).
        let Some(cached) =
            self.load_decoded::<TestDiscovery>(&self.test_discovery_key, "test discovery")
        else {
            return Ok(());
        };
        Self::compare_test_discovery(&cached, honest)
    }

    /// The env-independent core of [`Self::verify_test_discovery`], so the
    /// oracle's discriminating power (pass on a faithful cache, bail on a stale
    /// one) is unit-testable without mutating the process environment.
    pub(crate) fn compare_test_discovery(
        cached: &TestDiscovery,
        honest: &TestDiscovery,
    ) -> anyhow::Result<()> {
        if cached.testset_leaf_names != honest.testset_leaf_names {
            anyhow::bail!(
                "BAML_CACHE_VERIFY: cached `test --list` testset discovery differs from a fresh \
                 discovery ({} vs {} leaf tests). A testset generator is nondeterministic or reads \
                 uncached state (IO/env/LLM) — please report this.",
                cached.testset_leaf_names.len(),
                honest.testset_leaf_names.len(),
            );
        }
        Ok(())
    }
}

// ── The per-package tier ─────────────────────────────────────────────────────

impl CacheContext {
    /// The key of `root`'s output at `opt`: its own sources by package-relative
    /// path, and the interface digest of every package it reaches by the edge
    /// name it reaches it under (see [`package_key`]).
    pub(crate) fn package_key(
        &self,
        db: &dyn baml_db::baml_compiler2_hir::Db,
        root: SourceRoot,
        opt: OptLevel,
    ) -> CacheKey {
        let root_path = root.path(db);
        let files: Vec<(String, &str)> = root
            .files(db)
            .iter()
            .map(|file| (rel_path(root_path, &file.path(db)), file.text(db).as_str()))
            .collect();
        let dependencies: Vec<(String, [u8; 32])> = edge_table(db, root)
            .into_iter()
            .map(|(edge, dependency)| (edge.to_string(), self.interface_digest(db, dependency)))
            .collect();
        package_key(&PackageKeyInputs {
            compiler_fingerprint: self.fingerprint,
            opt_level: opt as u8,
            files: &files,
            dependencies: &dependencies,
        })
    }

    fn interface_digest(
        &self,
        db: &dyn baml_db::baml_compiler2_hir::Db,
        root: SourceRoot,
    ) -> [u8; 32] {
        let mut digests = self
            .digests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *digests
            .entry(root)
            .or_insert_with(|| interface_digest(db, root))
    }

    /// How many packages this context served from the cache so far.
    pub(crate) fn packages_served(&self) -> usize {
        self.packages_served.load(Ordering::Relaxed)
    }

    /// How many packages this context saw emitted fresh so far.
    pub(crate) fn packages_emitted(&self) -> usize {
        self.packages_emitted.load(Ordering::Relaxed)
    }
}

/// A package's output is served by its key and offered back under it. Under
/// `BAML_CACHE_VERIFY` nothing is served — the tripwire exercises the full
/// compile path — but everything emitted is still stored.
impl PackageCache for CacheContext {
    fn load(
        &self,
        db: &dyn baml_db::baml_compiler2_hir::Db,
        root: SourceRoot,
        opt: OptLevel,
    ) -> Option<EmittedPackage> {
        if Self::verify_enabled() {
            return None;
        }
        let emitted = self
            .cache
            .load_package_shared(&self.package_key(db, root, opt))?;
        self.packages_served.fetch_add(1, Ordering::Relaxed);
        cache_debug(format_args!("package served: {}", root.path(db).display()));
        Some(emitted)
    }

    fn store(
        &self,
        db: &dyn baml_db::baml_compiler2_hir::Db,
        root: SourceRoot,
        opt: OptLevel,
        emitted: &EmittedPackage,
    ) {
        self.packages_emitted.fetch_add(1, Ordering::Relaxed);
        let key = self.package_key(db, root, opt);
        if let Err(e) = self.cache.store_package_shared(&key, emitted) {
            cache_debug(format_args!(
                "package store failed for {}: {e}",
                root.path(db).display()
            ));
        }
    }
}

/// Compile the project: every package of its program served from the cache
/// when the cache holds its current output, emitted fresh otherwise, and the
/// outputs linked.
///
/// Served outputs are the cache's own contract (their keys cover every input
/// they depend on), but a corrupt or forged entry that does not bind must
/// never fail the user: a link error with a cache falls back to the fully
/// honest compile, which reproduces a real error or the byte-identical
/// program.
pub(crate) fn compile_program(
    db: &ProjectDatabase,
    package: SourceRoot,
    cache: Option<&CacheContext>,
) -> Result<Program, CompileProgramError> {
    let Some(ctx) = cache else {
        return baml_db::compile_program(db, package, CLI_OPT_LEVEL);
    };
    match baml_db::compile_program_with(db, package, CLI_OPT_LEVEL, ctx) {
        Ok(program) => {
            cache_debug(format_args!(
                "packages: {} served, {} emitted",
                ctx.packages_served(),
                ctx.packages_emitted()
            ));
            Ok(program)
        }
        Err(CompileProgramError::Link(error)) => {
            cache_debug(format_args!(
                "served packages failed to link; recompiling honestly: {error}"
            ));
            baml_db::compile_program(db, package, CLI_OPT_LEVEL)
        }
        Err(error) => Err(error),
    }
}

// ── The check layer ──────────────────────────────────────────────────────────

/// The previous compile's check rows, servable because the project is
/// byte-identical to the compile that wrote them: every user file's
/// diagnostics blob and `callable_throws` fragment, and the seeds the
/// database installs before the first typecheck query.
pub(crate) struct ServedChecks {
    /// Every user file's cached diagnostics blob, by rel path. Rehydrated to
    /// serve the file without re-checking, and copied verbatim into the next
    /// manifest.
    pub(crate) diagnostics: BTreeMap<String, Vec<u8>>,
    /// Every user file's `CallableThrowsFragment` blob, by rel path — the
    /// source of the `callable_throws` seeds, and what the sampled verify
    /// compares against an honest derivation.
    pub(crate) fragments: BTreeMap<String, Vec<u8>>,
    /// Per-file throw facts (full path → facts), installed by
    /// [`Self::install_seeds`].
    throw_facts:
        BTreeMap<String, Vec<baml_type::throw_facts::FunctionThrowFacts<baml_type::TypeName>>>,
    /// Per-function `callable_throws` seeds projected from `fragments`, keyed
    /// by full source path then by item-tree `LocalItemId::as_u32`. Empty
    /// under `BAML_NO_CALLABLE_THROWS_CACHE=1`.
    callable_throws: BTreeMap<String, BTreeMap<u32, baml_type::Ty<baml_type::TypeName>>>,
}

impl ServedChecks {
    /// Install the type-inference seeds so no served file re-walks its bodies
    /// to answer what it throws. The seeds are byte-for-byte the values the
    /// manifest was stored with, and the solve that consumes them is a
    /// deterministic pure function of the (identical) sources, so a served
    /// value is exactly the honest one.
    fn install_seeds(&mut self, db: &mut ProjectDatabase) {
        db.set_seeded_throw_facts(std::mem::take(&mut self.throw_facts));
        db.set_seeded_callable_throws(std::mem::take(&mut self.callable_throws));
    }
}

/// The result of the warm-database preamble ([`CacheContext::prepare_warm_db`]).
pub(crate) struct WarmPrep {
    /// The served check rows (`None` when the project changed since the last
    /// compile, so every file is checked honestly).
    pub(crate) served: Option<ServedChecks>,
    /// Whether the stdlib interface seed was served — run/test skip re-writing
    /// it in that case; `check` ignores this.
    pub(crate) stdlib_interface_hit: bool,
}

/// Cache diagnostics to stderr, gated on `BAML_CACHE_DEBUG=1`. For support
/// and perf triage: shows what was served, fallback reasons, and store
/// failures without affecting normal output.
#[allow(clippy::print_stderr)] // opt-in debug channel (BAML_CACHE_DEBUG=1)
pub(crate) fn cache_debug(args: std::fmt::Arguments<'_>) {
    if env_flag("BAML_CACHE_DEBUG") {
        eprintln!("[baml-cache] {args}");
    }
}

/// Root-relative display path for a user `SourceFile`, or `None` for a stdlib
/// (`<builtin>/`) file.
fn user_rel_path(db: &ProjectDatabase, root: &Path, sf: SourceFile) -> Option<String> {
    let path = sf.path(db);
    if path.to_string_lossy().starts_with("<builtin>/") {
        return None;
    }
    Some(rel_path(root, &path))
}

/// The user's source files — those of `package`, the root every cache
/// rel-path is keyed against — with their root-relative paths.
fn user_files_with_rel_paths(
    db: &ProjectDatabase,
    package: SourceRoot,
) -> Vec<(SourceFile, String)> {
    let root = package.path(db);
    package
        .files(db)
        .iter()
        .filter_map(|&sf| user_rel_path(db, root, sf).map(|rel| (sf, rel)))
        .collect()
}

/// Project each served file's fragment into a per-function `callable_throws`
/// seed map, keyed by full source path then by item-tree `LocalItemId::as_u32`
/// (the fragment's key form). A fragment that is empty or fails to decode is
/// skipped — its functions then infer honestly (degrade, never miscompile).
/// Empty under `BAML_NO_CALLABLE_THROWS_CACHE=1`.
fn project_callable_throws_seeds(
    fragments: &BTreeMap<String, Vec<u8>>,
    root: &Path,
) -> BTreeMap<String, BTreeMap<u32, baml_type::Ty<baml_type::TypeName>>> {
    if CacheContext::callable_throws_cache_disabled() {
        return BTreeMap::new();
    }
    use baml_db::baml_compiler2_hir_ty::package_interface::CallableThrowsFragment;
    let mut by_path = BTreeMap::new();
    for (rel, fragment_bytes) in fragments {
        if fragment_bytes.is_empty() {
            continue;
        }
        let fragment: CallableThrowsFragment<baml_type::TypeName> =
            match borsh::from_slice(fragment_bytes) {
                Ok(f) => f,
                Err(e) => {
                    cache_debug(format_args!(
                        "interface fragment for `{rel}` undecodable: {e}"
                    ));
                    continue;
                }
            };
        if fragment.by_id.is_empty() {
            continue;
        }
        let full = root.join(rel).display().to_string();
        by_path.insert(full, fragment.by_id);
    }
    by_path
}

impl CacheContext {
    /// The previous compile's check rows, if the project is byte-identical to
    /// the compile that wrote them.
    ///
    /// The manifest records the program key of that compile, and this
    /// context's key folds in every compile input (compiler build, options,
    /// `baml.toml`, every file's path and content), so equal keys prove every
    /// row current — the only condition under which a per-file row can be
    /// served soundly, since a file's diagnostics and throws depend on every
    /// declaration it resolves. `None` on a first compile, a changed
    /// project, a missing or undecodable manifest, or under
    /// `BAML_CACHE_VERIFY` (the oracle exercises the honest path).
    pub(crate) fn served_checks(
        &self,
        db: &ProjectDatabase,
        package: SourceRoot,
    ) -> Option<ServedChecks> {
        if Self::verify_enabled() {
            return None;
        }
        let manifest: ProjectManifest = self.load_decoded(&self.manifest_key, "manifest")?;
        if manifest.program_key != *self.key.as_bytes() {
            cache_debug(format_args!(
                "project changed since the last compile — every file is checked honestly"
            ));
            return None;
        }
        let root = package.path(db);
        let mut diagnostics = BTreeMap::new();
        let mut fragments = BTreeMap::new();
        let mut throw_facts = BTreeMap::new();
        for entry in manifest.files {
            let full = root.join(&entry.rel_path).display().to_string();
            throw_facts.insert(full, entry.throw_facts);
            fragments.insert(entry.rel_path.clone(), entry.callable_throws_fragment);
            diagnostics.insert(entry.rel_path, entry.diagnostics);
        }
        cache_debug(format_args!(
            "check rows served for {} file(s)",
            diagnostics.len()
        ));
        let callable_throws = project_callable_throws_seeds(&fragments, root);
        Some(ServedChecks {
            diagnostics,
            fragments,
            throw_facts,
            callable_throws,
        })
    }

    /// Seed the immutable stdlib typed interface (gated off under verify so the
    /// oracle exercises the honest path) and install the served check rows'
    /// seeds — the identical warm-database setup `run`, `test`, `check`, and
    /// the introspection commands each run before their first query.
    pub(crate) fn prepare_warm_db(
        &self,
        db: &mut ProjectDatabase,
        package: SourceRoot,
    ) -> WarmPrep {
        let stdlib_interface_hit = self.seed_stdlib_interface(db);
        let served = self.served_checks(db, package).map(|mut served| {
            served.install_seeds(db);
            served
        });
        WarmPrep {
            served,
            stdlib_interface_hit,
        }
    }

    /// Write the Program blob plus the manifest describing it. Called after
    /// every successful compile; best-effort like all cache writes.
    ///
    /// `fresh_by_file` holds the diagnostics blob for every file the gate freshly
    /// checked, by rel_path. Per file the manifest takes its fresh blob if
    /// present, else the served blob, else an empty blob — so a re-checked file
    /// always overwrites a stale/poison carry.
    pub(crate) fn store_program_and_manifest(
        &self,
        db: &ProjectDatabase,
        package: SourceRoot,
        program: &Program,
        fresh_by_file: &BTreeMap<String, Vec<u8>>,
        served: Option<&ServedChecks>,
    ) -> std::io::Result<()> {
        use baml_db::baml_compiler2_hir_ty::{
            package_interface::export_callable_throws_fragment,
            throw_facts::export_file_throw_facts,
        };

        self.store(program)?;

        let mut files: Vec<ManifestFile> = Vec::new();
        for (sf, rel) in user_files_with_rel_paths(db, package) {
            // The fragment holds no absolute paths; on the just-compiled
            // database every function's throws are memoized, so exporting it
            // pulls no fresh inference.
            let callable_throws_fragment = borsh::to_vec(&export_callable_throws_fragment(db, sf))
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            files.push(ManifestFile {
                // Free: seeded files return their seeds verbatim, freshly
                // checked files were extracted (and memoized) during the
                // compile. Spelled for the wire: every head by its root's
                // spelling in this database, which is what the next compile
                // seeds.
                throw_facts: export_file_throw_facts(db, sf),
                // Fresh blob if the gate re-checked this file, else the served
                // blob, else empty. A re-checked file always wins so a
                // stale/poison carry can't persist.
                diagnostics: fresh_by_file
                    .get(&rel)
                    .cloned()
                    .or_else(|| served.and_then(|served| served.diagnostics.get(&rel).cloned()))
                    .unwrap_or_else(crate::diagnostics_cache::empty_blob),
                callable_throws_fragment,
                rel_path: rel,
            });
        }
        files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

        let manifest = ProjectManifest {
            program_key: *self.key.as_bytes(),
            files,
        };
        let payload = borsh::to_vec(&manifest)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.cache.store_raw(&self.manifest_key, &payload)
    }

    /// The warm-path verify-and-store sequence shared by `run`, `test`,
    /// `pack`, and a seeding `check` for `session`'s compile: run every
    /// `BAML_CACHE_VERIFY` oracle, persist the Program and the manifest,
    /// materialize the stdlib interface (unless it was already served) and the
    /// per-toolchain builtin-diagnostics blob, then run the sampled
    /// field-verify on a fresh un-seeded database over the session's sources.
    pub(crate) fn verify_and_store(
        &self,
        session: &ProjectSession,
        program: &Program,
        fresh: &BTreeMap<String, Vec<u8>>,
        served: Option<&ServedChecks>,
        stdlib_interface_hit: bool,
    ) -> anyhow::Result<()> {
        let (db, package) = (&session.db, session.package);
        self.verify_against(program)?;
        self.verify_stdlib_interface(db)?;
        self.verify_diagnostics(db, package)?;
        self.verify_stdlib_diagnostics(db)?;
        self.verify_callable_throws_fragments(db, package)?;
        // A store failure never fails the command (the compile succeeded), but
        // it is LOUD on the debug channel, never silent.
        if let Err(e) = self.store_program_and_manifest(db, package, program, fresh, served) {
            cache_debug(format_args!("bytecode cache write failed: {e}"));
        }
        // Materialize the stdlib interface blob on a miss (idempotent on a hit,
        // so only write when the seed was absent).
        if !stdlib_interface_hit {
            self.store_stdlib_interface(db);
        }
        // Materialize the per-toolchain builtin-diagnostics blob on a miss
        // (self-gating on blob presence, so a warm hit is a no-op write).
        self.store_stdlib_diagnostics(db);
        // Sampled field verification (rustc-style 1-in-32): now that the compile
        // result exists, ~1 warm run in 32 re-derives one served file on a
        // fresh, un-seeded database and hard-errors on any drift. Bounded
        // latency (one file's honest work), loud on the silent-staleness bug
        // class the full `BAML_CACHE_VERIFY` guards.
        self.maybe_sampled_verify(served, || session.honest_db())?;
        Ok(())
    }

    /// Read-only diagnostics collector for `baml check`. It serves the same
    /// rows as run/test, but does not serialize fresh blobs: a check-only
    /// manifest cannot advance without a stored program.
    pub(crate) fn collect_diagnostics_for_check(
        &self,
        db: &ProjectDatabase,
        package: SourceRoot,
        served: Option<&ServedChecks>,
    ) -> Vec<baml_db::baml_compiler_diagnostics::Diagnostic> {
        self.collect_diagnostics_with_plan(db, package, self.serve_plan(served), false)
            .merged
    }

    /// Gate diagnostics on the warm path: serve every user file from its
    /// row when the rows are current, run `check_file` for the rest (and for
    /// the builtins unless the per-toolchain blob covers them), and return the
    /// merged set plus the fresh per-file blobs to persist. With `served ==
    /// None` (first compile / edited project / verify) this reduces to the
    /// honest full check, so the merged set always equals the honest
    /// collector and the caller renders errors identically.
    pub(crate) fn collect_diagnostics_incremental(
        &self,
        db: &ProjectDatabase,
        package: SourceRoot,
        served: Option<&ServedChecks>,
    ) -> IncrementalDiagnostics {
        self.collect_diagnostics_with_plan(db, package, self.serve_plan(served), true)
    }

    /// The rows to serve, unless serving is disabled (the isolation toggle
    /// mirrors `BAML_NO_STDLIB_INTERFACE_CACHE`: dropping the served rows
    /// checks every file, so a with/without comparison measures exactly the
    /// files this tier skips).
    fn serve_plan<'a>(&self, served: Option<&'a ServedChecks>) -> Option<DiagnosticsServePlan<'a>> {
        served
            .filter(|_| !Self::diagnostics_cache_disabled())
            .map(|served| DiagnosticsServePlan {
                diagnostics: &served.diagnostics,
            })
    }

    fn collect_diagnostics_with_plan(
        &self,
        db: &ProjectDatabase,
        package: SourceRoot,
        plan: Option<DiagnosticsServePlan<'_>>,
        persist_fresh: bool,
    ) -> IncrementalDiagnostics {
        let root = package.path(db);

        // Rehydrate the served files' diagnostics; a file that fails to
        // rehydrate degrades to a re-check (its blob is never served stale).
        let mut precomputed: Vec<baml_db::baml_compiler_diagnostics::Diagnostic> = Vec::new();
        let mut degrade: HashSet<String> = HashSet::new();
        if let Some(plan) = &plan {
            for (rel, blob) in plan.diagnostics {
                match crate::diagnostics_cache::rehydrate_file_blob(db, root, blob) {
                    Some(mut diags) => precomputed.append(&mut diags),
                    None => {
                        degrade.insert(rel.clone());
                    }
                }
            }
        }

        // Serve the builtin (stdlib) diagnostics from the per-toolchain constant
        // blob when present: the builtins then drop out of `should_check` below
        // and their (usually empty) diagnostics fold into `precomputed`, exactly
        // like a served user file's blob. This removes the ~1,900-scope stdlib
        // re-inference tail from every warm compile. It is independent of the
        // served rows (even a first-ever compile of a project can serve a blob
        // written by any earlier compile on the same toolchain). A missing /
        // corrupt blob, `BAML_NO_DIAGNOSTICS_CACHE`, and `BAML_CACHE_VERIFY` all
        // fall through to the honest builtin check (`load_stdlib_diagnostics`
        // gates the disable knob; verify is gated here so its oracle exercises
        // the honest path).
        let serve_builtins = !Self::verify_enabled()
            && self
                .load_stdlib_diagnostics()
                .and_then(|blob| crate::diagnostics_cache::rehydrate_builtin_blob(db, &blob))
                .map(|mut diags| precomputed.append(&mut diags))
                .is_some();

        let rel_of = |sf: SourceFile| user_rel_path(db, root, sf);
        let should_check = |sf: SourceFile| -> bool {
            match rel_of(sf) {
                // Builtin: served from the per-toolchain constant blob when
                // present (its diagnostics are already folded into `precomputed`);
                // otherwise checked honestly, and its blob stored afterward.
                None => !serve_builtins,
                Some(rel) => match &plan {
                    Some(plan) => !plan.diagnostics.contains_key(&rel) || degrade.contains(&rel),
                    None => true,
                },
            }
        };

        let narrowed =
            baml_db::collect_compiler2_diagnostics_narrowed(db, &should_check, precomputed);

        let mut fresh_by_file = if persist_fresh {
            crate::diagnostics_cache::fresh_blobs_by_file(db, root, &narrowed.fresh)
        } else {
            BTreeMap::new()
        };
        // Ensure every re-checked user file has an entry (empty if it produced
        // no diagnostics) so `store_program_and_manifest` overwrites a
        // stale/poison carry for a degraded-but-now-clean file rather than
        // re-carrying it.
        if persist_fresh {
            for (sf, rel) in user_files_with_rel_paths(db, package) {
                if should_check(sf) {
                    fresh_by_file
                        .entry(rel)
                        .or_insert_with(crate::diagnostics_cache::empty_blob);
                }
            }
        }

        IncrementalDiagnostics {
            merged: narrowed.merged,
            fresh_by_file,
        }
    }

    /// Localized diagnostics verify oracle (analog of `verify_stdlib_interface`):
    /// under `BAML_CACHE_VERIFY` no row is served, so the compile runs the
    /// honest full check; this compares every row a warm run would have served
    /// against a fresh `check_file`. A mismatch means the cached diagnostics are
    /// a stale substitute that would change what a warm run reports — a hard
    /// error, and a tighter signal than the whole-`Program` byte-compare.
    pub(crate) fn verify_diagnostics(
        &self,
        db: &ProjectDatabase,
        package: SourceRoot,
    ) -> anyhow::Result<()> {
        if !Self::verify_enabled() {
            return Ok(());
        }
        self.check_cached_diagnostics_against_fresh(db, package)
    }

    /// The env-independent core of [`Self::verify_diagnostics`], so the oracle's
    /// discriminating power (pass on a faithful cache, bail on a stale one) is
    /// unit-testable without mutating the process environment.
    pub(crate) fn check_cached_diagnostics_against_fresh(
        &self,
        db: &ProjectDatabase,
        package: SourceRoot,
    ) -> anyhow::Result<()> {
        let Some(manifest) = self.servable_manifest_for_verify() else {
            return Ok(());
        };
        let root = package.path(db);
        for entry in &manifest.files {
            let full = root.join(&entry.rel_path);
            let Some(sf) = db.get_file(&full) else {
                continue; // file removed — never served
            };
            let Some(served) =
                crate::diagnostics_cache::rehydrate_file_blob(db, root, &entry.diagnostics)
            else {
                continue; // poison / undecodable — would degrade to a re-check
            };
            // What an honest run produces for this file: `check_file` output only
            // (the package-level set is never cached, so it is excluded here too).
            let fresh = db.check_file(sf);
            if !diagnostic_sets_equal(&served, &fresh) {
                anyhow::bail!(
                    "BAML_CACHE_VERIFY: cached diagnostics for `{}` differ from a fresh check \
                     ({} cached vs {} fresh). The cached per-file diagnostics are a stale \
                     substitute — please report this.",
                    entry.rel_path,
                    served.len(),
                    fresh.len(),
                );
            }
        }
        Ok(())
    }

    /// The manifest whose rows a warm run of this project would serve — the
    /// one written for this compile's program key — for the verify oracles,
    /// bypassing the `served_checks` verify short-circuit (verify must still
    /// compare against whatever is on disk). `None` when no row would be
    /// served, so there is nothing to compare.
    fn servable_manifest_for_verify(&self) -> Option<ProjectManifest> {
        let manifest: ProjectManifest = self.load_decoded(&self.manifest_key, "manifest")?;
        (manifest.program_key == *self.key.as_bytes()).then_some(manifest)
    }

    /// Localized `callable_throws`-seed verify oracle (analog of
    /// `verify_stdlib_interface` / `verify_diagnostics`): under
    /// `BAML_CACHE_VERIFY` no row is served, so `db` derives every fragment
    /// honestly. This compares each fragment a warm run would have seeded from
    /// against that honest re-derivation; whole-fragment equality checks the
    /// exact seed-faithfulness invariant, not a heuristic.
    pub(crate) fn verify_callable_throws_fragments(
        &self,
        db: &ProjectDatabase,
        package: SourceRoot,
    ) -> anyhow::Result<()> {
        if !Self::verify_enabled() {
            return Ok(());
        }
        self.check_callable_throws_fragments_against_honest(db, package)
    }

    /// The env-independent core of [`Self::verify_callable_throws_fragments`], so the
    /// oracle's discriminating power (pass on a faithful fragment, bail on a
    /// stale one) is unit-testable without mutating the process environment.
    pub(crate) fn check_callable_throws_fragments_against_honest(
        &self,
        db: &ProjectDatabase,
        package: SourceRoot,
    ) -> anyhow::Result<()> {
        let Some(manifest) = self.servable_manifest_for_verify() else {
            return Ok(());
        };
        let root = package.path(db);
        for entry in &manifest.files {
            if entry.callable_throws_fragment.is_empty() {
                continue;
            }
            let full = root.join(&entry.rel_path);
            let Some(sf) = db.get_file(&full) else {
                continue; // file removed — never seeded
            };
            let honest =
                baml_db::baml_compiler2_hir_ty::package_interface::export_callable_throws_fragment(
                    db, sf,
                );
            let honest_bytes = borsh::to_vec(&honest).map_err(|e| {
                anyhow::anyhow!(
                    "honest interface fragment for `{}` failed to serialize: {e}",
                    entry.rel_path
                )
            })?;
            if honest_bytes != entry.callable_throws_fragment {
                anyhow::bail!(
                    "BAML_CACHE_VERIFY: cached interface fragment for `{}` differs from a fresh \
                     derivation ({} cached vs {} fresh bytes). A served file's stored fragment is \
                     a stale substitute — the seeded value would be wrong. Please report this.",
                    entry.rel_path,
                    entry.callable_throws_fragment.len(),
                    honest_bytes.len(),
                );
            }
        }
        Ok(())
    }

    /// The `BAML_CACHE_SAMPLED_VERIFY` knob: `Some(false)` disables sampling,
    /// `Some(true)` forces it on every warm compile (tests want determinism),
    /// `None` leaves the default 1/32 gate. Any other value is treated as unset.
    fn sampled_verify_force() -> Option<bool> {
        match std::env::var_os("BAML_CACHE_SAMPLED_VERIFY") {
            Some(v) if v == "0" => Some(false),
            Some(v) if v == "1" => Some(true),
            _ => None,
        }
    }

    /// Pick the one served file this compile should sample-verify, keyed off
    /// the program cache key (see [`sampled_pick_from_key`]). `None` when
    /// verify mode is active (it already does the full compare), this compile
    /// isn't sampled, or nothing is served.
    fn sampled_verify_pick(&self, served: &ServedChecks) -> Option<String> {
        if Self::verify_enabled() {
            return None;
        }
        let served_files: Vec<&str> = served.diagnostics.keys().map(String::as_str).collect();
        sampled_pick_from_key(
            self.key.as_bytes(),
            &served_files,
            Self::sampled_verify_force(),
        )
        .map(str::to_string)
    }

    /// Sampled field verification driver (the always-on tripwire). If this warm
    /// compile is sampled, build an honest database — deferred to `FnOnce` so an
    /// unsampled compile pays nothing — and verify one served file's artifacts
    /// against it. Meant to be called *after* the compile result is produced,
    /// so user-visible latency grows by at most one file's honest work.
    ///
    /// `build_honest_db` MUST produce a FRESH database from the same sources
    /// with NO seeds installed (throw facts, `callable_throws`, diagnostics).
    /// The compile database is seeded with exactly those, so re-deriving on it
    /// would compare a served seed against itself and prove nothing — the
    /// honest oracle only has teeth on a database that derives the user
    /// artifacts itself, exactly as full verify gets by serving nothing. (The
    /// stdlib build-constant *is* seeded below, for speed; that is orthogonal to
    /// user-file staleness — see the seeding comment.)
    pub(crate) fn maybe_sampled_verify(
        &self,
        served: Option<&ServedChecks>,
        build_honest_db: impl FnOnce() -> (ProjectDatabase, SourceRoot),
    ) -> anyhow::Result<()> {
        let Some(served) = served else {
            // Cold compile or a whole-image cache hit: nothing was served
            // row by row, so there is nothing to sample. (A pure hit serves
            // the program blob; sampling it would require a full honest
            // compile — unacceptable — so hits stay unsampled by design.)
            return Ok(());
        };
        let Some(rel) = self.sampled_verify_pick(served) else {
            return Ok(());
        };
        cache_debug(format_args!("sampled field verify: checking `{rel}`"));
        let (mut honest_db, honest_package) = build_honest_db();
        // Seed only the stdlib typed interface — a compiler-build constant keyed
        // by fingerprint, guarded by its own `verify_stdlib_interface` oracle and
        // unable to go stale across a warm edit. Without it, an honest per-file
        // check on a fresh database re-derives every stdlib package cold (the
        // dominant cost, ~hundreds of ms), swamping the one file's inference we
        // actually want to time. Seeding it keeps the user-file oracle fully
        // honest (the served vs honest USER artifact is still compared against
        // the same correct stdlib) while bounding latency to one file's work.
        // The user-file artifacts under test (throw facts, `callable_throws`,
        // diagnostics) are deliberately NOT seeded — that is the whole point.
        if let Some(blob) = self.load_stdlib_interface() {
            honest_db.set_seeded_stdlib_interface(blob);
        }
        self.verify_sampled_artifact(&honest_db, honest_package, served, &rel)
    }

    /// Compare one served file's diagnostics blob and `callable_throws`
    /// fragment against an honest re-derivation on `honest_db` (which must be
    /// un-seeded — see [`Self::maybe_sampled_verify`]). A mismatch is a hard
    /// error: the cache served a stale artifact, so the user's warm build
    /// silently diverged from an honest compile. The message points at
    /// `BAML_CACHE_VERIFY=1` (the full byte-compare) and names the file and
    /// artifact kind, mirroring rustc's ICE-on-fingerprint-mismatch.
    pub(crate) fn verify_sampled_artifact(
        &self,
        honest_db: &ProjectDatabase,
        honest_package: SourceRoot,
        served: &ServedChecks,
        rel: &str,
    ) -> anyhow::Result<()> {
        let root = honest_package.path(honest_db);
        let full = root.join(rel);
        let Some(sf) = honest_db.get_file(&full) else {
            return Ok(()); // file vanished between planning and verify — unserved
        };

        // (1) Served diagnostics blob vs a fresh per-file check. A blob that
        // fails to rehydrate would have degraded to a re-check (never served
        // stale), so it is not a mismatch — skip it, as the full oracle does.
        if let Some(blob) = served.diagnostics.get(rel)
            && let Some(served_diagnostics) =
                crate::diagnostics_cache::rehydrate_file_blob(honest_db, root, blob)
        {
            let fresh = honest_db.check_file(sf);
            if !diagnostic_sets_equal(&served_diagnostics, &fresh) {
                anyhow::bail!(
                    "BAML_CACHE_SAMPLED_VERIFY: the cache served STALE diagnostics for `{rel}` \
                     ({} served vs {} from an honest check). This is a cache-soundness bug — a \
                     warm build would report different errors than a clean one. Re-run with \
                     BAML_CACHE_VERIFY=1 for the full compare and please report this (file \
                     `{rel}`, artifact: diagnostics).",
                    served_diagnostics.len(),
                    fresh.len(),
                );
            }
        }

        // (2) Served `callable_throws` fragment vs an honest derivation. An
        // empty fragment seeds nothing, so there is no served artifact to check.
        if let Some(fragment) = served.fragments.get(rel)
            && !fragment.is_empty()
        {
            let honest =
                baml_db::baml_compiler2_hir_ty::package_interface::export_callable_throws_fragment(
                    honest_db, sf,
                );
            let honest_bytes = borsh::to_vec(&honest)?;
            if honest_bytes != *fragment {
                anyhow::bail!(
                    "BAML_CACHE_SAMPLED_VERIFY: the cache served a STALE callable-throws seed for \
                     `{rel}` ({} served vs {} honest bytes). This is a cache-soundness bug — a \
                     warm build would infer different throws than a clean one. Re-run with \
                     BAML_CACHE_VERIFY=1 for the full compare and please report this (file \
                     `{rel}`, artifact: callable-throws fragment).",
                    fragment.len(),
                    honest_bytes.len(),
                );
            }
        }
        Ok(())
    }

    /// Install the immutable stdlib typed-interface seed (a per-toolchain
    /// build constant). Returns whether the seed was served; gated off under
    /// `BAML_CACHE_VERIFY` so the oracle exercises the honest path.
    pub(crate) fn seed_stdlib_interface(&self, db: &mut ProjectDatabase) -> bool {
        !Self::verify_enabled()
            && self
                .load_stdlib_interface()
                .map(|by_package| db.set_seeded_stdlib_interface(by_package))
                .is_some()
    }

    /// Test hook: overwrite one file's cached diagnostics with an empty blob,
    /// simulating a stale cache that dropped a diagnostic. Used by the verify
    /// oracle's negative test.
    #[cfg(test)]
    pub(crate) fn poison_manifest_diagnostics_for_test(&self, rel_path: &str) {
        self.set_manifest_diagnostics_for_test(rel_path, crate::diagnostics_cache::empty_blob());
    }

    #[cfg(test)]
    pub(crate) fn corrupt_manifest_diagnostics_for_test(&self, rel_path: &str) {
        self.set_manifest_diagnostics_for_test(rel_path, vec![0xff]);
    }

    #[cfg(test)]
    fn set_manifest_diagnostics_for_test(&self, rel_path: &str, diagnostics: Vec<u8>) {
        self.edit_manifest_row_for_test(rel_path, |row| row.diagnostics = diagnostics);
    }

    /// Test hook: overwrite one file's stored `callable_throws` fragment with
    /// a poisoned one. Used by the fragment verify oracle's negative test.
    #[cfg(test)]
    pub(crate) fn poison_callable_throws_fragment_for_test(
        &self,
        rel_path: &str,
        fragment: Vec<u8>,
    ) {
        self.edit_manifest_row_for_test(rel_path, |row| row.callable_throws_fragment = fragment);
    }

    #[cfg(test)]
    fn edit_manifest_row_for_test(&self, rel_path: &str, edit: impl FnOnce(&mut ManifestFile)) {
        let bytes = self
            .cache
            .load_raw(&self.manifest_key)
            .expect("manifest present");
        let mut manifest: ProjectManifest = borsh::from_slice(&bytes).expect("manifest decodes");
        let row = manifest
            .files
            .iter_mut()
            .find(|file| file.rel_path == rel_path)
            .expect("manifest file present");
        edit(row);
        let payload = borsh::to_vec(&manifest).expect("manifest serializes");
        self.cache
            .store_raw(&self.manifest_key, &payload)
            .expect("manifest re-stored");
    }
}

#[derive(Clone, Copy)]
struct DiagnosticsServePlan<'a> {
    /// The served files' cached diagnostics blobs, by rel path.
    diagnostics: &'a BTreeMap<String, Vec<u8>>,
}

/// The merged (gate/render) diagnostics plus the per-file blobs to persist,
/// produced by [`CacheContext::collect_diagnostics_incremental`].
pub(crate) struct IncrementalDiagnostics {
    pub(crate) merged: Vec<baml_db::baml_compiler_diagnostics::Diagnostic>,
    pub(crate) fresh_by_file: BTreeMap<String, Vec<u8>>,
}

/// Order-independent equality of two diagnostic sets — the verify oracle
/// compares cached vs freshly-checked, which agree as sets but may differ in
/// vector order after a `FileId` remap.
fn diagnostic_sets_equal(
    a: &[baml_db::baml_compiler_diagnostics::Diagnostic],
    b: &[baml_db::baml_compiler_diagnostics::Diagnostic],
) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let key = |d: &baml_db::baml_compiler_diagnostics::Diagnostic| {
        let span = d.primary_span();
        (
            d.code(),
            d.message.clone(),
            span.map(|s| (s.file_id.as_u32(), u32::from(s.range.start()))),
        )
    };
    let mut a_sorted: Vec<_> = a.iter().collect();
    let mut b_sorted: Vec<_> = b.iter().collect();
    a_sorted.sort_by_key(|d| key(d));
    b_sorted.sort_by_key(|d| key(d));
    a_sorted.iter().zip(&b_sorted).all(|(x, y)| x == y)
}

/// The RNG-free sampled-verify decision, derived entirely from the program
/// cache key (`compute_key`'s sha256 over every compile input). Returns the
/// chosen served file — `None` when this compile is not sampled or nothing is
/// served.
///
/// No clock and no `rand`: the key already varies with the sources, so
/// deriving the choice from it makes sampling **deterministic** for a given
/// project state (same edit → same decision → reproducible, debuggable — a
/// field mismatch report can be replayed exactly) yet naturally spread across
/// different states (each distinct edit re-rolls both the gate and the index).
///
/// - Whether to sample: `key[0] & 31 == 0` — one value in 32, so ~1/32 of warm
///   compiles pay the check. `force = Some(true/false)` overrides the gate for
///   the `BAML_CACHE_SAMPLED_VERIFY=1/0` knobs.
/// - Which file: bytes 1..5 of the key (independent of the gate byte) index the
///   **sorted** served set, so the pick is stable regardless of set iteration
///   order.
fn sampled_pick_from_key<'a>(
    key: &[u8; 32],
    served_sorted: &[&'a str],
    force: Option<bool>,
) -> Option<&'a str> {
    let sample = match force {
        Some(forced) => forced,
        None => key[0] & 31 == 0,
    };
    if !sample || served_sorted.is_empty() {
        return None;
    }
    let idx = u32::from_le_bytes([key[1], key[2], key[3], key[4]]) as usize % served_sorted.len();
    Some(served_sorted[idx])
}

#[cfg(test)]
mod tests {
    //! Soundness of the three tiers: a warm compile serves every package and
    //! reproduces an honest compile byte for byte; an edit re-emits only the
    //! user package; the check rows are served only for an identical project;
    //! the stdlib typed-interface and builtin-diagnostics blobs; the fragment
    //! and diagnostics `BAML_CACHE_VERIFY` oracles (a faithful cache passes, a
    //! poisoned one bails); the discovery cache; sampled verification.

    use std::path::{Path, PathBuf};

    use super::*;
    use crate::cache_test_support::{cache_disabled, compile_and_store_v1, resolved, unique_root};

    /// A unique on-disk root for a `bytecode_cache` disk-round-trip test.
    fn bc_root() -> PathBuf {
        unique_root("baml-bc-cache-test")
    }

    fn build_db(files: &[(&str, &str)]) -> (ProjectDatabase, SourceRoot) {
        let root = Path::new("/bc-test");
        let (mut db, workspace) = crate::project_load::workspace_db(root);
        for (name, content) in files {
            db.add_or_update_file_in(workspace, &root.join(name), content);
        }
        (db, workspace)
    }

    fn file_named(db: &ProjectDatabase, name: &str) -> SourceFile {
        db.workspace_files()
            .into_iter()
            .find(|sf| sf.path(db).to_string_lossy().ends_with(name))
            .expect("source file present")
    }

    /// The number of packages a compile of `package` links: every root of its
    /// world with files.
    fn linked_package_count(db: &ProjectDatabase, package: SourceRoot) -> usize {
        baml_db::baml_compiler2_hir::package::world_roots(db, package)
            .iter()
            .filter(|root| !root.files(db).is_empty())
            .count()
    }

    fn program_bytes(program: &Program) -> Vec<u8> {
        borsh::to_vec(program).expect("serialize program")
    }

    /// An honest compile of `resolved`'s sources on a fresh database with no
    /// cache: the oracle every cached compile must reproduce byte for byte.
    fn honest_bytes(resolved: &ResolvedProject) -> Vec<u8> {
        let (db, package) = crate::project_load::build_db_from_sources(resolved, |_| {});
        program_bytes(&baml_db::compile_program(&db, package, CLI_OPT_LEVEL).expect("honest"))
    }

    const A_V1: &str = "function a() -> int {\n  1\n}\n";
    const A_V2: &str = "function a() -> int {\n  10 + 1\n}\n";
    const B: &str = "function b() -> int {\n  2\n}\n";

    // ── The per-package tier ─────────────────────────────────────────────

    #[test]
    fn warm_compile_serves_every_package_byte_identically() {
        if cache_disabled() {
            return;
        }
        let root = bc_root();
        let files = [("a.baml", A_V1), ("b.baml", B)];
        let _ = compile_and_store_v1(&root, &files);

        let r2 = resolved(&root, &files);
        let (db2, pkg2) = crate::project_load::build_db_from_sources(&r2, |_| {});
        let ctx2 = CacheContext::open(&r2).expect("cache reopens");
        let program = compile_program(&db2, pkg2, Some(&ctx2)).expect("warm compile");
        assert_eq!(
            ctx2.packages_served(),
            linked_package_count(&db2, pkg2),
            "an unchanged project serves every package from the cache"
        );
        assert_eq!(ctx2.packages_emitted(), 0, "nothing is emitted fresh");
        assert_eq!(
            program_bytes(&program),
            honest_bytes(&r2),
            "a compile linked from served packages is byte-identical to an honest compile"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn edit_reemits_only_the_user_package() {
        if cache_disabled() {
            return;
        }
        let root = bc_root();
        let _ = compile_and_store_v1(&root, &[("a.baml", A_V1), ("b.baml", B)]);

        let edited = [("a.baml", A_V2), ("b.baml", B)];
        let r2 = resolved(&root, &edited);
        let (db2, pkg2) = crate::project_load::build_db_from_sources(&r2, |_| {});
        let ctx2 = CacheContext::open(&r2).expect("cache reopens");
        let program = compile_program(&db2, pkg2, Some(&ctx2)).expect("warm compile");
        assert_eq!(
            ctx2.packages_emitted(),
            1,
            "only the user package's sources changed, so only it is emitted"
        );
        assert_eq!(
            ctx2.packages_served(),
            linked_package_count(&db2, pkg2) - 1,
            "every stdlib package is served"
        );
        assert_eq!(program_bytes(&program), honest_bytes(&r2));

        // The edited package's output is now cached too: a third compile of
        // the edited sources serves everything.
        let (db3, pkg3) = crate::project_load::build_db_from_sources(&r2, |_| {});
        let ctx3 = CacheContext::open(&r2).expect("cache reopens");
        let again = compile_program(&db3, pkg3, Some(&ctx3)).expect("warm compile");
        assert_eq!(ctx3.packages_emitted(), 0);
        assert_eq!(program_bytes(&again), program_bytes(&program));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_package_entry_is_re_emitted_identically() {
        if cache_disabled() {
            return;
        }
        let root = bc_root();
        let files = [("a.baml", A_V1), ("b.baml", B)];
        let (db1, ctx1) = compile_and_store_v1(&root, &files);
        let pkg1 = db1.workspace_root().expect("workspace root");
        // Corrupt the user package's entry in place: the header's checksum
        // rejects it, so the package is emitted fresh.
        let key = ctx1.package_key(&db1, pkg1, CLI_OPT_LEVEL);
        let hex = key.hex();
        let entry = ctx1
            .cache
            .dir()
            .join("bytecode")
            .join(&hex[..2])
            .join(format!("{hex}.bexc"));
        let mut bytes = std::fs::read(&entry).expect("package entry present");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        std::fs::write(&entry, &bytes).expect("rewrite package entry");

        let r2 = resolved(&root, &files);
        let (db2, pkg2) = crate::project_load::build_db_from_sources(&r2, |_| {});
        let ctx2 = CacheContext::open(&r2).expect("cache reopens");
        let program = compile_program(&db2, pkg2, Some(&ctx2)).expect("warm compile");
        assert_eq!(
            ctx2.packages_emitted(),
            1,
            "the corrupt entry is re-emitted"
        );
        assert_eq!(ctx2.packages_served(), linked_package_count(&db2, pkg2) - 1);
        assert_eq!(program_bytes(&program), honest_bytes(&r2));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn stdlib_package_keys_do_not_depend_on_the_project() {
        // Two contexts for two different projects (different roots, sources,
        // and program keys) key each stdlib package identically: a stdlib
        // package's inputs are all build constants, so one entry per
        // toolchain serves every project sharing a cache directory.
        let root_a = bc_root();
        let root_b = bc_root();
        let ra = resolved(&root_a, &[("a.baml", A_V1)]);
        let rb = resolved(&root_b, &[("other.baml", B), ("more.baml", A_V2)]);
        let (Some(ctx_a), Some(ctx_b)) = (CacheContext::open(&ra), CacheContext::open(&rb)) else {
            return;
        };
        assert_ne!(ctx_a.key, ctx_b.key, "the projects differ");
        let (db_a, pkg_a) = crate::project_load::build_db_from_sources(&ra, |_| {});
        let (db_b, pkg_b) = crate::project_load::build_db_from_sources(&rb, |_| {});
        let stdlib_keys = |db: &ProjectDatabase, package: SourceRoot, ctx: &CacheContext| {
            baml_db::baml_compiler2_hir::package::world_roots(db, package)
                .iter()
                .filter(|root| root.kind(db) == baml_db::SourceRootKind::Stdlib)
                .map(|&root| {
                    (
                        root.path(db).display().to_string(),
                        ctx.package_key(db, root, CLI_OPT_LEVEL),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        };
        let keys_a = stdlib_keys(&db_a, pkg_a, &ctx_a);
        let keys_b = stdlib_keys(&db_b, pkg_b, &ctx_b);
        assert!(!keys_a.is_empty(), "the program has stdlib packages");
        assert_eq!(keys_a, keys_b);
        assert_ne!(
            ctx_a.package_key(&db_a, pkg_a, CLI_OPT_LEVEL),
            ctx_b.package_key(&db_b, pkg_b, CLI_OPT_LEVEL),
            "the user packages differ"
        );
        let _ = std::fs::remove_dir_all(&root_a);
        let _ = std::fs::remove_dir_all(&root_b);
    }

    // ── The check layer ──────────────────────────────────────────────────

    #[test]
    fn check_rows_are_served_only_for_an_identical_project() {
        if cache_disabled() {
            return;
        }
        let root = bc_root();
        let files = [("a.baml", A_V1), ("b.baml", B)];
        let _ = compile_and_store_v1(&root, &files);

        let same = resolved(&root, &files);
        let (db_same, pkg_same) = crate::project_load::build_db_from_sources(&same, |_| {});
        let ctx_same = CacheContext::open(&same).expect("cache reopens");
        let served = ctx_same
            .served_checks(&db_same, pkg_same)
            .expect("an identical project serves its check rows");
        assert_eq!(
            served.diagnostics.keys().cloned().collect::<Vec<_>>(),
            vec!["a.baml".to_string(), "b.baml".to_string()],
            "every user file has a served row"
        );
        assert!(
            served
                .callable_throws
                .keys()
                .any(|path| path.ends_with("a.baml")),
            "the callable-throws seed covers the served files"
        );

        let edited = resolved(&root, &[("a.baml", A_V2), ("b.baml", B)]);
        let (db_edit, pkg_edit) = crate::project_load::build_db_from_sources(&edited, |_| {});
        let ctx_edit = CacheContext::open(&edited).expect("cache reopens");
        assert!(
            ctx_edit.served_checks(&db_edit, pkg_edit).is_none(),
            "an edited project serves no row — every file is checked honestly"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn seeded_callable_throws_is_consulted_on_path_key_hit() {
        // Guards the (abs-path, LocalItemId) seed key match end-to-end: seed one
        // function's key with a deliberately-wrong `Ty` and prove `callable_throws`
        // returns it (short-circuiting honest inference), while an unseeded
        // function still infers. If either key format drifted (rel vs abs, a
        // separator change) the seed would silently never apply and this fails.
        use baml_db::{
            baml_compiler2_hir::loc::FunctionLoc, baml_compiler2_hir_ty::callable::callable_throws,
        };

        let (mut db, _) = build_db(&[(
            "a.baml",
            "class MyErr {\n  msg string\n}\n\
             function f() -> int {\n  1\n}\n\
             function g() -> int {\n  throw MyErr { msg: \"x\" }\n}\n",
        )]);
        let file = file_named(&db, "a.baml");
        let (f_id, g_id) = {
            use baml_db::baml_compiler2_hir::item_data::{file_functions, function_data};
            let mut f_id = None;
            let mut g_id = None;
            for &loc in file_functions(&db, file) {
                match function_data(&db, loc).name.as_str() {
                    "f" => f_id = Some(loc.id(&db)),
                    "g" => g_id = Some(loc.id(&db)),
                    _ => {}
                }
            }
            (f_id.expect("f present"), g_id.expect("g present"))
        };

        // Honest values (seed empty at construction): f throws nothing, g throws
        // MyErr, so the two differ — g's throws is a distinguishable sentinel.
        let (f_honest, g_throws) = {
            let f_loc = FunctionLoc::new(&db, file, f_id);
            let g_loc = FunctionLoc::new(&db, file, g_id);
            (
                callable_throws(&db, f_loc).0.clone(),
                callable_throws(&db, g_loc).0.clone(),
            )
        };
        assert_ne!(
            f_honest, g_throws,
            "f (no throw) and g (throws MyErr) must differ honestly"
        );

        // Seed f's key with g's throw `Ty`, keyed by (abs path, f's LocalItemId).
        let abs_path = file.path(&db).display().to_string();
        let spelling = baml_db::baml_compiler2_hir::package::spelling(&db);
        let mut by_id = BTreeMap::new();
        by_id.insert(
            f_id.as_u32(),
            g_throws.map_heads(&mut |decl| spelling.wire(decl)),
        );
        let mut by_path = BTreeMap::new();
        by_path.insert(abs_path, by_id);
        db.set_seeded_callable_throws(by_path);

        let f_loc = FunctionLoc::new(&db, file, f_id);
        let g_loc = FunctionLoc::new(&db, file, g_id);
        assert_eq!(
            callable_throws(&db, f_loc).0,
            g_throws,
            "the seed must short-circuit: f returns the path-keyed seeded value"
        );
        assert_eq!(
            callable_throws(&db, g_loc).0,
            g_throws,
            "an unseeded function still infers honestly"
        );
    }

    #[test]
    fn verify_callable_throws_fragments_bails_on_stale_fragment() {
        // Seed-vs-honest divergence tripwire (unit level): corrupt a served
        // file's stored fragment and the verify core must bail, naming the
        // file. This is the single assumption the whole scheme rests on — that
        // a served seed equals the honest re-derivation.
        if cache_disabled() {
            return;
        }
        let root = bc_root();
        let files = [("a.baml", A_V1), ("b.baml", B)];
        let _ = compile_and_store_v1(&root, &files);

        // A faithful fragment cache passes the oracle.
        let r = resolved(&root, &files);
        let (db2, pkg2) = crate::project_load::build_db_from_sources(&r, |_| {});
        let ctx2 = CacheContext::open(&r).expect("cache reopens");
        ctx2.check_callable_throws_fragments_against_honest(&db2, pkg2)
            .expect("faithful fragments pass the oracle");

        // Corrupt a.baml's stored fragment; the oracle must now bail on it.
        ctx2.poison_callable_throws_fragment_for_test("a.baml", vec![0xde, 0xad, 0xbe, 0xef]);
        let (db3, pkg3) = crate::project_load::build_db_from_sources(&r, |_| {});
        let err = ctx2
            .check_callable_throws_fragments_against_honest(&db3, pkg3)
            .expect_err("a stale stored fragment must bail");
        assert!(
            err.to_string().contains("a.baml"),
            "the bail must name the drifted file; got: {err}"
        );

        // An edited project would serve nothing, so there is nothing to
        // compare: the poisoned row is not a violation there.
        let edited = resolved(&root, &[("a.baml", A_V2), ("b.baml", B)]);
        let (db4, pkg4) = crate::project_load::build_db_from_sources(&edited, |_| {});
        let ctx4 = CacheContext::open(&edited).expect("cache reopens");
        ctx4.check_callable_throws_fragments_against_honest(&db4, pkg4)
            .expect("rows that would not be served are not compared");
        let _ = std::fs::remove_dir_all(&root);
    }

    // B-694: stdlib typed-interface cache ("export data").

    #[test]
    fn stdlib_interface_is_deterministic_across_fresh_dbs() {
        // The stdlib is a compiler-build constant, so its per-package
        // `PackageInterface` must serialize byte-identically from two
        // independently-built fresh databases — the soundness foundation for
        // keying the blob by compiler fingerprint alone.
        let (db1, _) = build_db(&[("a.baml", A_V1)]);
        let (db2, _) = build_db(&[("a.baml", A_V1)]);
        let blob1 = baml_db::stdlib_prefix::stdlib_interfaces(&db1);
        let blob2 = baml_db::stdlib_prefix::stdlib_interfaces(&db2);
        assert_eq!(
            blob1, blob2,
            "stdlib interface blobs must be byte-identical across fresh databases"
        );
        // Sanity: every stdlib package is present and non-trivially populated.
        for name in baml_builtins2::stdlib_package_names().iter().copied() {
            let bytes = blob1.get(name).unwrap_or_else(|| panic!("{name} present"));
            assert!(!bytes.is_empty(), "{name} interface is non-empty");
        }
    }

    #[test]
    fn seeded_stdlib_interface_is_a_faithful_substitute() {
        // Deriving honestly, then seeding those exact bytes into a fresh db and
        // re-deriving, must reproduce the identical blob — the invariant the
        // `verify_stdlib_interface` oracle enforces (seeded == derived).
        let (cold, _) = build_db(&[("a.baml", A_V1)]);
        let cold_blob = baml_db::stdlib_prefix::stdlib_interfaces(&cold);

        let (mut warm, _) = build_db(&[("a.baml", A_V1)]);
        warm.set_seeded_stdlib_interface(cold_blob.clone());
        let warm_blob = baml_db::stdlib_prefix::stdlib_interfaces(&warm);
        assert_eq!(
            cold_blob, warm_blob,
            "a seeded stdlib interface must reproduce the honest derivation exactly"
        );
    }

    #[test]
    fn seeded_stdlib_interface_short_circuits_derivation() {
        use baml_db::{
            Name,
            baml_compiler2_hir_ty::package_interface::{
                FunctionThrowSets, PackageInterface, package_interface,
            },
        };

        // A fresh db derives a non-empty `log` interface (it exports log.info,
        // etc.). Seed a deliberately EMPTY sentinel interface for `log`; if the
        // query short-circuits on the seed it returns the empty sentinel, if it
        // ignored the seed it would derive the real, non-empty one. This proves
        // the seed is consulted (derivation skipped) without relying on the
        // process-global honest-derivation counter (racy under parallel tests).
        let (mut db, _) = build_db(&[("a.baml", A_V1)]);

        let sentinel = PackageInterface::<baml_type::TypeName> {
            types: Default::default(),
            functions: Default::default(),
            throw_sets: FunctionThrowSets {
                direct: Default::default(),
                transitive: Default::default(),
            },
            namespaces: Default::default(),
            impls: Default::default(),
        };
        let mut seed = BTreeMap::new();
        seed.insert(
            "log".to_string(),
            borsh::to_vec(&sentinel).expect("serialize sentinel"),
        );
        db.set_seeded_stdlib_interface(seed);

        let log_id = baml_db::baml_compiler2_hir::package::spelling(&db)
            .root(&Name::new("log"))
            .unwrap();
        let iface = package_interface(&db, log_id);
        assert!(
            iface.functions.is_empty() && iface.types.is_empty(),
            "seeded (empty) interface must be returned verbatim, not re-derived"
        );

        // A package that was NOT seeded still derives honestly and is non-empty.
        let baml_id = baml_db::baml_compiler2_hir::package::lang_roots(&db)
            .get(baml_db::LangPackage::Baml)
            .unwrap();
        let baml_iface = package_interface(&db, baml_id);
        assert!(
            !baml_iface.functions.is_empty() || !baml_iface.types.is_empty(),
            "an unseeded stdlib package must still derive its real interface"
        );
    }

    // ── Engine-boot floor: `baml test --list` discovery cache ────────────────

    fn sample_discovery() -> TestDiscovery {
        TestDiscovery {
            testset_leaf_names: vec![
                "root::suite::one".to_string(),
                "root::suite::two".to_string(),
                "root::nested::inner::leaf".to_string(),
            ],
        }
    }

    #[test]
    fn test_discovery_roundtrips_through_cache_context() {
        // Store then load the flattened test list through the CacheContext seam;
        // a fresh key misses, a written key round-trips byte-for-byte.
        if cache_disabled() {
            return;
        }
        let root = bc_root();
        let _ = std::fs::remove_dir_all(&root);
        let r = resolved(&root, &[("a.baml", A_V1)]);
        let ctx = CacheContext::open(&r).expect("cache opens");

        assert!(
            ctx.load_test_discovery().is_none(),
            "discovery miss on an empty cache"
        );

        let disco = sample_discovery();
        ctx.store_test_discovery(&disco);
        assert_eq!(
            ctx.load_test_discovery().as_ref(),
            Some(&disco),
            "a stored discovery blob round-trips through the cache"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_discovery_load_degrades_on_undecodable_blob() {
        // Graceful degradation: an entry that decodes as a valid cache blob but
        // is NOT a valid `TestDiscovery` (wire skew) is a silent `None`, so the
        // caller falls back to honest engine-boot discovery instead of rendering
        // garbage.
        if cache_disabled() {
            return;
        }
        let root = bc_root();
        let _ = std::fs::remove_dir_all(&root);
        let r = resolved(&root, &[("a.baml", A_V1)]);
        let ctx = CacheContext::open(&r).expect("cache opens");

        ctx.cache
            .store_raw(&ctx.test_discovery_key, b"not-a-valid-borsh-TestDiscovery")
            .expect("store raw garbage under the discovery key");
        assert!(
            ctx.load_test_discovery().is_none(),
            "an undecodable discovery blob degrades to a miss, not a crash"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compare_test_discovery_passes_on_identical() {
        // The verify oracle's env-independent core: identical discovery is a pass.
        let disco = sample_discovery();
        assert!(
            CacheContext::compare_test_discovery(&disco, &disco).is_ok(),
            "a faithful cached discovery must pass the verify core"
        );
    }

    #[test]
    fn compare_test_discovery_bails_on_testset_mismatch() {
        // A drifted testset leaf set (e.g. a nondeterministic generator) bails.
        let cached = sample_discovery();
        let mut honest = sample_discovery();
        honest.testset_leaf_names[0] = "root::suite::one-CHANGED".to_string();
        let err = CacheContext::compare_test_discovery(&cached, &honest)
            .expect_err("a testset-list mismatch must bail");
        assert!(
            err.to_string().contains("testset discovery differs"),
            "the bail message must name the testset divergence: {err}"
        );
    }

    // ── Sampled field verification (rustc-style 1-in-32) ─────────────────────

    #[test]
    fn sampled_pick_from_key_is_deterministic_gated_and_forceable() {
        let served = ["a.baml", "b.baml", "c.baml", "d.baml"];
        // byte0 low 5 bits == 0 → sampled at the default gate; bytes 1..5 == 2
        // (little-endian) → index 2 of the 4-file sorted set.
        let mut sampled_key = [0u8; 32];
        sampled_key[1] = 2;
        // byte0 low bits non-zero → NOT sampled at the default gate.
        let mut unsampled_key = [0u8; 32];
        unsampled_key[0] = 1;

        assert_eq!(
            sampled_pick_from_key(&sampled_key, &served, None),
            Some("c.baml"),
            "a gated key samples; bytes 1..5 select the sorted index"
        );
        assert_eq!(
            sampled_pick_from_key(&unsampled_key, &served, None),
            None,
            "a non-gated key must NOT sample at the default 1/32 gate"
        );
        // Pure function of the key: identical inputs → identical pick.
        assert_eq!(
            sampled_pick_from_key(&sampled_key, &served, None),
            sampled_pick_from_key(&sampled_key, &served, None),
            "the sampling decision must be deterministic for a given key"
        );
        // Force overrides the gate both ways (the =1 / =0 knobs).
        assert_eq!(
            sampled_pick_from_key(&unsampled_key, &served, Some(true)),
            Some("a.baml"),
            "force=Some(true) samples even a non-gated key"
        );
        assert_eq!(
            sampled_pick_from_key(&sampled_key, &served, Some(false)),
            None,
            "force=Some(false) disables even a gated key"
        );
        // A different key re-rolls the index → sampling varies across states.
        let mut other = sampled_key;
        other[1] = 3;
        assert_ne!(
            sampled_pick_from_key(&sampled_key, &served, Some(true)),
            sampled_pick_from_key(&other, &served, Some(true)),
            "a different key must be able to select a different file"
        );
        // Nothing served → nothing to sample.
        assert_eq!(sampled_pick_from_key(&sampled_key, &[], Some(true)), None);
    }

    /// Compile+store a project, reopen it byte-identically, and return the
    /// reopened context, its served check rows, and a FRESH un-seeded honest
    /// database — the three inputs `verify_sampled_artifact` compares. `None`
    /// when the on-disk cache is disabled by env.
    fn sampled_setup(
        files: &[(&str, &str)],
    ) -> Option<(
        PathBuf,
        CacheContext,
        ServedChecks,
        ProjectDatabase,
        SourceRoot,
    )> {
        if cache_disabled() {
            return None;
        }
        let root = bc_root();
        let _ = compile_and_store_v1(&root, files);

        let r = resolved(&root, files);
        let (db2, pkg2) = crate::project_load::build_db_from_sources(&r, |_| {});
        let ctx2 = CacheContext::open(&r).expect("cache reopens");
        let served = ctx2
            .served_checks(&db2, pkg2)
            .expect("an identical project serves its rows");
        // The oracle DB must be fresh and un-seeded (no `install_seeds`), else
        // the honest re-derivation would return the served seed verbatim.
        let (honest, honest_pkg) = crate::project_load::build_db_from_sources(&r, |_| {});
        Some((root, ctx2, served, honest, honest_pkg))
    }

    #[test]
    fn verify_sampled_artifact_passes_on_faithful_cache() {
        let files = [("a.baml", A_V1), ("b.baml", B)];
        let Some((root, ctx, served, honest, honest_pkg)) = sampled_setup(&files) else {
            return;
        };
        // A byte-identical reopen serves every file faithfully — both the
        // diagnostics blob and the throws fragment must verify clean.
        for rel in ["a.baml", "b.baml"] {
            ctx.verify_sampled_artifact(&honest, honest_pkg, &served, rel)
                .unwrap_or_else(|e| panic!("faithful cache must pass for {rel}: {e}"));
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn verify_sampled_artifact_bails_on_stale_diagnostics() {
        let files = [("a.baml", A_V1), ("b.baml", B)];
        let Some((root, ctx, mut served, honest, honest_pkg)) = sampled_setup(&files) else {
            return;
        };
        // Replace a.baml's served (empty) diagnostics blob with one fabricating
        // an error the honest check never produces: served != honest → bail.
        served.diagnostics.insert(
            "a.baml".to_string(),
            crate::diagnostics_cache::one_fake_diagnostic_blob("a.baml"),
        );
        let err = ctx
            .verify_sampled_artifact(&honest, honest_pkg, &served, "a.baml")
            .expect_err("a stale served diagnostics blob must hard-error");
        let msg = err.to_string();
        assert!(msg.contains("a.baml"), "must name the file; got: {msg}");
        assert!(
            msg.contains("diagnostics") && msg.contains("BAML_CACHE_VERIFY=1"),
            "must name the artifact kind and point at full verify; got: {msg}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn verify_sampled_artifact_bails_on_stale_fragment() {
        let files = [("a.baml", A_V1), ("b.baml", B)];
        let Some((root, ctx, mut served, honest, honest_pkg)) = sampled_setup(&files) else {
            return;
        };
        // Corrupt a.baml's served callable-throws fragment (the manifest-
        // resident copy the seeds project from); honest bytes differ.
        let fragment = served
            .fragments
            .get_mut("a.baml")
            .expect("a.baml fragment present in the served rows");
        assert!(
            !fragment.is_empty(),
            "a plain function must carry a non-empty fragment for this test to bite"
        );
        *fragment = vec![0xde, 0xad, 0xbe, 0xef];
        let err = ctx
            .verify_sampled_artifact(&honest, honest_pkg, &served, "a.baml")
            .expect_err("a stale served fragment must hard-error");
        let msg = err.to_string();
        assert!(
            msg.contains("a.baml") && msg.contains("callable-throws"),
            "must name the file and artifact kind; got: {msg}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn maybe_sampled_verify_skips_the_hit_path_without_building_a_db() {
        // A whole-image cache hit passes `served = None`: nothing was served
        // row by row, so the honest DB must never be built (sampling a hit
        // would need a full honest compile — the design forbids it).
        let root = bc_root();
        let project = resolved(&root, &[("a.baml", A_V1)]);
        let Some(ctx) = CacheContext::open(&project) else {
            return;
        };
        ctx.maybe_sampled_verify(None, || panic!("hit path must not build an honest DB"))
            .expect("a None plan is a no-op");
        let _ = std::fs::remove_dir_all(root);
    }

    // ── Stdlib (builtin) diagnostics cache ──────────────────────────────────

    /// A synthetic diagnostic anchored in a real builtin file, for the verify
    /// oracle's negative tests (there are usually no real builtin diagnostics).
    fn fabricate_builtin_diag(
        db: &ProjectDatabase,
    ) -> baml_db::baml_compiler_diagnostics::Diagnostic {
        use baml_db::baml_compiler_diagnostics::{Diagnostic, DiagnosticId};
        let builtin = baml_db::baml_compiler2_hir::compiler2_all_files(db)
            .into_iter()
            .find(|sf| sf.path(db).to_string_lossy().starts_with("<builtin>/"))
            .expect("a builtin file exists");
        Diagnostic::error(DiagnosticId::TypeMismatch, "synthetic builtin diag").with_primary_span(
            baml_db::Span {
                file_id: builtin.file_id(db),
                range: text_size::TextRange::new(
                    text_size::TextSize::new(0),
                    text_size::TextSize::new(3),
                ),
            },
        )
    }

    #[test]
    fn stdlib_diagnostics_blob_serves_builtins_leaving_merged_identical() {
        // The headline invariant: with the per-toolchain builtin-diagnostics blob
        // present, `collect_diagnostics_incremental` drops the builtins from
        // `should_check` yet the merged set stays byte-identical to the honest
        // full collector (builtins contribute exactly their cached — here empty —
        // diagnostics, folded into `precomputed`).
        if cache_disabled() || CacheContext::diagnostics_cache_disabled() {
            return;
        }
        let root = bc_root();
        let _ = std::fs::remove_dir_all(&root);
        let r = resolved(&root, &[("a.baml", A_V1)]);
        let (db, ws) = crate::project_load::build_db_from_sources(&r, |_| {});
        let ctx = CacheContext::open(&r).expect("cache opens");

        let honest = baml_db::collect_compiler2_diagnostics(&db);

        // No blob yet: the honest builtin check runs; merged equals honest.
        assert!(
            ctx.load_stdlib_diagnostics().is_none(),
            "no blob on a cold cache"
        );
        let before = ctx.collect_diagnostics_incremental(&db, ws, None).merged;
        assert_eq!(
            before, honest,
            "the no-blob path equals the honest collector"
        );

        // Materialize and serve: builtins are skipped, their cached diagnostics
        // fold into precomputed, and merged stays identical.
        ctx.store_stdlib_diagnostics(&db);
        assert!(
            ctx.load_stdlib_diagnostics().is_some(),
            "the builtin-diagnostics blob is materialized on the miss path"
        );
        let after = ctx.collect_diagnostics_incremental(&db, ws, None).merged;
        assert_eq!(
            after, honest,
            "serving builtins from the cached blob leaves the merged set byte-identical"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compare_stdlib_diagnostics_passes_on_faithful_blob() {
        // The verify oracle's env-independent core: a blob that equals a fresh
        // builtin check is a pass.
        let (db, _) = build_db(&[("a.baml", A_V1)]);
        let blob = crate::diagnostics_cache::serialize_builtin_diagnostics(&db);
        let honest = crate::diagnostics_cache::collect_builtin_diagnostics(&db);
        assert!(
            CacheContext::compare_stdlib_diagnostics(&db, &blob, &honest).is_ok(),
            "a faithful builtin-diagnostics blob passes the verify core"
        );
    }

    #[test]
    fn compare_stdlib_diagnostics_bails_on_dropped_diagnostic() {
        // A cache that dropped a builtin diagnostic the honest check produces:
        // the soundness direction — serving would hide a real diagnostic — bails.
        let (db, _) = build_db(&[("a.baml", A_V1)]);
        let cached = crate::diagnostics_cache::serialize_builtin_diagnostics(&db);
        let mut honest = crate::diagnostics_cache::collect_builtin_diagnostics(&db);
        honest.push(fabricate_builtin_diag(&db));
        let err = CacheContext::compare_stdlib_diagnostics(&db, &cached, &honest)
            .expect_err("a blob missing a diagnostic must bail");
        assert!(
            err.to_string()
                .contains("stdlib (builtin) diagnostics differ"),
            "the bail message must name the builtin divergence: {err}"
        );
    }

    #[test]
    fn compare_stdlib_diagnostics_bails_on_stale_extra() {
        // A cache carrying a stale diagnostic the honest check no longer produces
        // also bails (a non-empty cached blob vs the honest set).
        let (db, _) = build_db(&[("a.baml", A_V1)]);
        let fabricated = fabricate_builtin_diag(&db);
        let stale = crate::diagnostics_cache::serialize_builtin_blob(&db, &[&fabricated]);
        assert_eq!(
            crate::diagnostics_cache::rehydrate_builtin_blob(&db, &stale)
                .expect("stale blob rehydrates")
                .len(),
            1,
            "the fabricated builtin diagnostic serializes into the blob"
        );
        let honest = crate::diagnostics_cache::collect_builtin_diagnostics(&db);
        let err = CacheContext::compare_stdlib_diagnostics(&db, &stale, &honest)
            .expect_err("a stale extra diagnostic must bail");
        assert!(
            err.to_string()
                .contains("stdlib (builtin) diagnostics differ"),
            "the bail message must name the builtin divergence: {err}"
        );
    }

    #[test]
    fn compare_stdlib_diagnostics_passes_on_undecodable_blob() {
        // Degradation: an undecodable blob is NOT a verify violation (the warm
        // path falls back to the honest builtin check), so the core passes.
        let (db, _) = build_db(&[("a.baml", A_V1)]);
        let honest = vec![fabricate_builtin_diag(&db)];
        assert!(
            CacheContext::compare_stdlib_diagnostics(&db, b"not-a-valid-blob", &honest).is_ok(),
            "an undecodable blob degrades to the honest check, not a verify bail"
        );
    }
}
