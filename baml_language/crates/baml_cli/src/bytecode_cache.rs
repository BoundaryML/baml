//! CLI wiring for the content-addressed bytecode cache (`bex_cache`).
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
//!   artifact" hardening). On a warm incremental compile that *serves* cached
//!   artifacts, ~1 run in 32 picks one served clean file and checks its served
//!   diagnostics blob and `callable_throws` fragment against an honest,
//!   un-seeded re-derivation — after the compile result is already produced, so
//!   the added latency is one file's honest work. A mismatch is a hard error
//!   (silent staleness must be LOUD). `=0` disables; `=1` forces every warm
//!   compile (tests); default is 1/32. See [`sampled_pick_from_key`] for the
//!   key-derived, RNG-free sampling decision.
//! - `BAML_NO_DIAGNOSTICS_CACHE=1` — check every file instead of serving clean
//!   files from the per-file diagnostics cache (reuse / throws-seed unaffected),
//!   to isolate that feature's win. Also forces the builtin (stdlib) files to be
//!   checked honestly instead of served from the per-toolchain
//!   stdlib-diagnostics blob — one knob governs all diagnostics serving.
//! - `BAML_NO_CALLABLE_THROWS_CACHE=1` — empty the per-function `callable_throws`
//!   seed so every function infers its throws honestly (reuse / diagnostics
//!   serving unaffected), to isolate the Phase 2 fragment seed's win.
//! - `BAML_NO_DISCOVERY_CACHE=1` — never serve `baml test --list` from the
//!   cached discovery output (the flattened test list), forcing the honest
//!   engine-boot + in-VM discovery every time, to A/B the discovery cache's win.
//!   The rest of the cache (bytecode reuse, diagnostics, throws) is unaffected.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use baml_db::{
    SourceFile,
    baml_compiler2_emit::{
        CompileOptions, LoweringError, OptLevel, decompose_units, generate_project_bytecode,
        generate_project_bytecode_with_reuse_artifacts,
        generate_project_bytecode_with_reuse_artifacts_pregated,
        generate_project_bytecode_with_stdlib, generate_stdlib_program, reuse_throws_mismatches,
    },
    baml_compiler2_hir, baml_compiler2_ppir,
};
use baml_project::ProjectDatabase;
use bex_cache::{
    BytecodeCache, CacheKey, KeyInputs, ManifestFile, ProjectManifest, compiler_fingerprint,
    compute_key, content_hash, env_flag, manifest_key, rel_path, stdlib_diagnostics_key,
    stdlib_interface_key, test_discovery_key,
};
use bex_vm_types::{CompilationUnit, Object, Program, relink};

use crate::{
    file_signature::{file_layout_hash, file_signature_hash},
    project_load::ResolvedProject,
};

/// The optimization level every CLI compile uses (the emit default).
const CLI_OPT_LEVEL: OptLevel = OptLevel::Two;

/// Sentinel entry injected into a file's `referenced_names` when its compiled
/// bytecode bakes in a type's *layout* through an operand that carries no
/// recoverable type reference — a positional field offset (`LoadField`), an
/// enum discriminant (`Discriminant`/`JumpTable`), a type-tag/union dispatch,
/// or a virtual-dispatch slot (`VirtualCall`). Paired with a matching sentinel
/// added to `changed_names` whenever any *type* signature changes, it forces
/// such a file to be re-lowered on any type-layout change. This is the
/// conservative fallback for layout dependencies whose receiver type isn't
/// recoverable without full type inference (e.g. `let p = mk(); p.x`, where
/// `p`'s class is inferred and named nowhere in the file). See
/// `bakes_type_layout`.
///
/// The value is deliberately not a valid identifier (it holds `\0`, `:`, `-`),
/// so it can never collide with a real last-segment item name, with a
/// `syntactic_type_names` token, or with anything `defined_names` produces.
const LAYOUT_SENTINEL: &str = "\u{0}::any-type-layout::";

/// Sentinel entry injected into a file's `referenced_names` when the file
/// participates in interface-coherence checking (it declares an `impl`, an
/// out-of-body `implements … for …`, or a class `implements` block). Paired
/// with a matching sentinel added to `changed_names` whenever any dirty, added,
/// or removed file carries such a construct, it re-checks every
/// coherence-participating file together whenever the package's impl set moves.
///
/// This closes the `OverlappingImplements` (E0132) hole: a conflicting `impl`
/// added in one file surfaces its other half in a *different* file whose name
/// set never changed, so the name-based dirty propagation alone would leave that
/// half stale. Coherence-free projects never store the sentinel and pay nothing.
/// Same non-identifier shape as [`LAYOUT_SENTINEL`], so it can never collide.
const IMPL_SENTINEL: &str = "\u{0}::any-impl-coherence::";

/// An opened cache plus the keys for one resolved project + compile config.
pub(crate) struct CacheContext {
    cache: BytecodeCache,
    /// Whole-project Program, keyed by sources + options + compiler build.
    key: CacheKey,
    /// Precompiled stdlib slice, keyed by compiler build + opt level only.
    stdlib_key: CacheKey,
    /// Cached stdlib typed-interface blob (B-694), keyed by compiler build +
    /// opt level only — like `stdlib_key`, the stdlib is a build constant.
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
    /// Whether test cases are emitted — needed to decompose a full compile back
    /// into independently persisted units.
    emit_test_cases: bool,
}

impl CacheContext {
    /// `None` when caching is disabled via `BAML_NO_BYTECODE_CACHE=1`.
    pub(crate) fn open(resolved: &ResolvedProject, emit_test_cases: bool) -> Option<Self> {
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
            emit_test_cases,
            manifest: resolved.manifest.as_deref(),
            files: &files,
        });

        Some(CacheContext {
            cache: BytecodeCache::open(dir).with_remote_from_env(),
            key,
            stdlib_key: bex_cache::stdlib_key(&fingerprint, CLI_OPT_LEVEL as u8),
            stdlib_interface_key: stdlib_interface_key(&fingerprint, CLI_OPT_LEVEL as u8),
            stdlib_diagnostics_key: stdlib_diagnostics_key(&fingerprint, CLI_OPT_LEVEL as u8),
            manifest_key: manifest_key(
                &fingerprint,
                CLI_OPT_LEVEL as u8,
                emit_test_cases,
                &resolved.root,
                resolved.manifest.as_deref(),
            ),
            test_discovery_key: test_discovery_key(key.as_bytes()),
            emit_test_cases,
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
    /// (leaving the bytecode slice / per-file reuse intact) so a with/without
    /// timing comparison isolates B-694. `BAML_NO_BYTECODE_CACHE` already
    /// disables the whole cache, so this is the finer-grained knob.
    fn stdlib_interface_cache_disabled() -> bool {
        env_flag("BAML_NO_STDLIB_INTERFACE_CACHE")
    }

    /// Isolation toggle for measuring the per-file diagnostics cache's win:
    /// `BAML_NO_DIAGNOSTICS_CACHE=1` drops the serve plan so every file is
    /// re-checked (reuse / throws-seed still active), leaving `plan_reuse`
    /// otherwise intact. `BAML_NO_BYTECODE_CACHE` already disables the whole
    /// cache, so this is the finer-grained knob.
    fn diagnostics_cache_disabled() -> bool {
        env_flag("BAML_NO_DIAGNOSTICS_CACHE")
    }

    /// Isolation toggle for measuring the `callable_throws` seed's win:
    /// `BAML_NO_CALLABLE_THROWS_CACHE=1` empties `plan.seeded_callable_throws`
    /// so every function infers its throws honestly (the last cold
    /// `infer_scope_types` pull a dirty file forces on its clean callees),
    /// leaving reuse / diagnostics serving intact. `BAML_NO_BYTECODE_CACHE`
    /// already disables the whole cache, so this is the finer-grained knob.
    fn callable_throws_cache_disabled() -> bool {
        env_flag("BAML_NO_CALLABLE_THROWS_CACHE")
    }

    /// Load the cached stdlib typed-interface blob (B-694), if present:
    /// `load_raw` + borsh-decode into `package-name -> borsh(PackageInterface)`.
    /// `None` on a miss, a decode failure, or when the interface cache is
    /// disabled — every case falls through to honest stdlib derivation.
    pub(crate) fn load_stdlib_interface(
        &self,
    ) -> Option<std::collections::BTreeMap<String, Vec<u8>>> {
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
        let blob = extract_stdlib_interface(db);
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
        let derived = extract_stdlib_interface(db);
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
    fn load_raw_stdlib_interface_for_verify(
        &self,
    ) -> Option<std::collections::BTreeMap<String, Vec<u8>>> {
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

/// One legacy (`function + test` block) test as it appears in `baml test --list`
/// output — the render inputs, decoupled from the Rust-side `LegacyTest` so the
/// cache payload does not depend on `test_command.rs`'s private type.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct CachedLegacyTest {
    pub(crate) function_name: String,
    pub(crate) test_name: String,
    /// Public root-qualified test id. Adding this field intentionally makes
    /// older discovery blobs fail Borsh decoding and fall back to discovery.
    pub(crate) canonical_id: String,
    /// Project-root-relative display path (the `--list` `(path)` suffix).
    pub(crate) file_path: String,
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
    /// Legacy function-attached tests, unfiltered, in discovery order.
    pub(crate) legacy: Vec<CachedLegacyTest>,
    /// Fully-expanded testset leaf names (canonical `root...::...` ids), unfiltered, in
    /// `collect_leaf_names` order.
    pub(crate) testset_leaf_names: Vec<String>,
}

impl CacheContext {
    /// Isolation / A-B toggle for the `--list` discovery cache:
    /// `BAML_NO_DISCOVERY_CACHE=1` disables *only* serving/writing the flattened
    /// test list, so `--list` always boots the engine and discovers honestly,
    /// leaving bytecode reuse / diagnostics / throws seeding intact.
    /// `BAML_NO_BYTECODE_CACHE` already disables the whole cache, so this is the
    /// finer-grained knob.
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
        if cached.legacy != honest.legacy {
            anyhow::bail!(
                "BAML_CACHE_VERIFY: cached `test --list` legacy-test discovery differs from a \
                 fresh discovery ({} vs {} tests). Test discovery is nondeterministic or reads \
                 uncached state — please report this.",
                cached.legacy.len(),
                honest.legacy.len(),
            );
        }
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

/// Derive and borsh-serialize each stdlib package's `PackageInterface` from a
/// compiled database, keyed by package name. On a warm database the query
/// returns the seed verbatim, so re-serializing reproduces the same bytes
/// (idempotent); on a cold database it materializes the interface once.
fn extract_stdlib_interface(db: &ProjectDatabase) -> std::collections::BTreeMap<String, Vec<u8>> {
    use baml_db::{
        Name, baml_compiler2_hir::package::PackageId,
        baml_compiler2_hir_ty::package_interface::package_interface,
    };
    let mut out = std::collections::BTreeMap::new();
    for name in baml_builtins2::stdlib_package_names().iter().copied() {
        let pkg_id = PackageId::new(db, Name::new(name));
        let iface = package_interface(db, pkg_id);
        match borsh::to_vec(iface) {
            Ok(bytes) => {
                out.insert(name.to_string(), bytes);
            }
            Err(e) => cache_debug(format_args!("stdlib interface serialize `{name}`: {e}")),
        }
    }
    out
}

/// Compile the project, reusing (or materializing) the precompiled stdlib
/// slice when a cache is available.
///
/// The stdlib slice depends only on the compiler build + opt level — the Go
/// model: compiled once per toolchain, ever, then spliced into every compile.
/// Splice output is byte-identical to a full compile (enforced by the
/// `emit_determinism` suite), so callers and the project-blob cache see no
/// difference beyond speed. Any stdlib-entry problem falls back to compiling
/// it fresh; a failed write just means rebuilding it next run.
pub(crate) fn compile_program(
    db: &ProjectDatabase,
    options: &CompileOptions,
    cache: Option<&CacheContext>,
    plan: Option<&ReusePlan>,
) -> Result<Program, LoweringError> {
    compile_program_artifacts(db, options, cache, plan).map(|artifacts| artifacts.program)
}

pub(crate) struct CompiledArtifacts {
    pub(crate) program: Program,
    pub(crate) units: Option<Vec<CompilationUnit>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CacheStoreStats {
    pub(crate) unit_entries_written: usize,
    pub(crate) manifest_entries_written: usize,
}

pub(crate) fn compile_program_artifacts(
    db: &ProjectDatabase,
    options: &CompileOptions,
    cache: Option<&CacheContext>,
    plan: Option<&ReusePlan>,
) -> Result<CompiledArtifacts, LoweringError> {
    let Some(ctx) = cache else {
        return generate_project_bytecode(db, options).map(|program| CompiledArtifacts {
            program,
            units: None,
        });
    };
    let base = match ctx.cache.load_shared(&ctx.stdlib_key) {
        Some(base) => base,
        None => {
            let base = generate_stdlib_program(db, CLI_OPT_LEVEL)?;
            let _ = ctx.cache.store_shared(&ctx.stdlib_key, &base);
            base
        }
    };
    if let Some(plan) = plan {
        // `prepare_reuse_plan` already ran the serve-time throws gate against
        // these exact seeds; when it demoted nothing, skip emit's identical
        // re-run of the gate (see `throws_gate_verified`).
        let reuse_result = if plan.throws_gate_verified {
            generate_project_bytecode_with_reuse_artifacts_pregated(
                db,
                options,
                CLI_OPT_LEVEL,
                &base,
                &plan.prev_units,
                &plan.clean_files,
            )
        } else {
            generate_project_bytecode_with_reuse_artifacts(
                db,
                options,
                CLI_OPT_LEVEL,
                &base,
                &plan.prev_units,
                &plan.clean_files,
            )
        };
        match reuse_result {
            Ok((program, units)) => {
                return Ok(CompiledArtifacts {
                    program,
                    units: Some(units),
                });
            }
            // A real compile error must surface — it is not a reuse problem.
            Err(err @ LoweringError::ProjectHasErrors { .. }) => return Err(err),
            // A corrupt/incompatible previous unit or an unrelocatable
            // construct: fall back to the full (stdlib-spliced) compile, which
            // is byte-identical. Never an error the user should see.
            Err(other) => {
                cache_debug(format_args!("units relink fell back: {other}"));
            }
        }
    }
    generate_project_bytecode_with_stdlib(db, options, CLI_OPT_LEVEL, &base).map(|program| {
        CompiledArtifacts {
            program,
            units: None,
        }
    })
}

/// The per-file reuse decision for one compile: which files' compiled
/// bytecode can be spliced from the previous program, and which must be
/// re-typechecked and re-lowered.
pub(crate) struct ReusePlan {
    /// Root-relative paths eligible for bytecode splice.
    pub(crate) clean_files: HashSet<String>,
    /// Files that must be re-typechecked (everything not clean). This is
    /// also the diagnostics-gate set: a clean file references nothing whose
    /// signature changed, so its diagnostics are identical to the previous
    /// (error-free, or it wouldn't have been cached) compile.
    pub(crate) dirty_files: Vec<SourceFile>,
    /// The previous compile's symbolic units (B-693 Stage 3): clean files' units
    /// are reused verbatim, the rest re-emitted, then linked.
    pub(crate) prev_units: Vec<CompilationUnit>,
    /// Content-addressed unit keys for the clean files in `prev_units`. The
    /// store path copies these pointers into the next manifest without
    /// serializing or probing unchanged units again.
    pub(crate) unit_keys: HashMap<String, [u8; 32]>,
    /// Per-file throw facts (full path → facts), to be seeded into the database
    /// before compiling. Clean files' facts come from the manifest (their bodies
    /// are never re-walked); dirty/added files' facts are the ones the dirty-set
    /// pass already walked, folded in so the downstream demand hits the seed
    /// instead of re-walking those bodies a second time.
    pub(crate) seeded_throw_facts:
        std::collections::BTreeMap<String, Vec<baml_type::throw_facts::FunctionThrowFacts>>,
    /// Per-function `callable_throws` seeds projected from the clean files'
    /// cached interface fragments (Phase 2), keyed by full source path then by
    /// item-tree `LocalItemId::as_u32`. Injected before the first typecheck so a
    /// clean function's throws are served without inferring its (or any
    /// transitively-clean callee's) body. Empty under
    /// `BAML_NO_CALLABLE_THROWS_CACHE=1`.
    pub(crate) seeded_callable_throws:
        std::collections::BTreeMap<String, std::collections::BTreeMap<u32, baml_type::Ty>>,
    /// Clean files' opaque diagnostics blobs carried from the previous manifest,
    /// by rel_path. Rehydrated to serve those files' diagnostics without
    /// re-checking, and copied verbatim into the next manifest.
    pub(crate) clean_diagnostics: std::collections::BTreeMap<String, Vec<u8>>,
    /// Clean files' `CallableThrowsFragment` blobs carried verbatim from the
    /// previous manifest, by rel_path. The source of the `callable_throws`
    /// seeds (so seeding needs no unit payloads) and of the next manifest's
    /// fragment carry.
    pub(crate) clean_fragments: std::collections::BTreeMap<String, Vec<u8>>,
    /// True when the dirty partition found nothing changed: no dirty or added
    /// files, and every manifest entry still present on disk. On this path the
    /// serve-time throws gate is provably a tautology (the seeds being checked
    /// are byte-for-byte the values the manifest was stored with, units are
    /// content-addressed and written before the manifest, and the solve is a
    /// deterministic pure function), so it is skipped — and unit payloads are
    /// not loaded at all (nothing will be re-emitted or relinked from them).
    pub(crate) no_delta: bool,
    /// True once [`prepare_reuse_plan`] has run the serve-time throws gate
    /// (or proven it a tautology via `no_delta`) with zero demotions — i.e.
    /// `clean_files` already reflects the gate's verdict under the seeds now
    /// installed in the database. Emit may then skip its own
    /// `reuse_throws_mismatches` re-run (the second of two identical gates
    /// per warm compile). Left `false` when the gate demoted anything: that
    /// path clears the inference seeds, so emit's re-check runs against
    /// honestly-derived inference and can demote further.
    pub(crate) throws_gate_verified: bool,
    /// Files whose stored throws fingerprint no longer matches a
    /// recomputation from current inputs (see `throws_fingerprints`).
    /// Restricted to files the partition called clean — dirty files are
    /// re-derived regardless. In shadow mode (default) this is
    /// observability only; under `BAML_THROWS_FINGERPRINTS=enforce` it
    /// replaces the inference-priced serve-time gate as the demotion
    /// authority.
    pub(crate) fingerprint_invalid: std::collections::BTreeSet<String>,
    /// How many clean files the fingerprint validator checked (0 when the
    /// validator was skipped: env-off, or a manifest predating the field).
    pub(crate) fingerprint_checked: usize,
}

/// The result of the warm-database preamble ([`CacheContext::prepare_warm_db`]).
pub(crate) struct WarmPrep {
    /// The prepared per-file reuse plan (`None` when nothing can be reused).
    pub(crate) reuse_plan: Option<ReusePlan>,
    /// Whether the stdlib interface seed was served — run/test skip re-writing
    /// it in that case; `check` ignores this.
    pub(crate) stdlib_interface_hit: bool,
}

/// Install a reuse plan's type-inference seeds, then enforce its throws
/// invariant before callers may serve cached diagnostics. Files that fail the
/// invariant are demoted to dirty; the remaining clean units can still be
/// reused. Any mismatch clears inference seeds so the diagnostic pass derives
/// them honestly.
pub(crate) fn prepare_reuse_plan(
    db: &mut ProjectDatabase,
    plan: Option<ReusePlan>,
) -> Option<ReusePlan> {
    let mut plan = plan?;
    db.set_seeded_throw_facts(std::mem::take(&mut plan.seeded_throw_facts));
    let callable_seeds = std::mem::take(&mut plan.seeded_callable_throws);
    // No delta ⇒ the gate is a tautology: the seeds just installed are
    // byte-for-byte the values the manifest was stored with (the store path
    // persists `file_throw_facts` verbatim and units are content-addressed,
    // written before the manifest that points at them), and the solve +
    // runtime conversion the gate would re-run are deterministic pure
    // functions of those inputs. Comparing a value against itself cannot
    // demote anything, so skip the compare — it was the dominant cost of a
    // warm no-op invocation (it derives `package_items`, i.e. re-parses the
    // project). Any real edit, added or removed file takes the gate below.
    if plan.no_delta {
        db.set_seeded_callable_throws(callable_seeds);
        plan.throws_gate_verified = true;
        return Some(plan);
    }
    // Fingerprints in ENFORCE mode replace the inference-priced gate: the
    // validator already proved (by input-identity, not trust) that every
    // remaining clean file's stored throws re-derive from unchanged inputs.
    // Invalid files take the same fail-safe demotion the gate used.
    if matches!(
        crate::throws_fingerprints::mode(),
        crate::throws_fingerprints::FpMode::Enforce
    ) && plan.fingerprint_checked > 0
    {
        if plan.fingerprint_invalid.is_empty() {
            db.set_seeded_callable_throws(callable_seeds);
            plan.throws_gate_verified = true;
            cache_debug(format_args!(
                "throws fingerprints clean — inference gate skipped (enforce)"
            ));
            return Some(plan);
        }
        db.set_seeded_throw_facts(std::collections::BTreeMap::new());
        db.set_seeded_callable_throws(std::collections::BTreeMap::new());
        let root = db.get_project().map(|project| project.root(db).clone());
        let invalid = std::mem::take(&mut plan.fingerprint_invalid);
        for rel in invalid {
            cache_debug(format_args!("reuse demoted `{rel}`: throws fingerprint"));
            if !plan.clean_files.remove(&rel) {
                continue;
            }
            plan.unit_keys.remove(&rel);
            plan.clean_diagnostics.remove(&rel);
            let full = root
                .as_ref()
                .map_or_else(|| PathBuf::from(&rel), |root| root.join(&rel));
            if let Some(file) = db.get_file(&full)
                && !plan.dirty_files.contains(&file)
            {
                plan.dirty_files.push(file);
            }
        }
        if plan.clean_files.is_empty() {
            return None;
        }
        return Some(plan);
    }

    // The gate's runtime-type conversion derives the package alias tables,
    // which fold every file's semantic index. Prime the per-file indexes
    // across workers first so that derivation is a cheap fold instead of a
    // serial parse of the whole project under one salsa memo claim.
    baml_project::prime_file_indexes_parallel(db);
    let pre_gate_inferences = baml_db::baml_compiler2_hir_ty::infer::body_inferences();
    let mismatches = reuse_throws_mismatches(db, &plan.prev_units, &plan.clean_files);
    cache_debug(format_args!(
        "throws gate: {} body inferences",
        baml_db::baml_compiler2_hir_ty::infer::body_inferences() - pre_gate_inferences
    ));

    // Shadow comparison: the gate is the deciding oracle; log where the
    // fingerprint validator disagrees. A file the gate demotes but the
    // validator called valid is UNDER-INVALIDATION — the failure class that
    // blocks enforce mode — and is logged loudly per file. The reverse
    // (validator invalidates, gate passes) is mere conservatism.
    if plan.fingerprint_checked > 0 {
        let gate_demoted: std::collections::BTreeSet<&str> =
            mismatches.iter().map(|(rel, _)| rel.as_str()).collect();
        let under: Vec<&&str> = gate_demoted
            .iter()
            .filter(|rel| !plan.fingerprint_invalid.contains(**rel))
            .collect();
        let over = plan
            .fingerprint_invalid
            .iter()
            .filter(|rel| !gate_demoted.contains(rel.as_str()))
            .count();
        for rel in &under {
            cache_debug(format_args!(
                "THROWS-FP UNDER-INVALIDATION: gate demoted `{rel}` but fingerprint validated it"
            ));
        }
        cache_debug(format_args!(
            "throws fingerprints vs gate: fp_invalid={} gate_demoted={} under={} over={}",
            plan.fingerprint_invalid.len(),
            gate_demoted.len(),
            under.len(),
            over
        ));
    }

    if mismatches.is_empty() {
        db.set_seeded_callable_throws(callable_seeds);
        plan.throws_gate_verified = true;
        return Some(plan);
    }

    // A throws mismatch is a fail-safe path. Keep unaffected bytecode units,
    // but make every inference query honest for this invocation.
    db.set_seeded_throw_facts(std::collections::BTreeMap::new());
    db.set_seeded_callable_throws(std::collections::BTreeMap::new());
    let root = db.get_project().map(|project| project.root(db).clone());
    for (rel, detail) in mismatches {
        cache_debug(format_args!("reuse demoted `{rel}`: {detail}"));
        if !plan.clean_files.remove(&rel) {
            continue;
        }
        plan.unit_keys.remove(&rel);
        plan.clean_diagnostics.remove(&rel);
        let full = root
            .as_ref()
            .map_or_else(|| PathBuf::from(&rel), |root| root.join(&rel));
        if let Some(file) = db.get_file(&full)
            && !plan.dirty_files.contains(&file)
        {
            plan.dirty_files.push(file);
        }
    }
    if plan.clean_files.is_empty() {
        return None;
    }
    Some(plan)
}

/// Recompute every current file's throws fingerprint and compare the clean
/// files' against their stored values (see `throws_fingerprints`).
///
/// Inputs mirror the store side exactly: clean files read from the manifest
/// (their content is unchanged, so stored facts/hashes ARE current), dirty
/// and added files from the database (`file_throw_facts` is salsa-memoized
/// and was already demanded by the dirty partition). Removed files simply
/// drop out of the input set, which shifts every dependent's fingerprint —
/// no special case needed.
///
/// Returns `None` when disabled (`BAML_THROWS_FINGERPRINTS=off`), else
/// `(invalid clean rels, checked count)`.
fn validate_throws_fingerprints(
    db: &ProjectDatabase,
    manifest: &ProjectManifest,
    current_files: &HashMap<String, SourceFile>,
    clean_files: &HashSet<String>,
) -> Option<(std::collections::BTreeSet<String>, usize)> {
    if matches!(
        crate::throws_fingerprints::mode(),
        crate::throws_fingerprints::FpMode::Off
    ) {
        return None;
    }
    let entries: HashMap<&str, &ManifestFile> = manifest
        .files
        .iter()
        .map(|f| (f.rel_path.as_str(), f))
        .collect();
    type OwnedInput = (
        String,
        Vec<baml_type::throw_facts::FunctionThrowFacts>,
        [u8; 32],
        bool,
        [u8; 32],
    );
    let owned: Vec<OwnedInput> = current_files
        .iter()
        .map(|(rel, sf)| {
            if clean_files.contains(rel)
                && let Some(entry) = entries.get(rel.as_str())
            {
                (
                    rel.clone(),
                    entry.throw_facts.clone(),
                    entry.layout_hash,
                    entry.referenced_names.iter().any(|n| n == IMPL_SENTINEL),
                    entry.content_hash,
                )
            } else {
                (
                    rel.clone(),
                    baml_db::baml_compiler2_hir_ty::throw_facts::file_throw_facts(db, *sf)
                        .0
                        .clone(),
                    crate::file_signature::file_layout_hash(db, *sf),
                    file_has_impl_construct(db, *sf),
                    content_hash(sf.text(db)),
                )
            }
        })
        .collect();
    let inputs: Vec<crate::throws_fingerprints::FileFpInput<'_>> = owned
        .iter()
        .map(
            |(rel, facts, layout, has_impl, content)| crate::throws_fingerprints::FileFpInput {
                rel,
                facts,
                layout_hash: *layout,
                has_impl_construct: *has_impl,
                content_hash: *content,
            },
        )
        .collect();
    let (fps, stats) = crate::throws_fingerprints::compute_file_fingerprints(&inputs);
    cache_debug(format_args!(
        "throws fp graph: {} nodes, {} firewalled, {} env-dependent, {} unresolved edge names",
        stats.nodes, stats.firewalled, stats.env_dependent, stats.unresolved_edges
    ));
    let mut invalid = std::collections::BTreeSet::new();
    let mut checked = 0usize;
    for entry in &manifest.files {
        if !clean_files.contains(&entry.rel_path) {
            continue;
        }
        let Some(current_fp) = fps.get(&entry.rel_path) else {
            continue; // clean but no longer on disk: partition handles it
        };
        checked += 1;
        if *current_fp != entry.throws_fp {
            invalid.insert(entry.rel_path.clone());
        }
    }
    Some((invalid, checked))
}

/// Cache diagnostics to stderr, gated on `BAML_CACHE_DEBUG=1`. For support
/// and perf triage: shows plan sizes, fallback reasons, and store failures
/// without affecting normal output.
#[allow(clippy::print_stderr)] // opt-in debug channel (BAML_CACHE_DEBUG=1)
pub(crate) fn cache_debug(args: std::fmt::Arguments<'_>) {
    if env_flag("BAML_CACHE_DEBUG") {
        eprintln!("[baml-cache] {args}");
    }
}

/// Last path segment of a dotted fq name (`user.ns.foo` → `foo`).
fn last_segment(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

/// Last-segment names of every item `file` defines, from the HIR item tree.
fn defined_names(db: &ProjectDatabase, file: SourceFile) -> Vec<String> {
    use baml_compiler2_hir::contributions::Definition;
    use baml_compiler2_ppir::item_data::{
        file_classes, file_enums, file_functions, file_interfaces, file_lets, file_type_aliases,
    };
    use baml_db::baml_compiler2_mir::def_to_item_ref;

    let mut names: Vec<String> = Vec::new();
    let push = |def, names: &mut Vec<String>| {
        names.push(last_segment(&def_to_item_ref(db, def).to_string()).to_string());
    };
    for &loc in file_functions(db, file) {
        push(Definition::Function(loc), &mut names);
    }
    for &loc in file_lets(db, file) {
        push(Definition::Let(loc), &mut names);
    }
    for &loc in file_classes(db, file) {
        push(Definition::Class(loc), &mut names);
    }
    for &loc in file_enums(db, file) {
        push(Definition::Enum(loc), &mut names);
    }
    for &loc in file_interfaces(db, file) {
        push(Definition::Interface(loc), &mut names);
    }
    // Type aliases are erased into their consumers (a non-recursive alias is
    // expanded inline at every use), so an alias whose RHS changes must reach
    // the change-propagation set by *name*: a consumer that named the alias
    // would otherwise splice the stale expansion. `def_to_item_ref` handles
    // `TypeAlias` like any other named item.
    for &loc in file_type_aliases(db, file) {
        push(Definition::TypeAlias(loc), &mut names);
    }
    names.sort_unstable();
    names.dedup();
    names
}

/// Add every named type in a single type annotation to `out`, keyed by the
/// name's last path segment (matching `defined_names`/`changed_names`).
///
/// The HIR stores annotations as `baml_compiler2_ast::TypeExpr`, whose crate is
/// not a direct dependency of `baml_cli`, so the enum can't be matched
/// structurally here. Names are recovered from the annotation's canonical
/// `Display` instead — see `syntactic_type_names` for why this over-approximate
/// tokenization is sound.
fn add_type_display<T: std::fmt::Display>(te: &T, out: &mut HashSet<String>) {
    for token in te
        .to_string()
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
    {
        if token.is_empty()
            || token.starts_with(|c: char| c.is_ascii_digit())
            || is_builtin_type_word(token)
        {
            continue;
        }
        out.insert(token.to_string());
    }
}

/// Primitive / structural keywords that `TypeExpr`'s `Display` emits (`int`,
/// `map`, `throws`, …). None can name a user type, so dropping them just trims
/// noise; keeping any would be harmless, since a token only ever *adds* a file
/// to the dirty set (over-dirtying costs reuse, never correctness).
fn is_builtin_type_word(word: &str) -> bool {
    matches!(
        word,
        "int"
            | "bigint"
            | "float"
            | "string"
            | "bool"
            | "null"
            | "never"
            | "void"
            | "uint8array"
            | "unknown"
            | "type"
            | "error"
            | "map"
            | "throws"
    )
}

/// Last-segment type names *syntactically named* in `file`'s signatures and
/// type-level annotations: function/method params, returns, `throws`, and
/// generic bounds; class & interface field types; `implements`/`requires`/`for`
/// targets; associated-type bounds/defaults; and — crucially — type-alias
/// right-hand sides.
///
/// This complements `referenced_names_by_file`, which is derived from compiled
/// bytecode operands and therefore only captures types that surface as an
/// `Object` operand (a class constructed, an enum variant allocated). A file
/// that touches a type purely through its *layout* — positional field access
/// (`p.x` → `LoadField`), an enum variant match (`Discriminant`/`JumpTable`), a
/// virtual call, or an inline-expanded type alias — leaves no such operand, so
/// those references never enter the bytecode set. Extracting the names written
/// in the file's type annotations recovers every dependency that appears in a
/// signature (the common case, and every empirically-reproduced miss:
/// `diff(p: Point)`, `pay(m: Money)`).
///
/// Extraction is a deliberate over-approximation: tokenizing each annotation's
/// `Display` also yields primitive keywords, generic-parameter names, and
/// dotted-path prefixes. All are harmless — a spurious name only ever adds a
/// file to the dirty set, never removes one — while every real type name is
/// always present as an identifier run. The floor this guarantees: any file
/// naming a changed type in a signature is dirtied.
/// The firewall-`TypeRef` twin of [`add_type_display`]: render one type
/// reference from an item's `type_refs` arena and tokenize its name into `out`.
fn add_type_ref_display(
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: baml_compiler2_hir::type_ref::TypeRefId,
    out: &mut HashSet<String>,
) {
    add_type_display(&store.display(id), out);
}

fn syntactic_type_names(db: &ProjectDatabase, file: SourceFile) -> HashSet<String> {
    use baml_compiler2_ppir::item_data::{
        ImplSubjectData, class_data, file_classes, file_functions, file_impls, file_interfaces,
        file_template_strings, file_type_aliases, function_data, impl_block_data, interface_data,
        template_string_data, type_alias_data,
    };
    let mut names: HashSet<String> = HashSet::new();

    for &loc in file_functions(db, file) {
        let func = function_data(db, loc);
        for id in func.params.iter().filter_map(|p| p.type_ref) {
            add_type_ref_display(&func.type_refs, id, &mut names);
        }
        if let Some(id) = func.return_type {
            add_type_ref_display(&func.type_refs, id, &mut names);
        }
        if let Some(id) = func.throws {
            add_type_ref_display(&func.type_refs, id, &mut names);
        }
        for id in func.generic_params.iter().flat_map(|p| &p.bounds) {
            add_type_ref_display(&func.type_refs, *id, &mut names);
        }
    }
    for &loc in file_template_strings(db, file) {
        let ts = template_string_data(db, loc);
        for id in ts.params.iter().filter_map(|p| p.type_ref) {
            add_type_ref_display(&ts.type_refs, id, &mut names);
        }
    }
    for &loc in file_classes(db, file) {
        let class = class_data(db, loc);
        for id in class.fields.iter().map(|f| f.type_ref) {
            add_type_ref_display(&class.type_refs, id, &mut names);
        }
        for id in class.generic_params.iter().flat_map(|p| &p.bounds) {
            add_type_ref_display(&class.type_refs, *id, &mut names);
        }
        for block in &class.implements {
            add_type_ref_display(&class.type_refs, block.target, &mut names);
        }
    }
    for &loc in file_interfaces(db, file) {
        let iface = interface_data(db, loc);
        for id in iface.fields.iter().map(|f| f.type_ref) {
            add_type_ref_display(&iface.type_refs, id, &mut names);
        }
        for id in &iface.requires {
            add_type_ref_display(&iface.type_refs, *id, &mut names);
        }
        for id in iface.generic_params.iter().flat_map(|p| &p.bounds) {
            add_type_ref_display(&iface.type_refs, *id, &mut names);
        }
        for method in &iface.required_methods {
            for id in method.params.iter().filter_map(|p| p.type_ref) {
                add_type_ref_display(&iface.type_refs, id, &mut names);
            }
            if let Some(id) = method.return_type {
                add_type_ref_display(&iface.type_refs, id, &mut names);
            }
            if let Some(id) = method.throws {
                add_type_ref_display(&iface.type_refs, id, &mut names);
            }
            for id in method.generic_params.iter().flat_map(|p| &p.bounds) {
                add_type_ref_display(&iface.type_refs, *id, &mut names);
            }
        }
        for assoc in &iface.associated_types {
            if let Some(id) = assoc.bound {
                add_type_ref_display(&iface.type_refs, id, &mut names);
            }
            if let Some(id) = assoc.default {
                add_type_ref_display(&iface.type_refs, id, &mut names);
            }
        }
    }
    for &loc in file_type_aliases(db, file) {
        let alias = type_alias_data(db, loc);
        if let Some(id) = alias.value {
            add_type_ref_display(&alias.type_refs, id, &mut names);
        }
    }
    for &loc in file_impls(db, file) {
        let block = impl_block_data(db, loc);
        add_type_ref_display(&block.type_refs, block.interface_target, &mut names);
        // Out-of-body (`Free`) impls carry an explicit for-target; an in-class
        // impl's for-target is the class itself (no header type to display).
        // `names` is a set, so the interface_target added above is not
        // double-counted for free impls.
        if let ImplSubjectData::Free { for_target, .. } = &block.subject {
            add_type_ref_display(&block.type_refs, *for_target, &mut names);
        }
    }
    names
}

/// Whether `file` declares any type whose *layout* another file's bytecode may
/// bake in: a class (field order), an enum (discriminants), an interface
/// (dispatch), or a type alias (inline expansion). Used for the added-file case
/// of the `LAYOUT_SENTINEL`, where there is no prior `layout_hash` to diff
/// against; a *modified* file instead compares its [`file_layout_hash`] so a
/// function-only edit in a type-defining file no longer trips the sentinel.
fn file_defines_type(db: &ProjectDatabase, file: SourceFile) -> bool {
    use baml_compiler2_ppir::item_data::{
        file_classes, file_enums, file_interfaces, file_type_aliases,
    };
    !file_classes(db, file).is_empty()
        || !file_enums(db, file).is_empty()
        || !file_interfaces(db, file).is_empty()
        || !file_type_aliases(db, file).is_empty()
}

/// Whether `file` declares any interface-`impl` construct — an `impl` block, an
/// out-of-body `implements … for …`, or a class `implements` block. Gates the
/// `IMPL_SENTINEL`: only a change to such a file can move the package's impl set
/// (and thus a coherence verdict), so an impl-free edit never trips the fallback.
fn file_has_impl_construct(db: &ProjectDatabase, file: SourceFile) -> bool {
    use baml_compiler2_ppir::item_data::{class_data, file_classes, file_impls};
    // `file_impls` holds both in-class and out-of-body impl blocks; a class
    // `implements` block is a distinct construct, so it needs its own check.
    !file_impls(db, file).is_empty()
        || file_classes(db, file)
            .iter()
            .any(|&loc| !class_data(db, loc).implements.is_empty())
}

/// Last-segment names referenced by each user file's compiled bytecode,
/// grouped by root-relative path.
///
/// Extracted from the Program (not source), so desugared references — a
/// `for` loop's `next()`, injected guards — are all included. Object refs
/// resolve through the pool (classes/enums/interfaces by name; function
/// objects are a file's own lambdas — internal, skipped); global slots
/// resolve through the inverted name maps.
///
/// A file whose bytecode bakes a type's *layout* through a non-`Object`
/// operand (field offsets, enum discriminants, virtual-dispatch slots) also
/// gets the `LAYOUT_SENTINEL` — see `bakes_type_layout`. The bytecode set is
/// unioned with `syntactic_type_names` in `store_with_manifest`, so both
/// desugared-reference coverage and source-level type dependencies are kept.
fn referenced_names_by_file(program: &Program) -> HashMap<String, Vec<String>> {
    let mut slot_names: HashMap<usize, &str> = HashMap::new();
    for (name, &slot) in &program.function_global_indices {
        slot_names.insert(slot, name);
    }
    for (name, &slot) in &program.let_global_indices {
        slot_names.insert(slot, name);
    }

    let mut by_file: HashMap<&str, HashSet<String>> = HashMap::new();
    for obj in program.objects.iter() {
        let Object::Function(function) = obj else {
            continue;
        };
        if function.source_file.is_empty() || function.source_file.starts_with("<builtin>/") {
            continue;
        }
        let names = by_file.entry(function.source_file.as_str()).or_default();
        let bakes_type_layout =
            relink::visit_index_operands_ref(function, |operand| match operand {
                relink::IndexOperandRef::Global(slot) => {
                    if let Some(name) = slot_names.get(&slot.raw()) {
                        names.insert(last_segment(name).to_string());
                    }
                }
                relink::IndexOperandRef::Object(obj_idx) => {
                    let referenced = match program.objects.get(obj_idx.raw()) {
                        Some(Object::Class(class)) => Some(class.name.to_string()),
                        Some(Object::Enum(enum_def)) => Some(enum_def.name.to_string()),
                        Some(Object::Interface(iface)) => Some(iface.name.to_string()),
                        _ => None,
                    };
                    if let Some(name) = referenced {
                        names.insert(last_segment(&name).to_string());
                    }
                }
            });
        // Field offsets / discriminants / vtable slots bake a type's layout but
        // name no `Object`; tag the file so any type-layout change re-lowers it.
        if bakes_type_layout {
            names.insert(LAYOUT_SENTINEL.to_string());
        }
    }
    by_file
        .into_iter()
        .map(|(file, names)| {
            let mut names: Vec<String> = names.into_iter().collect();
            names.sort_unstable();
            (file.to_string(), names)
        })
        .collect()
}

/// Root-relative display path for a user `SourceFile`, or `None` for the legacy
/// `<builtin>/` entries the v1 compiler still surfaces.
fn user_rel_path(db: &ProjectDatabase, root: &std::path::Path, sf: SourceFile) -> Option<String> {
    let path = sf.path(db);
    if path.to_string_lossy().starts_with("<builtin>/") {
        return None;
    }
    Some(rel_path(root, &path))
}

/// User source files with their root-relative paths (skipping legacy
/// `<builtin>/` entries the v1 compiler still sees).
fn user_files_with_rel_paths(db: &ProjectDatabase) -> Vec<(SourceFile, String)> {
    let Some(project) = db.get_project() else {
        return Vec::new();
    };
    let root = project.root(db);
    db.get_source_files()
        .into_iter()
        .filter_map(|sf| user_rel_path(db, &root, sf).map(|rel| (sf, rel)))
        .collect()
}

/// The clean/dirty partition of the current user files against a previous
/// manifest. Shared by [`CacheContext::plan_reuse`] (to build the reuse plan)
/// and [`CacheContext::verify_callable_throws_fragments`] (to check exactly the files
/// that would have been seeded).
struct DirtyPartition {
    /// Root-relative paths eligible for reuse (clean).
    clean_files: HashSet<String>,
    /// Current `SourceFile`s that must be re-derived (dirty).
    dirty_files: Vec<SourceFile>,
    /// Freshly-walked throw facts for the dirty/added files, keyed by absolute
    /// path. Seeded so the downstream `file_throw_facts` demand hits the seed
    /// instead of re-walking each body a second time (the partition already
    /// walked them, and the facts are content-derived so the value is honest).
    fresh_throw_facts:
        std::collections::BTreeMap<String, Vec<baml_type::throw_facts::FunctionThrowFacts>>,
    /// Every current user file with its rel path — the single walk this pass
    /// makes over the source set, handed back so [`CacheContext::plan_reuse`]
    /// need not list them again on the warm hot path.
    current: Vec<(SourceFile, String)>,
}

fn throw_fn_names(
    facts: &[baml_type::throw_facts::FunctionThrowFacts],
) -> impl Iterator<Item = String> + '_ {
    facts
        .iter()
        .map(|fact| last_segment(fact.key.as_str()).to_string())
}

/// Partition the current user files into clean (reusable) and dirty against a
/// previous manifest.
///
/// Dirty = content-changed ∪ added ∪ (files whose referenced names intersect the
/// signature/layout/impl change set, one hop) ∪ (files reachable from a
/// throws-changed function through the transitive throws-taint closure).
///
/// The throws-taint closure covers throws changes that flow transitively.
/// `callable_throws` is transitive over the call graph, so a throws change can
/// flow through a *content-clean intermediary* — whose own `file_throw_facts`
/// are byte-identical, only its solved transitive throws grew — to a caller a
/// one-hop pass would never reach. The closure seeds a worklist with the
/// last-segment names of every function whose facts changed (or was
/// added/removed), then walks reverse call edges — over-approximated by
/// `referenced_names` — marking each referencing file dirty and re-tainting its
/// own functions, stopping at closed-`throws` contracts (whose `callable_throws`
/// is the declared set, independent of callees). It over-dirties only, so a
/// seeded function's body and transitive throw contributors are always stable —
/// the invariant the `callable_throws` seed rests on.
fn compute_dirty_partition(db: &ProjectDatabase, manifest: &ProjectManifest) -> DirtyPartition {
    let prev_files: HashMap<&str, &ManifestFile> = manifest
        .files
        .iter()
        .map(|f| (f.rel_path.as_str(), f))
        .collect();

    let current = user_files_with_rel_paths(db);
    let current_rels: HashSet<&str> = current.iter().map(|(_, rel)| rel.as_str()).collect();

    // Δ: names whose *signature/layout/impl* meaning may have changed (one-hop).
    let mut changed_names: HashSet<String> = HashSet::new();
    // Files needing recompilation for their own sake.
    let mut dirty: HashSet<String> = HashSet::new();
    // A type's layout (field offset, enum discriminant, type tag, vtable slot)
    // moved somewhere, so every file that bakes any layout must re-lower. Raised
    // by a modified file whose `layout_hash` changed, or any added/removed
    // type-defining file — never by a plain function-signature edit.
    let mut layout_changed = false;
    let mut impl_set_changed = false;
    // Root-cause function names whose `callable_throws` may have changed — the
    // seed of the transitive throws-taint closure. Last-segment form, matched
    // against `referenced_names` (the reverse-call-edge over-approximation).
    let mut throws_taint: HashSet<String> = HashSet::new();
    // Freshly-extracted throw facts for the dirty/added files this pass already
    // walked. Seeded (beside the clean files' manifest facts) so the downstream
    // `file_throw_facts` demand hits the seed instead of re-walking each body —
    // the facts are content-derived, so this fresh value is exactly the honest
    // one the recompute would produce. Keyed by absolute path, like the seed.
    let mut fresh_throw_facts: std::collections::BTreeMap<
        String,
        Vec<baml_type::throw_facts::FunctionThrowFacts>,
    > = std::collections::BTreeMap::new();

    for (sf, rel) in &current {
        match prev_files.get(rel.as_str()) {
            None => {
                // Added file: recompiles, and its names may shadow existing
                // references anywhere.
                dirty.insert(rel.clone());
                changed_names.extend(defined_names(db, *sf));
                // A newly-introduced type conservatively counts as a layout
                // change: the file is new, so there is no prior `layout_hash` to
                // diff against.
                if file_defines_type(db, *sf) {
                    layout_changed = true;
                }
                if file_has_impl_construct(db, *sf) {
                    impl_set_changed = true;
                }
                // An added function may shadow an existing callee, so callers'
                // transitive throws can move — seed the taint closure.
                let fresh = baml_db::baml_compiler2_hir_ty::throw_facts::file_throw_facts(db, *sf);
                throws_taint.extend(throw_fn_names(&fresh.0));
                fresh_throw_facts.insert(sf.path(db).display().to_string(), fresh.0.clone());
            }
            Some(entry) => {
                if entry.content_hash == content_hash(sf.text(db)) {
                    continue;
                }
                dirty.insert(rel.clone());
                // Throws taint: a body-only edit that grows this file's inferred
                // throws leaves its `signature_hash` unchanged, so no signature
                // sentinel catches it. When the fresh facts differ from the
                // stored ones, seed the taint closure with both the fresh and
                // the stored function names — a rename/removal shifts which
                // callers resolve where.
                let fresh = baml_db::baml_compiler2_hir_ty::throw_facts::file_throw_facts(db, *sf);
                if fresh.0 != entry.throw_facts {
                    throws_taint.extend(throw_fn_names(&fresh.0));
                    throws_taint.extend(throw_fn_names(&entry.throw_facts));
                }
                fresh_throw_facts.insert(sf.path(db).display().to_string(), fresh.0.clone());
                if file_has_impl_construct(db, *sf)
                    || entry
                        .referenced_names
                        .iter()
                        .any(|name| name == IMPL_SENTINEL)
                {
                    impl_set_changed = true;
                }
                if entry.signature_hash != file_signature_hash(db, *sf) {
                    changed_names.extend(entry.defined_names.iter().cloned());
                    changed_names.extend(defined_names(db, *sf));
                }
                // Fire the layout sentinel only when this file's *layout*
                // surface actually moved — a field/variant/alias reorder, a new
                // generic parameter — not merely because a type-defining file's
                // signature changed. A function-only signature edit in a
                // class-defining file leaves `layout_hash` fixed, so it no longer
                // drags every layout-baking file into the dirty set.
                if entry.layout_hash != file_layout_hash(db, *sf) {
                    layout_changed = true;
                }
            }
        }
    }
    for (rel, entry) in &prev_files {
        if !current_rels.contains(rel) {
            // Removed file: its names vanish from resolution. A removal can
            // erase a type another file inferred-baked, so it conservatively
            // counts as a layout change (there is no current `layout_hash` to
            // diff, and removals are rare on the hot edit path).
            changed_names.extend(entry.defined_names.iter().cloned());
            layout_changed = true;
            if entry.referenced_names.iter().any(|n| n == IMPL_SENTINEL) {
                impl_set_changed = true;
            }
            // A removed function's callers now resolve elsewhere or error — a
            // throws change; seed the taint closure with its function names.
            throws_taint.extend(throw_fn_names(&entry.throw_facts));
        }
    }
    if layout_changed {
        changed_names.insert(LAYOUT_SENTINEL.to_string());
    }
    if impl_set_changed {
        changed_names.insert(IMPL_SENTINEL.to_string());
    }

    // Signature-meaning cascade + one-hop propagation, run jointly to a
    // fixpoint. A file whose *full* `referenced_names` intersect the change set
    // is dirty — a layout or name dependency surfaces one hop away. A file whose
    // *signature* surface names a changed type has, in addition, its own resolved
    // signature meaning changed, so its defined names join the change set
    // (cascading to its callers) and its functions seed the throws closure —
    // `callable_throws` can move when a signature type is re-targeted.
    //
    // A purely body-level reference to a changed name is dirtied but does NOT
    // cascade: a body reference can only move the file's *throws* (the transitive
    // closure below covers that), never its signature, so growing the change set
    // from it would needlessly dirty the whole call graph. This is what closes
    // the alias-re-target hole — a caller that names neither the re-targeted alias
    // nor the changed type, only a content-unchanged callee whose signature now
    // resolves differently — while a body-only caller stops the cascade.
    let mut cascaded: HashSet<String> = HashSet::new();
    loop {
        let mut grew = false;
        for (_, rel) in &current {
            let Some(entry) = prev_files.get(rel.as_str()) else {
                continue; // added file: already dirty, its names already in Δ
            };
            let sig_hit = entry
                .sig_referenced_names
                .iter()
                .any(|name| changed_names.contains(name));
            if sig_hit
                || entry
                    .referenced_names
                    .iter()
                    .any(|name| changed_names.contains(name))
            {
                dirty.insert(rel.clone());
            }
            if sig_hit && cascaded.insert(rel.clone()) {
                for name in &entry.defined_names {
                    if changed_names.insert(name.clone()) {
                        grew = true;
                    }
                }
                throws_taint.extend(throw_fn_names(&entry.throw_facts));
            }
        }
        if !grew {
            break;
        }
    }

    // Transitive throws-taint closure, firewall-pruned. Reverse index: a
    // last-segment name -> the current files whose stored bytecode references it.
    let mut referencers: HashMap<&str, Vec<&str>> = HashMap::new();
    for (rel, entry) in &prev_files {
        if !current_rels.contains(rel) {
            continue;
        }
        for name in &entry.referenced_names {
            referencers.entry(name.as_str()).or_default().push(rel);
        }
    }
    let mut worklist: Vec<String> = throws_taint.iter().cloned().collect();
    // Re-taint each newly-reached file's functions exactly once.
    let mut retainted: HashSet<String> = HashSet::new();
    while let Some(name) = worklist.pop() {
        let Some(rels) = referencers.get(name.as_str()) else {
            continue;
        };
        for &rel in rels {
            // A file referencing a tainted name is dirty (its open functions'
            // throws may have moved); mark it and re-taint its own non-firewall
            // functions so the change propagates transitively.
            dirty.insert(rel.to_string());
            if !retainted.insert(rel.to_string()) {
                continue;
            }
            let Some(entry) = prev_files.get(rel) else {
                continue;
            };
            for ff in &entry.throw_facts {
                // Closed `throws` contract: `callable_throws` is the declared
                // set, independent of callees, so the taint stops here.
                if ff.has_declared_contract {
                    continue;
                }
                let n = last_segment(ff.key.as_str()).to_string();
                if throws_taint.insert(n.clone()) {
                    worklist.push(n);
                }
            }
        }
    }

    let clean_files: HashSet<String> = current
        .iter()
        .filter(|(_, rel)| !dirty.contains(rel))
        .map(|(_, rel)| rel.clone())
        .collect();
    let dirty_files: Vec<SourceFile> = current
        .iter()
        .filter(|(_, rel)| dirty.contains(rel))
        .map(|(sf, _)| *sf)
        .collect();

    DirtyPartition {
        clean_files,
        dirty_files,
        fresh_throw_facts,
        current,
    }
}

/// Project each clean file's cached interface fragment into a per-function
/// `callable_throws` seed map, keyed by full source path then by item-tree
/// `LocalItemId::as_u32` (the fragment's key form). A unit whose fragment is
/// empty or fails to decode is skipped — its functions then infer honestly
/// (degrade, never miscompile). Empty under `BAML_NO_CALLABLE_THROWS_CACHE=1`.
fn project_callable_throws_seeds(
    clean_fragments: &std::collections::BTreeMap<String, Vec<u8>>,
    root: Option<&PathBuf>,
) -> std::collections::BTreeMap<String, std::collections::BTreeMap<u32, baml_type::Ty>> {
    if CacheContext::callable_throws_cache_disabled() {
        return std::collections::BTreeMap::new();
    }
    use baml_db::baml_compiler2_hir_ty::package_interface::CallableThrowsFragment;
    let mut by_path = std::collections::BTreeMap::new();
    for (rel, fragment_bytes) in clean_fragments {
        if fragment_bytes.is_empty() {
            continue;
        }
        let fragment: CallableThrowsFragment = match borsh::from_slice(fragment_bytes) {
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
        let full = root
            .map(|r| r.join(rel).display().to_string())
            .unwrap_or_else(|| rel.clone());
        by_path.insert(full, fragment.by_id);
    }
    by_path
}

impl CacheContext {
    /// Decide which files can reuse their previously compiled bytecode.
    ///
    /// `None` means no reuse is possible (first compile, compiler changed,
    /// previous units trimmed, or everything is dirty) — callers take the
    /// stdlib-splice path and gate diagnostics on all files.
    ///
    /// The clean/dirty partition (with the throws-taint closure) is computed by
    /// [`compute_dirty_partition`]; this method loads each candidate clean
    /// file's content-addressed unit, degrades individual misses to dirty, then
    /// attaches the reuse seeds and clean-file diagnostics for what remains.
    pub(crate) fn plan_reuse(&self, db: &ProjectDatabase) -> Option<ReusePlan> {
        // The verify tripwire must exercise the full compile path — a
        // relink-produced blob verified against a relink compile would only
        // prove relink self-consistency (the test suite's oracles cover
        // relink == full; verify covers key completeness in the field).
        if Self::verify_enabled() {
            return None;
        }
        let Some(manifest_bytes) = self.cache.load_raw(&self.manifest_key) else {
            cache_debug(format_args!("no manifest — full compile"));
            return None;
        };
        let Ok(manifest) = borsh::from_slice::<ProjectManifest>(&manifest_bytes) else {
            cache_debug(format_args!("manifest undecodable — full compile"));
            return None;
        };
        let partition = compute_dirty_partition(db, &manifest);
        if partition.clean_files.is_empty() {
            cache_debug(format_args!("all files dirty — full compile"));
            return None;
        }
        let DirtyPartition {
            mut clean_files,
            mut dirty_files,
            fresh_throw_facts,
            current,
        } = partition;

        // No dirty or added files, and no manifest entry missing from disk
        // (a removed file's entry would be neither clean nor current, so the
        // counts diverge): nothing will be re-emitted or relinked, so unit
        // payloads are not needed at all — the seeds and diagnostics blobs
        // are manifest-resident. This skips both the unit read/hash pass and
        // (in `prepare_reuse_plan`) the serve-time throws gate.
        let no_delta = dirty_files.is_empty() && clean_files.len() == manifest.files.len();

        // Reuse the single file walk `compute_dirty_partition` already made rather
        // than re-listing the source set on the warm hot path.
        let current_files: HashMap<String, SourceFile> =
            current.into_iter().map(|(file, rel)| (rel, file)).collect();
        let mut prev_units = Vec::with_capacity(clean_files.len());
        let mut unit_keys = HashMap::with_capacity(clean_files.len());
        if no_delta {
            for entry in &manifest.files {
                unit_keys.insert(entry.rel_path.clone(), entry.unit_key);
            }
        } else {
            // Load clean units across worker threads (read + digest + borsh
            // decode per unit, sizable at large projects). `par_iter`'s
            // indexed collect preserves manifest order exactly, so
            // `prev_units` is byte-for-byte the sequence the serial loop
            // produced — emit determinism is unaffected.
            use rayon::prelude::*;
            let clean_entries: Vec<_> = manifest
                .files
                .iter()
                .filter(|entry| clean_files.contains(&entry.rel_path))
                .collect();
            let loaded: Vec<(&str, [u8; 32], Option<CompilationUnit>)> = clean_entries
                .par_iter()
                .map(|entry| {
                    let key = CacheKey::from_bytes(entry.unit_key);
                    let unit = self.cache.load_unit_shared(&key).filter(|unit| {
                        unit.source_file == entry.rel_path
                            && !std::path::Path::new(&unit.source_file).is_absolute()
                    });
                    (entry.rel_path.as_str(), entry.unit_key, unit)
                })
                .collect();
            let mut degraded = Vec::new();
            for (rel, entry_key, unit) in loaded {
                match unit {
                    Some(unit) => {
                        unit_keys.insert(rel.to_string(), entry_key);
                        prev_units.push(unit);
                    }
                    None => degraded.push(rel.to_string()),
                }
            }
            for rel in degraded {
                cache_debug(format_args!(
                    "unit `{rel}` missing or invalid — degraded to dirty"
                ));
                clean_files.remove(&rel);
                unit_keys.remove(&rel);
                if let Some(file) = current_files.get(&rel)
                    && !dirty_files.contains(file)
                {
                    dirty_files.push(*file);
                }
            }
            if clean_files.is_empty() {
                cache_debug(format_args!("all reusable units missing — full compile"));
                return None;
            }
        }
        cache_debug(format_args!(
            "reuse plan: {} clean, {} dirty{}",
            clean_files.len(),
            dirty_files.len(),
            if no_delta { " (no delta)" } else { "" }
        ));

        // Throws-fingerprint validation (shadow by default; see
        // `throws_fingerprints`). Runs before seeding so its inputs are the
        // same honest sources the store side used.
        let (fingerprint_invalid, fingerprint_checked) = match validate_throws_fingerprints(
            db,
            &manifest,
            &current_files,
            &clean_files,
        ) {
            Some((invalid, checked)) => {
                cache_debug(format_args!(
                    "throws fingerprints: {} checked, {} invalid",
                    checked,
                    invalid.len()
                ));
                if no_delta && !invalid.is_empty() {
                    // Identical inputs must reproduce identical
                    // fingerprints; anything else is a determinism bug in
                    // the fingerprint computation itself.
                    cache_debug(format_args!(
                        "THROWS-FP SELF-CHECK FAILURE: {} files invalid on a no-delta plan: {:?}",
                        invalid.len(),
                        invalid
                    ));
                }
                (invalid, checked)
            }
            None => (Default::default(), 0),
        };

        // Seed throw facts for every file. Clean files' facts come from the
        // manifest (their content is unchanged, and nothing they reference
        // changed signature). Dirty/added files' facts are the ones the partition
        // already walked (`fresh_throw_facts`) — folding them in lets the
        // downstream `file_throw_facts` demand hit the seed rather than re-walking
        // each body a second time (the seed-after-query invalidation the taint
        // closure would otherwise cause). Every value is the honest one.
        let root = db.get_project().map(|p| p.root(db));
        let mut seeded_throw_facts: std::collections::BTreeMap<
            String,
            Vec<baml_type::throw_facts::FunctionThrowFacts>,
        > = manifest
            .files
            .iter()
            .filter(|entry| clean_files.contains(&entry.rel_path))
            .map(|entry| {
                let full = root
                    .as_ref()
                    .map(|r| r.join(&entry.rel_path).display().to_string())
                    .unwrap_or_else(|| entry.rel_path.clone());
                (full, entry.throw_facts.clone())
            })
            .collect();
        seeded_throw_facts.extend(fresh_throw_facts);

        // Carry clean files' interface-fragment blobs verbatim (rel-path-keyed):
        // the `callable_throws` seeds project from these, and the store path
        // copies them into the next manifest unchanged. Manifest-resident so
        // seeding never has to read unit payloads.
        let clean_fragments: std::collections::BTreeMap<String, Vec<u8>> = manifest
            .files
            .iter()
            .filter(|entry| clean_files.contains(&entry.rel_path))
            .map(|entry| {
                (
                    entry.rel_path.clone(),
                    entry.callable_throws_fragment.clone(),
                )
            })
            .collect();

        // Project clean files' cached interface fragments into a per-function
        // `callable_throws` seed (Phase 2): a clean function's throws — and hence
        // any dirty caller's throws-dependent inference over it — are served
        // without walking its body. The throws-taint closure guarantees a seeded
        // function's transitive throw contributors are all unchanged.
        let seeded_callable_throws = project_callable_throws_seeds(&clean_fragments, root.as_ref());

        // Carry clean files' cached diagnostics blobs verbatim (already
        // rel-path-keyed): the gate rehydrates them to serve those files without
        // re-checking, and they are copied into the next manifest unchanged.
        let clean_diagnostics = manifest
            .files
            .iter()
            .filter(|entry| clean_files.contains(&entry.rel_path))
            .map(|entry| (entry.rel_path.clone(), entry.diagnostics.clone()))
            .collect();

        Some(ReusePlan {
            clean_files,
            dirty_files,
            prev_units,
            unit_keys,
            seeded_throw_facts,
            seeded_callable_throws,
            clean_diagnostics,
            clean_fragments,
            no_delta,
            throws_gate_verified: false,
            fingerprint_invalid,
            fingerprint_checked,
        })
    }

    /// Write the Program blob plus the manifest describing it. Called after
    /// every successful compile; best-effort like all cache writes.
    ///
    /// `fresh_by_file` holds the diagnostics blob for every file the gate freshly
    /// checked (dirty or degraded), by rel_path. Per file the manifest takes its
    /// fresh blob if present, else the plan's carried clean blob, else an empty
    /// blob — so a re-checked file always overwrites a stale/poison carry.
    pub(crate) fn store_with_manifest(
        &self,
        db: &ProjectDatabase,
        program: &Program,
        fresh_by_file: &std::collections::BTreeMap<String, Vec<u8>>,
        plan: Option<&ReusePlan>,
    ) -> std::io::Result<CacheStoreStats> {
        self.store_artifacts_with_manifest(db, program, None, fresh_by_file, plan)
    }

    pub(crate) fn store_artifacts_with_manifest(
        &self,
        db: &ProjectDatabase,
        program: &Program,
        units: Option<&[CompilationUnit]>,
        fresh_by_file: &std::collections::BTreeMap<String, Vec<u8>>,
        plan: Option<&ReusePlan>,
    ) -> std::io::Result<CacheStoreStats> {
        self.store(program)?;

        let reused_units = units.is_some();
        let owned_units;
        let units = match units {
            Some(units) => units,
            None => {
                let options = CompileOptions {
                    emit_test_cases: self.emit_test_cases,
                };
                owned_units = decompose_units(db, &options, program).map_err(|error| {
                    std::io::Error::other(format!("unit decomposition failed: {error}"))
                })?;
                &owned_units
            }
        };

        let mut units_by_source = HashMap::with_capacity(units.len());
        for unit in units {
            if units_by_source
                .insert(unit.source_file.as_str(), unit)
                .is_some()
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("duplicate compilation unit for `{}`", unit.source_file),
                ));
            }
        }

        let user_files = user_files_with_rel_paths(db);
        // Only a successful reuse compile returns assembled `units`. A full
        // fallback must persist freshly decomposed units for every file rather
        // than carrying pointers from the abandoned reuse plan.
        let pointer_plan = if reused_units { plan } else { None };
        let mut unit_keys = HashMap::with_capacity(user_files.len());
        let mut unit_entries_written = 0usize;
        for (_, rel) in &user_files {
            if let Some(key) = pointer_plan
                .filter(|plan| plan.clean_files.contains(rel))
                .and_then(|plan| plan.unit_keys.get(rel))
            {
                unit_keys.insert(rel.clone(), *key);
                continue;
            }
            let Some(unit) = units_by_source.get(rel.as_str()) else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("compiled output has no unit for `{rel}`"),
                ));
            };
            if std::path::Path::new(&unit.source_file).is_absolute() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unit source path is not root-relative: `{rel}`"),
                ));
            }
            let (key, wrote) = self.cache.store_unit_shared(unit)?;
            unit_entries_written += usize::from(wrote);
            unit_keys.insert(rel.clone(), *key.as_bytes());
        }
        cache_debug(format_args!(
            "unit store: wrote {unit_entries_written}, reused {}",
            user_files.len().saturating_sub(unit_entries_written)
        ));

        // Throws fingerprints for every file, from the same db state the
        // entries below read (facts are memoized/seeded, hashes recomputed).
        // Always computed at store time regardless of mode: the field must be
        // populated for a later validate-side run to compare against.
        let throws_fps: std::collections::BTreeMap<String, [u8; 32]> = {
            type OwnedInput = (
                String,
                Vec<baml_type::throw_facts::FunctionThrowFacts>,
                [u8; 32],
                bool,
                [u8; 32],
            );
            let owned: Vec<OwnedInput> = user_files
                .iter()
                .map(|(sf, rel)| {
                    (
                        rel.clone(),
                        baml_db::baml_compiler2_hir_ty::throw_facts::file_throw_facts(db, *sf)
                            .0
                            .clone(),
                        file_layout_hash(db, *sf),
                        file_has_impl_construct(db, *sf),
                        content_hash(sf.text(db)),
                    )
                })
                .collect();
            let inputs: Vec<crate::throws_fingerprints::FileFpInput<'_>> = owned
                .iter()
                .map(|(rel, facts, layout, has_impl, content)| {
                    crate::throws_fingerprints::FileFpInput {
                        rel,
                        facts,
                        layout_hash: *layout,
                        has_impl_construct: *has_impl,
                        content_hash: *content,
                    }
                })
                .collect();
            crate::throws_fingerprints::compute_file_fingerprints(&inputs).0
        };

        let mut referenced = referenced_names_by_file(program);
        let mut files: Vec<ManifestFile> = user_files
            .into_iter()
            .map(|(sf, rel)| {
                // Union the bytecode-derived references (desugared calls,
                // constructed classes, plus the layout sentinel) with the
                // types named in this file's source-level annotations, so
                // layout/alias dependencies invisible to the bytecode are
                // tracked too.
                let mut set: HashSet<String> = referenced
                    .remove(&rel)
                    .unwrap_or_default()
                    .into_iter()
                    .collect();
                // The signature-surface names, kept both as their own manifest
                // field (the cascade's meaning-propagation input) and folded into
                // the full referenced set (the one-hop layout/name dependency).
                let sig_names = syntactic_type_names(db, sf);
                let mut sig_referenced_names: Vec<String> = sig_names.iter().cloned().collect();
                sig_referenced_names.sort_unstable();
                set.extend(sig_names);
                // Tag coherence-participating files so any package-wide impl-set
                // change re-checks every one of them together (IMPL_SENTINEL).
                if file_has_impl_construct(db, sf) {
                    set.insert(IMPL_SENTINEL.to_string());
                }
                let mut referenced_names: Vec<String> = set.into_iter().collect();
                referenced_names.sort_unstable();
                ManifestFile {
                    content_hash: content_hash(sf.text(db)),
                    signature_hash: file_signature_hash(db, sf),
                    layout_hash: file_layout_hash(db, sf),
                    defined_names: defined_names(db, sf),
                    referenced_names,
                    sig_referenced_names,
                    // Free: seeded files return their seeds verbatim, dirty
                    // files were extracted (and memoized) during the compile.
                    throw_facts: baml_db::baml_compiler2_hir_ty::throw_facts::file_throw_facts(
                        db, sf,
                    )
                    .0
                    .clone(),
                    // Fresh blob if the gate re-checked this file, else the
                    // carried clean blob, else empty. A re-checked file always
                    // wins so a stale/poison carry can't persist.
                    diagnostics: fresh_by_file
                        .get(&rel)
                        .cloned()
                        .or_else(|| plan.and_then(|p| p.clean_diagnostics.get(&rel).cloned()))
                        .unwrap_or_else(crate::diagnostics_cache::empty_blob),
                    // Verbatim copy of the unit's fragment bytes when this
                    // compile produced/assembled the unit, else the plan's
                    // carried manifest copy — the two are byte-identical by
                    // construction, so the manifest copy can seed without
                    // reading unit payloads.
                    callable_throws_fragment: units_by_source
                        .get(rel.as_str())
                        .map(|unit| unit.callable_throws_fragment.clone())
                        .or_else(|| plan.and_then(|p| p.clean_fragments.get(&rel).cloned()))
                        .unwrap_or_default(),
                    unit_key: unit_keys[&rel],
                    throws_fp: throws_fps.get(&rel).copied().unwrap_or([0u8; 32]),
                    rel_path: rel,
                }
            })
            .collect();
        files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

        let manifest = ProjectManifest {
            program_key: *self.key.as_bytes(),
            files,
        };
        let payload = borsh::to_vec(&manifest)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.cache.store_raw(&self.manifest_key, &payload)?;
        Ok(CacheStoreStats {
            unit_entries_written,
            manifest_entries_written: 1,
        })
    }

    /// Seed the immutable stdlib typed interface (gated off under verify so the
    /// oracle exercises the honest path) and prepare the per-file reuse plan —
    /// the identical warm-database setup `run`, `test`, and `check` each run
    /// before the diagnostics gate. `check` discards `stdlib_interface_hit`.
    pub(crate) fn prepare_warm_db(&self, db: &mut ProjectDatabase) -> WarmPrep {
        let stdlib_interface_hit = self.seed_stdlib_interface(db);
        let reuse_plan = prepare_reuse_plan(db, self.plan_reuse(db));
        WarmPrep {
            reuse_plan,
            stdlib_interface_hit,
        }
    }

    /// The warm-path verify-and-store sequence shared by `run` and `test`: run
    /// every `BAML_CACHE_VERIFY` oracle, persist the Program + per-file units and
    /// manifest, materialize the stdlib interface (unless it was already served)
    /// and the per-toolchain builtin-diagnostics blob, then run the sampled
    /// field-verify. `honest_db` builds a fresh un-seeded database for that
    /// sampled oracle.
    pub(crate) fn verify_and_store(
        &self,
        db: &ProjectDatabase,
        compiled: &CompiledArtifacts,
        fresh: &std::collections::BTreeMap<String, Vec<u8>>,
        plan: Option<&ReusePlan>,
        stdlib_interface_hit: bool,
        honest_db: impl FnOnce() -> ProjectDatabase,
    ) -> anyhow::Result<()> {
        self.verify_against(&compiled.program)?;
        self.verify_stdlib_interface(db)?;
        self.verify_diagnostics(db)?;
        self.verify_stdlib_diagnostics(db)?;
        self.verify_callable_throws_fragments(db)?;
        if let Err(e) = self.store_artifacts_with_manifest(
            db,
            &compiled.program,
            compiled.units.as_deref(),
            fresh,
            plan,
        ) {
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
        // result exists, ~1 warm run in 32 re-derives one served clean file on a
        // fresh, un-seeded database and hard-errors on any drift. Bounded latency
        // (one file's honest work), loud on the silent-staleness bug class the
        // full `BAML_CACHE_VERIFY` guards.
        self.maybe_sampled_verify(plan, honest_db)?;
        Ok(())
    }

    /// Read-only diagnostics collector for `baml check`. It uses the same
    /// prepared reuse plan as run/test, but does not serialize fresh blobs: a
    /// check-only manifest cannot advance hashes without matching new units.
    pub(crate) fn collect_diagnostics_for_check(
        &self,
        db: &ProjectDatabase,
        plan: Option<&ReusePlan>,
    ) -> Vec<baml_db::baml_compiler_diagnostics::Diagnostic> {
        let plan = plan
            .filter(|_| !Self::diagnostics_cache_disabled())
            .map(|plan| DiagnosticsServePlan {
                clean_files: &plan.clean_files,
                clean_diagnostics: &plan.clean_diagnostics,
            });
        self.collect_diagnostics_with_plan(db, plan, false).merged
    }

    /// Gate diagnostics on the warm path: run `check_file` only for the reuse
    /// plan's dirty (plus degraded and builtin) files, serving clean files from
    /// their cached blobs, and return the merged set plus the fresh per-file
    /// blobs to persist. With `plan == None` (first compile / verify / all
    /// dirty) this reduces to the honest full check, so the merged set always
    /// equals the honest collector and the caller renders errors identically.
    pub(crate) fn collect_diagnostics_incremental(
        &self,
        db: &ProjectDatabase,
        plan: Option<&ReusePlan>,
    ) -> IncrementalDiagnostics {
        // Isolation toggle (mirrors `BAML_NO_STDLIB_INTERFACE_CACHE`): dropping
        // the serve plan checks every file, so a with/without scope-inference
        // comparison measures exactly the clean files this feature skips.
        let plan = plan
            .filter(|_| !Self::diagnostics_cache_disabled())
            .map(|plan| DiagnosticsServePlan {
                clean_files: &plan.clean_files,
                clean_diagnostics: &plan.clean_diagnostics,
            });
        self.collect_diagnostics_with_plan(db, plan, true)
    }

    fn collect_diagnostics_with_plan(
        &self,
        db: &ProjectDatabase,
        plan: Option<DiagnosticsServePlan<'_>>,
        persist_fresh: bool,
    ) -> IncrementalDiagnostics {
        let Some(root) = db.get_project().map(|p| p.root(db).clone()) else {
            // No project context: fall back to the honest full check with no
            // cacheable output (there are no user files to key by).
            return IncrementalDiagnostics {
                merged: baml_project::collect_compiler2_diagnostics(db),
                fresh_by_file: std::collections::BTreeMap::new(),
            };
        };

        // Rehydrate clean files' cached diagnostics; a file that fails to
        // rehydrate degrades to a re-check (its blob is never served stale).
        let mut precomputed: Vec<baml_db::baml_compiler_diagnostics::Diagnostic> = Vec::new();
        let mut degrade: HashSet<String> = HashSet::new();
        if let Some(plan) = &plan {
            for (rel, blob) in plan.clean_diagnostics {
                match crate::diagnostics_cache::rehydrate_file_blob(db, &root, blob) {
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
        // like a clean user file's served blob. This removes the ~1,900-scope
        // stdlib re-inference tail from every warm dirty compile. It is
        // independent of the per-file reuse `plan` (even a first-ever compile of
        // a project can serve a blob written by any earlier compile on the same
        // toolchain). A missing / corrupt blob, `BAML_NO_DIAGNOSTICS_CACHE`, and
        // `BAML_CACHE_VERIFY` all fall through to the honest builtin check
        // (`load_stdlib_diagnostics` gates the disable knob; verify is gated
        // here so its oracle exercises the honest path).
        let serve_builtins = !Self::verify_enabled()
            && self
                .load_stdlib_diagnostics()
                .and_then(|blob| crate::diagnostics_cache::rehydrate_builtin_blob(db, &blob))
                .map(|mut diags| precomputed.append(&mut diags))
                .is_some();

        let rel_of = |sf: SourceFile| user_rel_path(db, &root, sf);
        let should_check = |sf: SourceFile| -> bool {
            match rel_of(sf) {
                // Builtin: served from the per-toolchain constant blob when
                // present (its diagnostics are already folded into `precomputed`);
                // otherwise checked honestly, and its blob stored afterward.
                None => !serve_builtins,
                Some(rel) => match &plan {
                    Some(plan) => !plan.clean_files.contains(&rel) || degrade.contains(&rel),
                    None => true,
                },
            }
        };
        // Serving observability: which serve layers engaged, and how many user
        // files each covers. A dirty warm check that re-infers most of the
        // project shows up here as blobs≪clean or degraded≫0.
        cache_debug(format_args!(
            "diagnostics serve: plan={} clean={} blobs={} degraded={} serve_builtins={}",
            plan.is_some(),
            plan.as_ref().map_or(0, |p| p.clean_files.len()),
            plan.as_ref().map_or(0, |p| p.clean_diagnostics.len()),
            degrade.len(),
            serve_builtins,
        ));

        let narrowed =
            baml_project::collect_compiler2_diagnostics_narrowed(db, &should_check, precomputed);

        let mut fresh_by_file = if persist_fresh {
            crate::diagnostics_cache::fresh_blobs_by_file(db, &root, &narrowed.fresh)
        } else {
            std::collections::BTreeMap::new()
        };
        // Ensure every re-checked user file has an entry (empty if it produced
        // no diagnostics) so `store_with_manifest` overwrites a stale/poison
        // carry for a degraded-but-now-clean file rather than re-carrying it.
        if persist_fresh {
            for (sf, rel) in user_files_with_rel_paths(db) {
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
    /// under `BAML_CACHE_VERIFY` the reuse plan is disabled, so the compile runs
    /// the honest full check; this compares every file the reuse plan would have
    /// served from cache (the dirty-partition clean set — not merely the
    /// content-unchanged files) against a fresh `check_file`. A mismatch means the
    /// cached diagnostics are a stale substitute that would change what a warm
    /// incremental run reports — a hard error, and a tighter signal than the
    /// whole-`Program` byte-compare.
    pub(crate) fn verify_diagnostics(&self, db: &ProjectDatabase) -> anyhow::Result<()> {
        if !Self::verify_enabled() {
            return Ok(());
        }
        self.check_cached_diagnostics_against_fresh(db)
    }

    /// The env-independent core of [`Self::verify_diagnostics`], so the oracle's
    /// discriminating power (pass on a faithful cache, bail on a stale one) is
    /// unit-testable without mutating the process environment.
    pub(crate) fn check_cached_diagnostics_against_fresh(
        &self,
        db: &ProjectDatabase,
    ) -> anyhow::Result<()> {
        let Some(manifest) = self.load_prev_manifest_for_verify() else {
            return Ok(());
        };
        let Some(root) = db.get_project().map(|p| p.root(db).clone()) else {
            return Ok(());
        };
        // Exactly the files a served warm compile would serve from cache — the
        // throws-taint-closure / signature-cascade clean set, not merely the
        // content-unchanged files. A content-unchanged file dirtied by a changed
        // referenced name (or a sentinel, or the cascade) is re-checked, never
        // served stale, so comparing its stored blob against a fresh check would
        // bail spuriously on ordinary cross-file edits. Mirrors the sibling
        // fragment oracle's gating.
        let clean_files = compute_dirty_partition(db, &manifest).clean_files;
        for entry in &manifest.files {
            if !clean_files.contains(&entry.rel_path) {
                continue; // dirty — always re-checked, never served from cache
            }
            let full = root.join(&entry.rel_path);
            let Some(sf) = db.get_file(&full) else {
                continue; // file removed — never served
            };
            let Some(served) =
                crate::diagnostics_cache::rehydrate_file_blob(db, &root, &entry.diagnostics)
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

    /// Load the previous manifest for the verify oracle, bypassing the
    /// `plan_reuse` verify short-circuit (verify must still compare against
    /// whatever manifest is on disk).
    fn load_prev_manifest_for_verify(&self) -> Option<ProjectManifest> {
        self.load_decoded(&self.manifest_key, "manifest")
    }

    /// Load the previous compile's symbolic units for the verify oracle,
    /// bypassing the `plan_reuse` verify short-circuit.
    fn load_prev_units_for_verify(&self, manifest: &ProjectManifest) -> Vec<CompilationUnit> {
        manifest
            .files
            .iter()
            .filter_map(|entry| {
                let unit = self
                    .cache
                    .load_unit_shared(&CacheKey::from_bytes(entry.unit_key))?;
                (unit.source_file == entry.rel_path).then_some(unit)
            })
            .collect()
    }

    /// Localized interface-fragment / `callable_throws`-seed verify oracle
    /// (analog of `verify_stdlib_interface` / `verify_diagnostics`): under
    /// `BAML_CACHE_VERIFY` the reuse plan is disabled, so `db` derives every
    /// fragment honestly (no seed served). This compares each *taint-clean*
    /// file's stored fragment against that honest re-derivation.
    ///
    /// The clean set is the exact throws-taint-closure partition a served warm
    /// compile would seed — not merely the content-unchanged files — so the
    /// oracle checks precisely what would have been seeded. A clean file whose
    /// stored fragment differs from honest means the closure failed to dirty a
    /// file whose `callable_throws` actually moved, so the seed would have been
    /// stale. Whole-fragment equality checks the exact seed-faithfulness
    /// invariant, not a heuristic.
    pub(crate) fn verify_callable_throws_fragments(
        &self,
        db: &ProjectDatabase,
    ) -> anyhow::Result<()> {
        if !Self::verify_enabled() {
            return Ok(());
        }
        self.check_callable_throws_fragments_against_honest(db)
    }

    /// The env-independent core of [`Self::verify_callable_throws_fragments`], so the
    /// oracle's discriminating power (pass on a faithful fragment, bail on a
    /// stale one) is unit-testable without mutating the process environment.
    pub(crate) fn check_callable_throws_fragments_against_honest(
        &self,
        db: &ProjectDatabase,
    ) -> anyhow::Result<()> {
        let Some(manifest) = self.load_prev_manifest_for_verify() else {
            return Ok(());
        };
        let Some(root) = db.get_project().map(|p| p.root(db).clone()) else {
            return Ok(());
        };
        // Exactly the files a served warm compile would have seeded. The
        // served artifact is the manifest-resident fragment blob (seeds
        // project from the manifest, not from unit payloads), so that copy is
        // what the oracle must compare.
        let clean_files = compute_dirty_partition(db, &manifest).clean_files;

        for entry in &manifest.files {
            if !clean_files.contains(&entry.rel_path) || entry.callable_throws_fragment.is_empty() {
                continue;
            }
            let full = root.join(&entry.rel_path);
            let Some(sf) = db.get_file(&full) else {
                continue; // file removed — never seeded
            };
            let honest =
                baml_db::baml_compiler2_hir_ty::package_interface::file_callable_throws_fragment(
                    db, sf,
                );
            let honest_bytes = borsh::to_vec(honest).map_err(|e| {
                anyhow::anyhow!(
                    "honest interface fragment for `{}` failed to serialize: {e}",
                    entry.rel_path
                )
            })?;
            if honest_bytes != entry.callable_throws_fragment {
                anyhow::bail!(
                    "BAML_CACHE_VERIFY: cached interface fragment for `{}` differs from a fresh \
                     derivation ({} cached vs {} fresh bytes). A clean file's stored fragment is \
                     a stale substitute — the throws-taint closure failed to dirty a file whose \
                     `callable_throws` changed, so the seeded value would be \
                     wrong. Please report this.",
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

    /// Pick the one served clean file this compile should sample-verify, keyed
    /// off the program cache key (see [`sampled_pick_from_key`]). `None` when
    /// verify mode is active (it already does the full compare), this compile
    /// isn't sampled, or the plan serves no clean file.
    fn sampled_verify_pick(&self, plan: &ReusePlan) -> Option<String> {
        if Self::verify_enabled() {
            return None;
        }
        let mut clean: Vec<&str> = plan.clean_files.iter().map(String::as_str).collect();
        clean.sort_unstable();
        sampled_pick_from_key(self.key.as_bytes(), &clean, Self::sampled_verify_force())
            .map(str::to_string)
    }

    /// Sampled field verification driver (the always-on tripwire). If this warm
    /// compile is sampled, build an honest database — deferred to `FnOnce` so an
    /// unsampled compile pays nothing — and verify one served clean file's
    /// artifacts against it. Meant to be called *after* the compile result is
    /// produced, so user-visible latency grows by at most one file's honest work.
    ///
    /// `build_honest_db` MUST produce a FRESH database from the same sources
    /// with NO per-file reuse seeds installed (throw facts, `callable_throws`,
    /// diagnostics). The compile database is seeded with exactly those, so
    /// re-deriving on it would compare a served seed against itself and prove
    /// nothing — the honest oracle only has teeth on a database that derives the
    /// user artifacts itself, exactly as full verify gets by short-circuiting
    /// `plan_reuse`. (The stdlib build-constant *is* seeded below, for speed;
    /// that is orthogonal to user-file staleness — see the seeding comment.)
    pub(crate) fn maybe_sampled_verify(
        &self,
        plan: Option<&ReusePlan>,
        build_honest_db: impl FnOnce() -> ProjectDatabase,
    ) -> anyhow::Result<()> {
        let Some(plan) = plan else {
            // Cold compile or a whole-image cache hit: nothing was served
            // artifact-by-artifact, so there is nothing to sample. (A pure hit
            // serves the program blob; sampling it would require a full honest
            // compile — unacceptable — so hits stay unsampled by design.)
            return Ok(());
        };
        let Some(rel) = self.sampled_verify_pick(plan) else {
            return Ok(());
        };
        cache_debug(format_args!("sampled field verify: checking `{rel}`"));
        let mut honest_db = build_honest_db();
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
        self.verify_sampled_artifact(&honest_db, plan, &rel)
    }

    /// Compare one served clean file's diagnostics blob and `callable_throws`
    /// fragment against an honest re-derivation on `honest_db` (which must be
    /// un-seeded — see [`Self::maybe_sampled_verify`]). A mismatch is a hard
    /// error: the incremental cache served a stale artifact, so the user's warm
    /// build silently diverged from an honest compile. The message points at
    /// `BAML_CACHE_VERIFY=1` (the full byte-compare) and names the file and
    /// artifact kind, mirroring rustc's ICE-on-fingerprint-mismatch.
    pub(crate) fn verify_sampled_artifact(
        &self,
        honest_db: &ProjectDatabase,
        plan: &ReusePlan,
        rel: &str,
    ) -> anyhow::Result<()> {
        let Some(root) = honest_db.get_project().map(|p| p.root(honest_db).clone()) else {
            return Ok(());
        };
        let full = root.join(rel);
        let Some(sf) = honest_db.get_file(&full) else {
            return Ok(()); // file vanished between planning and verify — unserved
        };

        // (1) Served diagnostics blob vs a fresh per-file check. A blob that
        // fails to rehydrate would have degraded to a re-check (never served
        // stale), so it is not a mismatch — skip it, as the full oracle does.
        if let Some(blob) = plan.clean_diagnostics.get(rel)
            && let Some(served) =
                crate::diagnostics_cache::rehydrate_file_blob(honest_db, &root, blob)
        {
            let fresh = honest_db.check_file(sf);
            if !diagnostic_sets_equal(&served, &fresh) {
                anyhow::bail!(
                    "BAML_CACHE_SAMPLED_VERIFY: the incremental cache served STALE diagnostics \
                     for `{rel}` ({} served vs {} from an honest check). This is a \
                     cache-soundness bug — a warm build would report different errors than a \
                     clean one. Re-run with BAML_CACHE_VERIFY=1 for the full compare and please \
                     report this (file `{rel}`, artifact: diagnostics).",
                    served.len(),
                    fresh.len(),
                );
            }
        }

        // (2) Served `callable_throws` fragment vs an honest derivation. The
        // served copy is the manifest-resident blob (what the seeds project
        // from); an empty fragment seeds nothing, so there is no served
        // artifact to check.
        if let Some(fragment) = plan.clean_fragments.get(rel)
            && !fragment.is_empty()
        {
            let honest =
                baml_db::baml_compiler2_hir_ty::package_interface::file_callable_throws_fragment(
                    honest_db, sf,
                );
            let honest_bytes = borsh::to_vec(honest)?;
            if honest_bytes != *fragment {
                anyhow::bail!(
                    "BAML_CACHE_SAMPLED_VERIFY: the incremental cache served a STALE \
                     callable-throws seed for `{rel}` ({} served vs {} honest bytes). This is a \
                     cache-soundness bug — a warm build would infer different throws than a clean \
                     one. Re-run with BAML_CACHE_VERIFY=1 for the full compare and please report \
                     this (file `{rel}`, artifact: callable-throws fragment).",
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
        let bytes = self
            .cache
            .load_raw(&self.manifest_key)
            .expect("manifest present");
        let mut manifest: ProjectManifest = borsh::from_slice(&bytes).expect("manifest decodes");
        manifest
            .files
            .iter_mut()
            .find(|file| file.rel_path == rel_path)
            .expect("manifest file present")
            .diagnostics = diagnostics;
        let payload = borsh::to_vec(&manifest).expect("manifest serializes");
        self.cache
            .store_raw(&self.manifest_key, &payload)
            .expect("manifest re-stored");
    }

    /// Test hook: point one manifest entry at a newly content-addressed unit
    /// carrying a poisoned interface fragment. Used by the fragment verify
    /// oracle's negative test.
    #[cfg(test)]
    pub(crate) fn poison_callable_throws_fragment_for_test(
        &self,
        rel_path: &str,
        fragment: Vec<u8>,
    ) {
        let manifest_bytes = self
            .cache
            .load_raw(&self.manifest_key)
            .expect("manifest present");
        let mut manifest: ProjectManifest =
            borsh::from_slice(&manifest_bytes).expect("manifest decodes");
        let entry = manifest
            .files
            .iter_mut()
            .find(|entry| entry.rel_path == rel_path)
            .expect("manifest file present");
        let mut unit = self
            .cache
            .load_unit_shared(&CacheKey::from_bytes(entry.unit_key))
            .expect("unit present");
        unit.callable_throws_fragment = fragment.clone();
        let (key, _) = self
            .cache
            .store_unit_shared(&unit)
            .expect("poisoned unit stored");
        entry.unit_key = *key.as_bytes();
        // The manifest carries its own verbatim fragment copy (the one seeds
        // project from) — poison it too so the drift is observable on the
        // served artifact, matching what a real store-path bug would produce.
        entry.callable_throws_fragment = fragment;
        let payload = borsh::to_vec(&manifest).expect("manifest serializes");
        self.cache
            .store_raw(&self.manifest_key, &payload)
            .expect("manifest re-stored");
    }
}

#[derive(Clone, Copy)]
struct DiagnosticsServePlan<'a> {
    clean_files: &'a HashSet<String>,
    clean_diagnostics: &'a std::collections::BTreeMap<String, Vec<u8>>,
}

/// The merged (gate/render) diagnostics plus the per-file blobs to persist,
/// produced by [`CacheContext::collect_diagnostics_incremental`].
pub(crate) struct IncrementalDiagnostics {
    pub(crate) merged: Vec<baml_db::baml_compiler_diagnostics::Diagnostic>,
    pub(crate) fresh_by_file: std::collections::BTreeMap<String, Vec<u8>>,
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
/// chosen clean file — `None` when this compile is not sampled or nothing is
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
///   **sorted** clean set, so the pick is stable regardless of set iteration
///   order.
fn sampled_pick_from_key<'a>(
    key: &[u8; 32],
    clean_sorted: &[&'a str],
    force: Option<bool>,
) -> Option<&'a str> {
    let sample = match force {
        Some(forced) => forced,
        None => key[0] & 31 == 0,
    };
    if !sample || clean_sorted.is_empty() {
        return None;
    }
    let idx = u32::from_le_bytes([key[1], key[2], key[3], key[4]]) as usize % clean_sorted.len();
    Some(clean_sorted[idx])
}

#[cfg(test)]
mod tests {
    //! Soundness of the dirty-set computation and the Phase 2 seeding path.
    //! Covers, in layers:
    //!   - the layout dependencies the bytecode-operand set alone misses (field
    //!     reorder, enum variant reorder, type-alias change, inferred-receiver
    //!     field access) — a `plan_reuse` test caches a compile, edits one type,
    //!     and asserts the affected consumer is dirtied while an unrelated file
    //!     stays clean;
    //!   - the transitive throws-taint closure and the signature-meaning cascade
    //!     (direct / clean-intermediary / firewall / body-only-reference cases),
    //!     asserted on the dirty/clean/seeded partition and on served==honest
    //!     diagnostics and byte-identity;
    //!   - the stdlib typed-interface cache and per-file throws seeds;
    //!   - the fragment and diagnostics `BAML_CACHE_VERIFY` oracles (a faithful
    //!     cache passes, a poisoned one bails).

    use std::path::{Path, PathBuf};

    use super::*;
    use crate::cache_test_support::{
        cache_disabled, compile_and_store_v1, dirty_basenames, opts, resolved, unique_root,
    };

    /// A unique on-disk root for a `bytecode_cache` disk-round-trip test.
    fn bc_root() -> PathBuf {
        unique_root("baml-bc-cache-test")
    }

    fn build_db(files: &[(&str, &str)]) -> ProjectDatabase {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("/bc-test"));
        for (name, content) in files {
            db.add_or_update_file(&Path::new("/bc-test").join(name), content);
        }
        db
    }

    fn file_named(db: &ProjectDatabase, name: &str) -> SourceFile {
        db.get_source_files()
            .into_iter()
            .find(|sf| sf.path(db).to_string_lossy().ends_with(name))
            .expect("source file present")
    }

    /// Compile+cache `initial`, then plan a reuse against `edited`; return the
    /// set of dirty file names (`None` when caching is disabled by env). A strict
    /// subset of [`plan_after_edit`]'s partition.
    fn dirty_after_edit(
        initial: &[(&str, &str)],
        edited: &[(&str, &str)],
    ) -> Option<HashSet<String>> {
        plan_after_edit(initial, edited).map(|plan| plan.dirty)
    }

    /// The reuse plan's file partition after an edit, by basename: which files
    /// are dirty, which stay clean, and which carry a `callable_throws` seed
    /// (Phase 2). Used by the seeding-specific scenarios below.
    #[derive(Debug)]
    struct PlanSummary {
        dirty: HashSet<String>,
        clean: HashSet<String>,
        seeded: HashSet<String>,
    }

    fn basename(s: &str) -> String {
        Path::new(s)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| s.to_string())
    }

    /// Compile+cache `initial`, then plan a reuse against `edited` and summarize
    /// the partition (dirty / clean / seeded) by basename. `None` when caching is
    /// disabled. Every scenario keeps at
    /// least one unrelated clean file so a reuse plan is always produced.
    fn plan_after_edit(initial: &[(&str, &str)], edited: &[(&str, &str)]) -> Option<PlanSummary> {
        if cache_disabled() {
            return None;
        }
        let root = bc_root();
        let _ = compile_and_store_v1(&root, initial);

        let r2 = resolved(&root, edited);
        let db2 = crate::project_load::build_db_from_sources(&r2, |_| {});
        let ctx2 = CacheContext::open(&r2, false).expect("cache reopens");
        let plan = ctx2.plan_reuse(&db2).expect("reuse plan available");

        let dirty = dirty_basenames(&plan.dirty_files, &db2);
        let clean: HashSet<String> = plan.clean_files.iter().map(|r| basename(r)).collect();
        let seeded: HashSet<String> = plan
            .seeded_callable_throws
            .keys()
            .map(|p| basename(p))
            .collect();
        let _ = std::fs::remove_dir_all(&root);
        Some(PlanSummary {
            dirty,
            clean,
            seeded,
        })
    }

    /// Like [`plan_after_edit`], but also runs the full incremental flow the CLI
    /// runs — seed the plan's throw facts + `callable_throws`, relink through the
    /// reuse units — and compares the relinked `Program` byte-for-byte against an
    /// honest full compile of the same edited sources. Returns the plan summary
    /// plus whether relink and full are byte-identical. `None` when caching is
    /// disabled.
    fn plan_and_relink_after_edit(
        initial: &[(&str, &str)],
        edited: &[(&str, &str)],
    ) -> Option<(PlanSummary, bool)> {
        if cache_disabled() {
            return None;
        }
        let root = bc_root();
        let _ = compile_and_store_v1(&root, initial);

        // v2 served path: plan reuse, seed exactly as the CLI does, relink.
        let r2 = resolved(&root, edited);
        let mut db2 = crate::project_load::build_db_from_sources(&r2, |_| {});
        let ctx2 = CacheContext::open(&r2, false).expect("cache reopens");
        let pending_plan = ctx2.plan_reuse(&db2);
        let seeded = pending_plan
            .as_ref()
            .into_iter()
            .flat_map(|plan| plan.seeded_callable_throws.keys())
            .map(|path| basename(path))
            .collect();
        let plan = prepare_reuse_plan(&mut db2, pending_plan).expect("reuse plan available");
        let relinked =
            compile_program(&db2, &opts(), Some(&ctx2), Some(&plan)).expect("relink compile");

        // v2 honest path: an independent fresh database, no reuse plan — the
        // stdlib-spliced full compile the relink must reproduce byte-for-byte.
        let db_full = crate::project_load::build_db_from_sources(&r2, |_| {});
        let full = compile_program(&db_full, &opts(), Some(&ctx2), None).expect("full compile");
        let byte_identical = borsh::to_vec(&relinked).expect("ser relink")
            == borsh::to_vec(&full).expect("ser full");

        let summary = PlanSummary {
            dirty: dirty_basenames(&plan.dirty_files, &db2),
            clean: plan.clean_files.iter().map(|r| basename(r)).collect(),
            seeded,
        };
        let _ = std::fs::remove_dir_all(&root);
        Some((summary, byte_identical))
    }

    /// Like [`plan_and_relink_after_edit`], but additionally asserts diagnostics
    /// parity: the warm *served* diagnostics (reuse plan honored — clean files
    /// served from cache, dirty files re-checked) must equal an independent fresh
    /// database's full check. Returns the plan summary, whether served == honest,
    /// and whether relink == full.
    fn plan_diags_and_relink_after_edit(
        initial: &[(&str, &str)],
        edited: &[(&str, &str)],
    ) -> Option<(PlanSummary, bool, bool)> {
        if cache_disabled() {
            return None;
        }
        let root = bc_root();
        let _ = compile_and_store_v1(&root, initial);

        let r2 = resolved(&root, edited);
        let mut db2 = crate::project_load::build_db_from_sources(&r2, |_| {});
        let ctx2 = CacheContext::open(&r2, false).expect("cache reopens");
        let pending_plan = ctx2.plan_reuse(&db2);
        let seeded = pending_plan
            .as_ref()
            .into_iter()
            .flat_map(|plan| plan.seeded_callable_throws.keys())
            .map(|path| basename(path))
            .collect();
        let plan = prepare_reuse_plan(&mut db2, pending_plan).expect("reuse plan available");

        // Warm served diagnostics (plan honored) vs an honest full check in an
        // independent database with no seeds.
        let served = ctx2
            .collect_diagnostics_incremental(&db2, Some(&plan))
            .merged;
        let db_honest = crate::project_load::build_db_from_sources(&r2, |_| {});
        let honest = baml_project::collect_compiler2_diagnostics(&db_honest);
        let diags_match = diagnostic_sets_equal(&served, &honest);

        let relinked =
            compile_program(&db2, &opts(), Some(&ctx2), Some(&plan)).expect("relink compile");
        let full = compile_program(&db_honest, &opts(), Some(&ctx2), None).expect("full compile");
        let byte_identical = borsh::to_vec(&relinked).expect("ser relink")
            == borsh::to_vec(&full).expect("ser full");

        let summary = PlanSummary {
            dirty: dirty_basenames(&plan.dirty_files, &db2),
            clean: plan.clean_files.iter().map(|r| basename(r)).collect(),
            seeded,
        };
        let _ = std::fs::remove_dir_all(&root);
        Some((summary, diags_match, byte_identical))
    }

    // ── Layout-scoped sentinel (mixed class+function files) ──────────────────

    /// A function-signature edit in a file that ALSO defines a class must leave
    /// the layout sentinel unraised: only the edited file is dirty, an unrelated
    /// layout-baking file stays clean (the whole win), and the relink is
    /// byte-identical to a full compile.
    #[test]
    fn plan_reuse_mixed_file_function_sig_edit_stays_minimal() {
        let mixed_v1 = "class Widget {\n  w int\n  h int\n}\n\
                        function helper(a: int) -> int {\n  a\n}\n";
        // Only `helper`'s signature changes; `Widget`'s layout is untouched.
        let mixed_v2 = "class Widget {\n  w int\n  h int\n}\n\
                        function helper(a: int, b: int) -> int {\n  a + b\n}\n";
        // A layout-baker naming nothing `mixed.baml` defines: `o.a` bakes
        // `Other`'s field offset, so it carries LAYOUT_SENTINEL — the file the
        // old "any sig change in a type-defining file" rule wrongly dirtied.
        let baker = "class Other {\n  a int\n  b int\n}\n\
                     function reado(o: Other) -> int {\n  o.a\n}\n";
        let unrelated = "function unrelated() -> int {\n  42\n}\n";
        let initial = [
            ("mixed.baml", mixed_v1),
            ("baker.baml", baker),
            ("z.baml", unrelated),
        ];
        let edited = [
            ("mixed.baml", mixed_v2),
            ("baker.baml", baker),
            ("z.baml", unrelated),
        ];
        let Some((p, byte_identical)) = plan_and_relink_after_edit(&initial, &edited) else {
            return;
        };
        assert!(
            p.dirty.contains("mixed.baml"),
            "the edited file must be dirty; {p:?}"
        );
        assert!(
            !p.dirty.contains("baker.baml"),
            "a layout-baking file must stay clean on a function-only sig edit — \
             the layout sentinel must NOT fire; dirty = {:?}",
            p.dirty
        );
        assert!(
            p.clean.contains("baker.baml") && p.clean.contains("z.baml"),
            "both non-edited files stay clean; {p:?}"
        );
        assert!(
            byte_identical,
            "relink must be byte-identical to a full compile"
        );
    }

    /// A field reorder in that same mixed file MUST fire the sentinel: the
    /// layout-baking `baker.baml` is dragged into the dirty set, and the relink
    /// stays byte-identical.
    #[test]
    fn plan_reuse_mixed_file_field_reorder_fires_sentinel() {
        let mixed_v1 = "class Widget {\n  w int\n  h int\n}\n\
                        function helper(a: int) -> int {\n  a\n}\n";
        let mixed_v2 = "class Widget {\n  h int\n  w int\n}\n\
                        function helper(a: int) -> int {\n  a\n}\n";
        let baker = "class Other {\n  a int\n  b int\n}\n\
                     function reado(o: Other) -> int {\n  o.a\n}\n";
        let unrelated = "function unrelated() -> int {\n  42\n}\n";
        let initial = [
            ("mixed.baml", mixed_v1),
            ("baker.baml", baker),
            ("z.baml", unrelated),
        ];
        let edited = [
            ("mixed.baml", mixed_v2),
            ("baker.baml", baker),
            ("z.baml", unrelated),
        ];
        let Some((p, byte_identical)) = plan_and_relink_after_edit(&initial, &edited) else {
            return;
        };
        assert!(
            p.dirty.contains("mixed.baml"),
            "the reordered file must be dirty; {p:?}"
        );
        assert!(
            p.dirty.contains("baker.baml"),
            "a field reorder must fire the layout sentinel and dirty every \
             layout-baking file; dirty = {:?}",
            p.dirty
        );
        assert!(
            byte_identical,
            "relink must be byte-identical to a full compile"
        );
    }

    /// Editing a class's generic-parameter list is a layout change — it must
    /// fire the sentinel (the generic-param count feeds the class object),
    /// dirtying layout-baking files, byte-identity preserved.
    #[test]
    fn plan_reuse_mixed_file_type_param_edit_fires_sentinel() {
        let mixed_v1 = "class Box<T> {\n  value T\n}\n\
                        function helper(a: int) -> int {\n  a\n}\n";
        let mixed_v2 = "class Box<T, U> {\n  value T\n}\n\
                        function helper(a: int) -> int {\n  a\n}\n";
        let baker = "class Other {\n  a int\n  b int\n}\n\
                     function reado(o: Other) -> int {\n  o.a\n}\n";
        let unrelated = "function unrelated() -> int {\n  42\n}\n";
        let initial = [
            ("mixed.baml", mixed_v1),
            ("baker.baml", baker),
            ("z.baml", unrelated),
        ];
        let edited = [
            ("mixed.baml", mixed_v2),
            ("baker.baml", baker),
            ("z.baml", unrelated),
        ];
        let Some((p, byte_identical)) = plan_and_relink_after_edit(&initial, &edited) else {
            return;
        };
        assert!(
            p.dirty.contains("mixed.baml"),
            "the edited file must be dirty; {p:?}"
        );
        assert!(
            p.dirty.contains("baker.baml"),
            "a generic-parameter change must fire the layout sentinel; dirty = {:?}",
            p.dirty
        );
        assert!(
            byte_identical,
            "relink must be byte-identical to a full compile"
        );
    }

    // ── FINDING 1: field reorder (receiver named in the signature) ───────────

    #[test]
    fn plan_reuse_dirties_field_reader_on_field_reorder() {
        let initial = [
            ("a.baml", "class Point {\n  x int\n  y int\n}\n"),
            (
                "b.baml",
                "function diff(p: Point) -> int {\n  p.x - p.y\n}\n",
            ),
            ("c.baml", "function unrelated() -> int {\n  42\n}\n"),
        ];
        let edited = [
            ("a.baml", "class Point {\n  y int\n  x int\n}\n"),
            (
                "b.baml",
                "function diff(p: Point) -> int {\n  p.x - p.y\n}\n",
            ),
            ("c.baml", "function unrelated() -> int {\n  42\n}\n"),
        ];
        let Some(dirty) = dirty_after_edit(&initial, &edited) else {
            return;
        };
        assert!(
            dirty.contains("b.baml"),
            "field reader must be dirtied when Point's fields reorder; dirty = {dirty:?}"
        );
        assert!(
            !dirty.contains("c.baml"),
            "a file referencing no changed type must stay clean; dirty = {dirty:?}"
        );
    }

    // ── FINDING 1 (deep): inferred receiver, caught only by the sentinel ─────

    #[test]
    fn plan_reuse_dirties_inferred_receiver_field_access_via_layout_sentinel() {
        // `use_it` names no type: `p`'s class is inferred from `mk()`'s return,
        // so only the layout sentinel (its `LoadField` bakes Point's offsets)
        // can catch a reorder. `mk` lives in its own file, so reordering Point
        // does not change `mk`'s file signature — ruling out a name-based hit.
        let point_v1 = "class Point {\n  x int\n  y int\n}\n";
        let point_v2 = "class Point {\n  y int\n  x int\n}\n";
        let mk = "function mk() -> Point {\n  Point { x: 1, y: 2 }\n}\n";
        let use_it = "function use_it() -> int {\n  let p = mk();\n  p.x\n}\n";
        let unrelated = "function unrelated() -> int {\n  42\n}\n";
        let initial = [
            ("a.baml", point_v1),
            ("d.baml", mk),
            ("b.baml", use_it),
            ("c.baml", unrelated),
        ];
        let edited = [
            ("a.baml", point_v2),
            ("d.baml", mk),
            ("b.baml", use_it),
            ("c.baml", unrelated),
        ];
        let Some(dirty) = dirty_after_edit(&initial, &edited) else {
            return;
        };
        assert!(
            dirty.contains("b.baml"),
            "inferred-receiver field reader must be dirtied via the layout sentinel; \
             dirty = {dirty:?}"
        );
        assert!(
            !dirty.contains("c.baml"),
            "unrelated file must stay clean; dirty = {dirty:?}"
        );
    }

    // ── FINDING 1 (enum analog): variant reorder ─────────────────────────────

    #[test]
    fn plan_reuse_dirties_enum_consumer_on_variant_reorder() {
        let initial = [
            ("a.baml", "enum Color {\n  Red\n  Green\n}\n"),
            ("b.baml", "function pick(c: Color) -> Color {\n  c\n}\n"),
            ("c.baml", "function unrelated() -> int {\n  42\n}\n"),
        ];
        let edited = [
            ("a.baml", "enum Color {\n  Green\n  Red\n}\n"),
            ("b.baml", "function pick(c: Color) -> Color {\n  c\n}\n"),
            ("c.baml", "function unrelated() -> int {\n  42\n}\n"),
        ];
        let Some(dirty) = dirty_after_edit(&initial, &edited) else {
            return;
        };
        assert!(
            dirty.contains("b.baml"),
            "enum consumer must be dirtied when Color's variants reorder; dirty = {dirty:?}"
        );
        assert!(
            !dirty.contains("c.baml"),
            "unrelated file must stay clean; dirty = {dirty:?}"
        );
    }

    // ── FINDING 2: type-alias change ─────────────────────────────────────────

    #[test]
    fn plan_reuse_dirties_type_alias_consumer_on_alias_change() {
        // `pay` forwards a `Money` value (no field access → no layout sentinel),
        // so only tracking the *alias name* `Money` — via `defined_names` and
        // `syntactic_type_names` — can dirty it when the RHS changes.
        let a_v1 = "class Dollars {\n  v int\n}\nclass Euros {\n  v int\n}\ntype Money = Dollars\n";
        let a_v2 = "class Dollars {\n  v int\n}\nclass Euros {\n  v int\n}\ntype Money = Euros\n";
        let pay = "function pay(m: Money) -> Money {\n  m\n}\n";
        let unrelated = "function unrelated() -> int {\n  42\n}\n";
        let initial = [("a.baml", a_v1), ("b.baml", pay), ("c.baml", unrelated)];
        let edited = [("a.baml", a_v2), ("b.baml", pay), ("c.baml", unrelated)];
        let Some(dirty) = dirty_after_edit(&initial, &edited) else {
            return;
        };
        assert!(
            dirty.contains("b.baml"),
            "type-alias consumer must be dirtied when the alias RHS changes; dirty = {dirty:?}"
        );
    }

    // ── Risk 1: inferred-throws change dirties the caller ────────────────────

    #[test]
    fn plan_reuse_dirties_caller_on_callee_inferred_throws_change() {
        // A body-only edit to `risky` grows its inferred throws (none -> MyErr)
        // without touching its signature, so its `signature_hash` is unchanged
        // and the name-based dirty set alone would leave the caller clean. Only
        // the throws-change propagation (Risk 1) dirties `caller`, whose
        // throws-contract / catch diagnostics depend on `risky`'s throws.
        let err = "class MyErr {\n  msg string\n}\n";
        let risky_v1 = "function risky(x: int) -> int {\n  x\n}\n";
        let risky_v2 = "function risky(x: int) -> int {\n  throw MyErr { msg: \"boom\" }\n}\n";
        let caller = "function caller(x: int) -> int {\n  risky(x)\n}\n";
        let unrelated = "function unrelated() -> int {\n  42\n}\n";
        let initial = [
            ("err.baml", err),
            ("risky.baml", risky_v1),
            ("caller.baml", caller),
            ("z.baml", unrelated),
        ];
        let edited = [
            ("err.baml", err),
            ("risky.baml", risky_v2),
            ("caller.baml", caller),
            ("z.baml", unrelated),
        ];
        let Some(dirty) = dirty_after_edit(&initial, &edited) else {
            return;
        };
        assert!(
            dirty.contains("caller.baml"),
            "caller must be dirtied when the callee's inferred throws grow; dirty = {dirty:?}"
        );
        assert!(
            !dirty.contains("z.baml"),
            "an unrelated file must stay clean; dirty = {dirty:?}"
        );
    }

    // ── Phase 2d: seeding-specific scenarios ─────────────────────────────────

    #[test]
    fn plan_reuse_dirties_transitive_caller_through_clean_intermediary() {
        // A -> B -> C. Only C's body is edited (its inferred throws grow from
        // none to MyErr). B and A are byte-identical, so B is a *clean
        // intermediary*: its own `file_throw_facts` are unchanged, yet its solved
        // transitive throws grew and A's throws grew through it. The one-hop
        // propagation reaches only B (which references C by name); ONLY the
        // transitive throws-taint closure reaches A. This is the sharp §5b case
        // — A must be dirtied so its `callable_throws` seed is never served stale.
        let err = "class MyErr {\n  msg string\n}\n";
        let c_v1 = "function c() -> int {\n  1\n}\n";
        let c_v2 = "function c() -> int {\n  throw MyErr { msg: \"boom\" }\n}\n";
        let b = "function b() -> int {\n  c()\n}\n";
        let a = "function a() -> int {\n  b()\n}\n";
        let unrelated = "function unrelated() -> int {\n  42\n}\n";
        let initial = [
            ("err.baml", err),
            ("c.baml", c_v1),
            ("b.baml", b),
            ("a.baml", a),
            ("z.baml", unrelated),
        ];
        let edited = [
            ("err.baml", err),
            ("c.baml", c_v2),
            ("b.baml", b),
            ("a.baml", a),
            ("z.baml", unrelated),
        ];
        let Some(p) = plan_after_edit(&initial, &edited) else {
            return;
        };
        assert!(
            p.dirty.contains("c.baml"),
            "the edited callee must be dirty; {p:?}"
        );
        assert!(
            p.dirty.contains("b.baml"),
            "the clean intermediary (direct caller of C) must be dirty; {p:?}"
        );
        assert!(
            p.dirty.contains("a.baml"),
            "the TRANSITIVE caller through the clean intermediary must be dirty — \
             only the taint closure reaches it; dirty = {:?}",
            p.dirty
        );
        assert!(
            !p.seeded.contains("a.baml"),
            "a transitively-tainted caller must NOT be seeded; seeded = {:?}",
            p.seeded
        );
        assert!(
            p.clean.contains("z.baml") && p.seeded.contains("z.baml"),
            "an unrelated file stays clean and seeded; {p:?}"
        );
    }

    #[test]
    fn plan_reuse_firewall_stops_taint_at_closed_throws_contract() {
        // A -> B -> C, but B declares a *closed* `throws` contract. C's inferred
        // throws change ({MyErr} -> {OtherErr}), so B is (over-)dirtied for
        // referencing C — but B's `callable_throws` is its declared set,
        // independent of C, so the taint STOPS at B: A's throws are unchanged, so
        // A stays clean and its seed remains valid.
        let err = "class MyErr {\n  msg string\n}\nclass OtherErr {\n  msg string\n}\n";
        let c_v1 = "function c() -> int {\n  throw MyErr { msg: \"x\" }\n}\n";
        let c_v2 = "function c() -> int {\n  throw OtherErr { msg: \"y\" }\n}\n";
        let b = "function b() -> int throws MyErr {\n  c()\n}\n";
        let a = "function a() -> int {\n  b()\n}\n";
        let unrelated = "function unrelated() -> int {\n  42\n}\n";
        let initial = [
            ("err.baml", err),
            ("c.baml", c_v1),
            ("b.baml", b),
            ("a.baml", a),
            ("z.baml", unrelated),
        ];
        let edited = [
            ("err.baml", err),
            ("c.baml", c_v2),
            ("b.baml", b),
            ("a.baml", a),
            ("z.baml", unrelated),
        ];
        let Some(p) = plan_after_edit(&initial, &edited) else {
            return;
        };
        assert!(
            p.dirty.contains("c.baml"),
            "the edited callee must be dirty; {p:?}"
        );
        assert!(
            p.clean.contains("a.baml"),
            "the closed-`throws` firewall must keep A clean; dirty = {:?}",
            p.dirty
        );
        assert!(
            p.seeded.contains("a.baml"),
            "A must be seeded — its throws are unchanged behind the firewall; \
             seeded = {:?}",
            p.seeded
        );
    }

    #[test]
    fn plan_reuse_body_edit_not_touching_throws_keeps_callers_seeded() {
        // A -> B -> C. B's body is rewritten (a no-op `let`), so B is
        // content-dirty and re-emitted, but its inferred throws are unchanged, so
        // the taint closure never fires: A stays clean and its `callable_throws`
        // seed is served, as does the untouched callee C.
        let c = "function c() -> int {\n  1\n}\n";
        let b_v1 = "function b() -> int {\n  c()\n}\n";
        let b_v2 = "function b() -> int {\n  let r = c()\n  r\n}\n";
        let a = "function a() -> int {\n  b()\n}\n";
        let unrelated = "function unrelated() -> int {\n  42\n}\n";
        let initial = [
            ("c.baml", c),
            ("b.baml", b_v1),
            ("a.baml", a),
            ("z.baml", unrelated),
        ];
        let edited = [
            ("c.baml", c),
            ("b.baml", b_v2),
            ("a.baml", a),
            ("z.baml", unrelated),
        ];
        let Some(p) = plan_after_edit(&initial, &edited) else {
            return;
        };
        assert!(
            p.dirty.contains("b.baml"),
            "the body-edited file must be dirty; {p:?}"
        );
        assert!(
            p.clean.contains("a.baml") && p.seeded.contains("a.baml"),
            "a caller whose callee's throws are unchanged stays clean and seeded; {p:?}"
        );
        assert!(
            p.clean.contains("c.baml") && p.seeded.contains("c.baml"),
            "the untouched callee stays clean and seeded; {p:?}"
        );
    }

    #[test]
    fn plan_reuse_signature_edit_invalidates_caller_seed() {
        // A change to C's *signature* (a new param) dirties its direct caller B
        // (which names C), so B's seed is invalidated — riding the same name
        // propagation the throws closure extends, but proving a signature edit is
        // also caught for the `callable_throws` seed.
        let c_v1 = "function c() -> int {\n  1\n}\n";
        let c_v2 = "function c(x: int) -> int {\n  x\n}\n";
        let b_v1 = "function b() -> int {\n  c()\n}\n";
        let b_v2 = "function b() -> int {\n  c(1)\n}\n";
        let unrelated = "function unrelated() -> int {\n  42\n}\n";
        let initial = [("c.baml", c_v1), ("b.baml", b_v1), ("z.baml", unrelated)];
        let edited = [("c.baml", c_v2), ("b.baml", b_v2), ("z.baml", unrelated)];
        let Some(p) = plan_after_edit(&initial, &edited) else {
            return;
        };
        assert!(
            p.dirty.contains("c.baml") && p.dirty.contains("b.baml"),
            "a signature edit dirties the callee and its caller; {p:?}"
        );
        assert!(
            !p.seeded.contains("b.baml"),
            "a dirtied caller must not be seeded; seeded = {:?}",
            p.seeded
        );
        assert!(
            p.clean.contains("z.baml") && p.seeded.contains("z.baml"),
            "an unrelated file stays clean and seeded; {p:?}"
        );
    }

    // ── Transitive signature-meaning cascade (alias re-target) ───────────────

    #[test]
    fn plan_reuse_cascades_signature_meaning_through_clean_callee() {
        // The soundness repro. `alias.baml` re-targets `type AppErr` from `Boom`
        // to `Kaboom`. `sink` is content-unchanged but names `AppErr` in its
        // signature, so its resolved parameter type moves — the signature-meaning
        // cascade dirties it AND joins its name to the change set. `boundary` is
        // content-unchanged and names neither `AppErr` nor `Kaboom`; it only calls
        // `sink` in its body, so the one-hop propagation reaches it only *after*
        // the cascade adds `sink` to the change set. Without the cascade, boundary
        // stays clean and its cached no-error diagnostics are served, hiding the
        // arg-type mismatch (Boom passed where sink now expects Kaboom) — a
        // program the honest compiler rejects. With it, served == honest and the
        // relink is byte-identical to a full compile.
        let alias_v1 = "type AppErr = Boom\n";
        let alias_v2 = "type AppErr = Kaboom\n";
        let errs = "class Boom {\n  b int\n}\nclass Kaboom {\n  k int\n}\n";
        let sink = "function sink(e: AppErr) -> int throws AppErr {\n  throw e\n}\n";
        let boundary = "function boundary(e: Boom) -> int {\n  sink(e)\n}\n";
        let unrelated = "function unrelated() -> int {\n  42\n}\n";
        let initial = [
            ("alias.baml", alias_v1),
            ("errs.baml", errs),
            ("sink.baml", sink),
            ("boundary.baml", boundary),
            ("z.baml", unrelated),
        ];
        let edited = [
            ("alias.baml", alias_v2),
            ("errs.baml", errs),
            ("sink.baml", sink),
            ("boundary.baml", boundary),
            ("z.baml", unrelated),
        ];
        let Some((p, diags_match, byte_identical)) =
            plan_diags_and_relink_after_edit(&initial, &edited)
        else {
            return;
        };
        assert!(
            p.dirty.contains("alias.baml"),
            "the re-targeted alias file must be dirty; {p:?}"
        );
        assert!(
            p.dirty.contains("sink.baml"),
            "the content-unchanged callee whose signature names the alias must be \
             dirtied by the cascade; {p:?}"
        );
        assert!(
            p.dirty.contains("boundary.baml"),
            "the transitive caller — naming neither the alias nor the changed type, \
             only the clean callee `sink` in its body — must be dirtied so its stale \
             no-error diagnostics are never served; dirty = {:?}",
            p.dirty
        );
        assert!(
            diags_match,
            "warm served diagnostics must equal the honest full check — the arg-type \
             mismatch at boundary's call site must not be hidden; {p:?}"
        );
        assert!(
            byte_identical,
            "the relink must reproduce a full compile byte-for-byte; {p:?}"
        );
    }

    #[test]
    fn plan_reuse_signature_cascade_stops_at_body_only_reference() {
        // The cascade must NOT over-fire: a file that references a changed name
        // only in its *body* is dirtied but does not propagate its own defined
        // names further. `types.baml` re-targets `type Alias` from `A` to `B`.
        // `mid` names `Alias` in its signature and has a closed `throws never`
        // contract, so it cascades (its defined name joins the change set) but is a
        // throws firewall. `bodyref` calls `mid` in its body and constructs `A`, so
        // it is dirtied — but its signature names no changed type, so it must NOT
        // cascade. `topcaller` calls only `bodyref` (behind the firewall), names no
        // changed type, and must therefore stay clean and seeded: proof the cascade
        // stopped at the body-only reference rather than dirtying the whole graph.
        let types_v1 = "type Alias = A\nclass A {\n  a int\n}\nclass B {\n  b int\n}\n";
        let types_v2 = "type Alias = B\nclass A {\n  a int\n}\nclass B {\n  b int\n}\n";
        let mid = "function mid(x: Alias) -> int throws never {\n  0\n}\n";
        let bodyref = "function bodyref() -> int throws never {\n  mid(A { a: 1 })\n}\n";
        let topcaller = "function topcaller() -> int throws never {\n  bodyref()\n}\n";
        let unrelated = "function unrelated() -> int {\n  42\n}\n";
        let initial = [
            ("types.baml", types_v1),
            ("mid.baml", mid),
            ("bodyref.baml", bodyref),
            ("topcaller.baml", topcaller),
            ("z.baml", unrelated),
        ];
        let edited = [
            ("types.baml", types_v2),
            ("mid.baml", mid),
            ("bodyref.baml", bodyref),
            ("topcaller.baml", topcaller),
            ("z.baml", unrelated),
        ];
        let Some(p) = plan_after_edit(&initial, &edited) else {
            return;
        };
        assert!(
            p.dirty.contains("mid.baml"),
            "the callee whose signature names the alias must cascade (be dirty); {p:?}"
        );
        assert!(
            p.dirty.contains("bodyref.baml"),
            "the body-only caller of `mid` must be dirtied (re-checked); {p:?}"
        );
        assert!(
            p.clean.contains("topcaller.baml") && p.seeded.contains("topcaller.baml"),
            "the cascade must STOP at the body-only reference — a caller of the \
             body-only referencer stays clean and seeded; dirty = {:?}",
            p.dirty
        );
    }

    #[test]
    fn verify_diagnostics_passes_when_dependent_dirtied_by_cross_file_edit() {
        // Fix for the over-strict diagnostics oracle: it must gate on the served
        // (clean) set, not the content-unchanged set. Reusing the alias repro, the
        // content-UNCHANGED `boundary` is dirtied by the cascade and would report a
        // NEW error on a fresh check. Because it is dirty it is re-checked, never
        // served, so the oracle must skip it. Gating on `content_hash` instead
        // would compare boundary's stored (clean) blob against its fresh (erroring)
        // check and bail spuriously on this ordinary cross-file edit.
        if cache_disabled() {
            return;
        }
        let errs = "class Boom {\n  b int\n}\nclass Kaboom {\n  k int\n}\n";
        let sink = "function sink(e: AppErr) -> int throws AppErr {\n  throw e\n}\n";
        let boundary = "function boundary(e: Boom) -> int {\n  sink(e)\n}\n";
        let unrelated = "function unrelated() -> int {\n  42\n}\n";
        let initial = [
            ("alias.baml", "type AppErr = Boom\n"),
            ("errs.baml", errs),
            ("sink.baml", sink),
            ("boundary.baml", boundary),
            ("z.baml", unrelated),
        ];
        let edited = [
            ("alias.baml", "type AppErr = Kaboom\n"),
            ("errs.baml", errs),
            ("sink.baml", sink),
            ("boundary.baml", boundary),
            ("z.baml", unrelated),
        ];
        let root = bc_root();
        let _ = compile_and_store_v1(&root, &initial);

        // v2 sources; the diagnostics oracle (env-independent core) must pass:
        // boundary is dirty (skipped), the truly-clean files are unchanged.
        let r2 = resolved(&root, &edited);
        let db2 = crate::project_load::build_db_from_sources(&r2, |_| {});
        let ctx2 = CacheContext::open(&r2, false).expect("cache reopens");
        ctx2.check_cached_diagnostics_against_fresh(&db2).expect(
            "the diagnostics oracle must not bail on a content-unchanged file that \
             the cascade dirtied (it is re-checked, never served stale)",
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
        use baml_compiler2_hir::loc::FunctionLoc;
        use baml_db::baml_compiler2_hir_ty::callable::callable_throws;

        let mut db = build_db(&[(
            "a.baml",
            "class MyErr {\n  msg string\n}\n\
             function f() -> int {\n  1\n}\n\
             function g() -> int {\n  throw MyErr { msg: \"x\" }\n}\n",
        )]);
        let file = file_named(&db, "a.baml");
        let (f_id, g_id) = {
            use baml_compiler2_ppir::item_data::{file_functions, function_data};
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
        let mut by_id = std::collections::BTreeMap::new();
        by_id.insert(f_id.as_u32(), g_throws.clone());
        let mut by_path = std::collections::BTreeMap::new();
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
        // Seed-vs-honest divergence tripwire (unit level): corrupt a clean file's
        // stored fragment and the verify core must bail, naming the file. This is
        // the single assumption the whole scheme rests on — that a served seed
        // equals the honest re-derivation.
        if cache_disabled() {
            return;
        }
        let root = bc_root();
        let files = [
            ("a.baml", "function a() -> int {\n  1\n}\n"),
            ("b.baml", "function b() -> int {\n  2\n}\n"),
        ];
        let _ = compile_and_store_v1(&root, &files);

        // A faithful fragment cache passes the oracle (all files content-clean).
        let r = resolved(&root, &files);
        let db2 = crate::project_load::build_db_from_sources(&r, |_| {});
        let ctx2 = CacheContext::open(&r, false).expect("cache reopens");
        ctx2.check_callable_throws_fragments_against_honest(&db2)
            .expect("faithful fragments pass the oracle");

        // Corrupt a.baml's stored fragment; the oracle must now bail on it.
        ctx2.poison_callable_throws_fragment_for_test("a.baml", vec![0xde, 0xad, 0xbe, 0xef]);
        let db3 = crate::project_load::build_db_from_sources(&r, |_| {});
        let err = ctx2
            .check_callable_throws_fragments_against_honest(&db3)
            .expect_err("a stale stored fragment must bail");
        assert!(
            err.to_string().contains("a.baml"),
            "the bail must name the drifted file; got: {err}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── Risk 2: impl edit dirties coherence peers ────────────────────────────

    #[test]
    fn plan_reuse_dirties_coherence_peer_on_impl_edit() {
        // Editing one impl-bearing file's method body must re-check the OTHER
        // impl-bearing file: an `OverlappingImplements` verdict spans both, and
        // the peer names no symbol the edit changed — only `IMPL_SENTINEL`
        // connects them. The method-body edit keeps the peer's signature intact,
        // so no layout/name path can substitute for the sentinel.
        let iface = "interface Speaker {\n  function speak(self) -> string\n}\n";
        let a = "class Dog {\n  name string\n  implements Speaker {\n    \
                 function speak(self) -> string {\n      \"woof\"\n    }\n  }\n}\n";
        let b_v1 = "class Cat {\n  name string\n  implements Speaker {\n    \
                    function speak(self) -> string {\n      \"meow\"\n    }\n  }\n}\n";
        let b_v2 = "class Cat {\n  name string\n  implements Speaker {\n    \
                    function speak(self) -> string {\n      \"MEOW\"\n    }\n  }\n}\n";
        let unrelated = "function unrelated() -> int {\n  42\n}\n";
        let initial = [
            ("iface.baml", iface),
            ("a.baml", a),
            ("b.baml", b_v1),
            ("z.baml", unrelated),
        ];
        let edited = [
            ("iface.baml", iface),
            ("a.baml", a),
            ("b.baml", b_v2),
            ("z.baml", unrelated),
        ];
        let Some(dirty) = dirty_after_edit(&initial, &edited) else {
            return;
        };
        assert!(
            dirty.contains("b.baml"),
            "the edited impl file must be dirty; dirty = {dirty:?}"
        );
        assert!(
            dirty.contains("a.baml"),
            "a coherence peer must be re-checked via IMPL_SENTINEL; dirty = {dirty:?}"
        );
        assert!(
            !dirty.contains("z.baml"),
            "an impl-free file must stay clean; dirty = {dirty:?}"
        );
    }

    #[test]
    fn plan_reuse_dirties_coherence_peer_when_impl_is_removed() {
        let iface = "interface Speaker {\n  function speak(self) -> string\n}\n";
        let dog = "class Dog {\n  implements Speaker {\n    \
                   function speak(self) -> string {\n      \"woof\"\n    }\n  }\n}\n";
        let cat_with_impl = "class Cat {\n  implements Speaker {\n    \
                             function speak(self) -> string {\n      \"meow\"\n    }\n  }\n}\n";
        let cat_without_impl = "class Cat {}\n";
        let initial = [
            ("iface.baml", iface),
            ("dog.baml", dog),
            ("cat.baml", cat_with_impl),
        ];
        let edited = [
            ("iface.baml", iface),
            ("dog.baml", dog),
            ("cat.baml", cat_without_impl),
        ];
        let Some(dirty) = dirty_after_edit(&initial, &edited) else {
            return;
        };
        assert!(dirty.contains("cat.baml"));
        assert!(
            dirty.contains("dog.baml"),
            "the previous impl sentinel must dirty remaining coherence peers: {dirty:?}"
        );
    }

    #[test]
    fn prepare_reuse_plan_demotes_stale_throws_before_diagnostics() {
        if cache_disabled() {
            return;
        }
        let root = bc_root();
        let initial = [
            ("a.baml", "function stable() -> int {\n  1\n}\n"),
            ("b.baml", "function edited() -> int {\n  1\n}\n"),
            ("c.baml", "function untouched() -> int {\n  3\n}\n"),
        ];
        let edited = [
            ("a.baml", "function stable() -> int {\n  1\n}\n"),
            ("b.baml", "function edited() -> int {\n  2\n}\n"),
            ("c.baml", "function untouched() -> int {\n  3\n}\n"),
        ];

        let _ = compile_and_store_v1(&root, &initial);

        let r2 = resolved(&root, &edited);
        let mut db2 = crate::project_load::build_db_from_sources(&r2, |_| {});
        let ctx2 = CacheContext::open(&r2, false).expect("cache reopens");
        let mut plan = ctx2.plan_reuse(&db2).expect("partial reuse available");
        assert!(plan.clean_files.contains("a.baml"));
        let stable_unit = plan
            .prev_units
            .iter_mut()
            .find(|unit| unit.source_file == "a.baml")
            .expect("stable unit");
        let stable_fn = stable_unit
            .code
            .iter_mut()
            .find_map(|object| match object {
                Object::Function(function) if function.name.ends_with("stable") => Some(function),
                _ => None,
            })
            .expect("stable function");
        stable_fn.throws_type = baml_type::TyTemplate::String {
            attr: baml_type::TyAttr::default(),
        };

        let prepared = prepare_reuse_plan(&mut db2, Some(plan))
            .expect("the unaffected clean unit remains reusable");
        assert!(!prepared.clean_files.contains("a.baml"));
        assert!(prepared.clean_files.contains("c.baml"));
        assert!(prepared.dirty_files.iter().any(|file| {
            file.path(&db2)
                .file_name()
                .is_some_and(|name| name == "a.baml")
        }));
        assert!(!prepared.clean_diagnostics.contains_key("a.baml"));
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── Mechanism-level assertions (independent of the on-disk cache) ─────────

    #[test]
    fn defined_names_includes_type_aliases() {
        let db = build_db(&[("m.baml", "type Money = int\n")]);
        let f = file_named(&db, "m.baml");
        let names = defined_names(&db, f);
        assert!(
            names.iter().any(|n| n == "Money"),
            "a type alias must contribute a defined name; got {names:?}"
        );
    }

    #[test]
    fn syntactic_type_names_capture_signature_and_alias_types() {
        let db = build_db(&[(
            "t.baml",
            "class Point {\n  x int\n  y int\n}\n\
             type Money = int\n\
             function diff(p: Point) -> int {\n  p.x - p.y\n}\n\
             function pay(m: Money) -> Money {\n  m\n}\n",
        )]);
        let f = file_named(&db, "t.baml");
        let names = syntactic_type_names(&db, f);
        for expected in ["Point", "Money"] {
            assert!(
                names.contains(expected),
                "expected `{expected}` among syntactic type names {names:?}"
            );
        }
        assert!(
            !names.contains("int"),
            "primitive keywords must be filtered out; got {names:?}"
        );
    }

    #[test]
    fn referenced_names_carry_layout_sentinel_for_field_reader() {
        let db = build_db(&[(
            "a.baml",
            "class Point {\n  x int\n  y int\n}\n\
             function diff(p: Point) -> int {\n  p.x - p.y\n}\n",
        )]);
        let base = generate_stdlib_program(&db, CLI_OPT_LEVEL).expect("stdlib compiles");
        let program = generate_project_bytecode_with_stdlib(&db, &opts(), CLI_OPT_LEVEL, &base)
            .expect("project compiles");
        let refs = referenced_names_by_file(&program);
        let a = refs.get("a.baml").expect("a.baml has referenced names");
        assert!(
            a.iter().any(|n| n == LAYOUT_SENTINEL),
            "a field reader's bytecode must carry the layout sentinel; got {a:?}"
        );
    }

    // Per-file content-addressed unit storage.

    #[test]
    fn missing_unit_degrades_only_that_file_and_relinks_identically() {
        if cache_disabled() {
            return;
        }
        let files = [
            ("a.baml", "function a() -> int {\n  1\n}\n"),
            ("b.baml", "function b() -> int {\n  2\n}\n"),
            ("c.baml", "function c() -> int {\n  3\n}\n"),
        ];
        // The second compile edits c.baml: unit validation (and hence missing-
        // unit degradation) runs only when the partition has a delta — a
        // no-delta invocation serves entirely from the manifest and never
        // opens unit payloads (a missing unit there surfaces later as the
        // relink fallback to a full compile).
        let edited = [
            ("a.baml", "function a() -> int {\n  1\n}\n"),
            ("b.baml", "function b() -> int {\n  2\n}\n"),
            ("c.baml", "function c() -> int {\n  3 + 0\n}\n"),
        ];
        let root = bc_root();
        let (_db1, ctx1) = compile_and_store_v1(&root, &files);

        let manifest_bytes = ctx1
            .cache
            .load_raw(&ctx1.manifest_key)
            .expect("manifest present");
        let manifest: ProjectManifest =
            borsh::from_slice(&manifest_bytes).expect("manifest decodes");
        let missing_key = CacheKey::from_bytes(
            manifest
                .files
                .iter()
                .find(|entry| entry.rel_path == "b.baml")
                .expect("b entry")
                .unit_key,
        );
        let hex = missing_key.hex();
        let missing_path = ctx1
            .cache
            .dir()
            .join("bytecode")
            .join(&hex[..2])
            .join(format!("{hex}.bexc"));
        std::fs::remove_file(missing_path).expect("remove b unit");

        let r = resolved(&root, &edited);
        let mut db2 = crate::project_load::build_db_from_sources(&r, |_| {});
        let ctx2 = CacheContext::open(&r, false).expect("cache reopens");
        let pending = ctx2.plan_reuse(&db2).expect("partial reuse survives");
        let dirty = dirty_basenames(&pending.dirty_files, &db2);
        assert_eq!(
            dirty,
            HashSet::from(["b.baml".to_string(), "c.baml".to_string()])
        );
        assert_eq!(pending.clean_files, HashSet::from(["a.baml".to_string()]));

        let plan = prepare_reuse_plan(&mut db2, Some(pending)).expect("reuse plan");
        let _ = baml_db::baml_compiler2_emit::take_lowered_files();
        let relinked =
            compile_program(&db2, &opts(), Some(&ctx2), Some(&plan)).expect("incremental compile");
        let mut lowered = baml_db::baml_compiler2_emit::take_lowered_files();
        lowered.sort();
        assert_eq!(lowered, vec!["b.baml".to_string(), "c.baml".to_string()]);

        let honest_db = crate::project_load::build_db_from_sources(&r, |_| {});
        let full = compile_program(&honest_db, &opts(), Some(&ctx2), None).expect("full compile");
        assert_eq!(
            borsh::to_vec(&relinked).expect("serialize relink"),
            borsh::to_vec(&full).expect("serialize full")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn warm_body_edit_writes_one_unit_and_one_manifest() {
        if cache_disabled() {
            return;
        }
        let initial = [
            ("a.baml", "function a() -> int {\n  1\n}\n"),
            ("b.baml", "function b() -> int {\n  2\n}\n"),
            ("c.baml", "function c() -> int {\n  3\n}\n"),
        ];
        let edited = [
            ("a.baml", "function a() -> int {\n  10 + 1\n}\n"),
            ("b.baml", "function b() -> int {\n  2\n}\n"),
            ("c.baml", "function c() -> int {\n  3\n}\n"),
        ];
        let root = bc_root();

        let r1 = resolved(&root, &initial);
        let db1 = crate::project_load::build_db_from_sources(&r1, |_| {});
        let ctx1 = CacheContext::open(&r1, false).expect("cache opens");
        let program1 = compile_program(&db1, &opts(), Some(&ctx1), None).expect("compile");
        let fresh1 = ctx1
            .collect_diagnostics_incremental(&db1, None)
            .fresh_by_file;
        let cold_stats = ctx1
            .store_with_manifest(&db1, &program1, &fresh1, None)
            .expect("cold store");
        assert_eq!(cold_stats.unit_entries_written, 3);
        assert_eq!(cold_stats.manifest_entries_written, 1);

        let r2 = resolved(&root, &edited);
        let mut db2 = crate::project_load::build_db_from_sources(&r2, |_| {});
        let ctx2 = CacheContext::open(&r2, false).expect("cache reopens");
        let pending = ctx2.plan_reuse(&db2);
        let plan = prepare_reuse_plan(&mut db2, pending).expect("reuse plan");
        assert_eq!(plan.dirty_files.len(), 1);
        let fresh2 = ctx2
            .collect_diagnostics_incremental(&db2, Some(&plan))
            .fresh_by_file;
        let compiled =
            compile_program_artifacts(&db2, &opts(), Some(&ctx2), Some(&plan)).expect("compile");
        let stats = ctx2
            .store_artifacts_with_manifest(
                &db2,
                &compiled.program,
                compiled.units.as_deref(),
                &fresh2,
                Some(&plan),
            )
            .expect("warm store");
        assert_eq!(stats.unit_entries_written, 1);
        assert_eq!(stats.manifest_entries_written, 1);
        let _ = std::fs::remove_dir_all(root);
    }

    // B-694: stdlib typed-interface cache ("export data").

    #[test]
    fn stdlib_interface_is_deterministic_across_fresh_dbs() {
        // The stdlib is a compiler-build constant, so its per-package
        // `PackageInterface` must serialize byte-identically from two
        // independently-built fresh databases — the soundness foundation for
        // keying the blob by compiler fingerprint alone.
        let db1 = build_db(&[("a.baml", "function f() -> int {\n  1\n}\n")]);
        let db2 = build_db(&[("a.baml", "function f() -> int {\n  1\n}\n")]);
        let blob1 = extract_stdlib_interface(&db1);
        let blob2 = extract_stdlib_interface(&db2);
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
        let cold = build_db(&[("a.baml", "function f() -> int {\n  1\n}\n")]);
        let cold_blob = extract_stdlib_interface(&cold);

        let mut warm = build_db(&[("a.baml", "function f() -> int {\n  1\n}\n")]);
        warm.set_seeded_stdlib_interface(cold_blob.clone());
        let warm_blob = extract_stdlib_interface(&warm);
        assert_eq!(
            cold_blob, warm_blob,
            "a seeded stdlib interface must reproduce the honest derivation exactly"
        );
    }

    #[test]
    fn seeded_stdlib_interface_short_circuits_derivation() {
        use baml_db::{
            Name,
            baml_compiler2_hir::package::PackageId,
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
        let mut db = build_db(&[("a.baml", "function f() -> int {\n  1\n}\n")]);

        let sentinel = PackageInterface {
            types: Default::default(),
            functions: Default::default(),
            throw_sets: FunctionThrowSets {
                direct: Default::default(),
                transitive: Default::default(),
            },
            namespaces: Default::default(),
            impls: Default::default(),
        };
        let mut seed = std::collections::BTreeMap::new();
        seed.insert(
            "log".to_string(),
            borsh::to_vec(&sentinel).expect("serialize sentinel"),
        );
        db.set_seeded_stdlib_interface(seed);

        let log_id = PackageId::new(&db, Name::new("log"));
        let iface = package_interface(&db, log_id);
        assert!(
            iface.functions.is_empty() && iface.types.is_empty(),
            "seeded (empty) interface must be returned verbatim, not re-derived"
        );

        // A package that was NOT seeded still derives honestly and is non-empty.
        let baml_id = PackageId::new(&db, Name::new("baml"));
        let baml_iface = package_interface(&db, baml_id);
        assert!(
            !baml_iface.functions.is_empty() || !baml_iface.types.is_empty(),
            "an unseeded stdlib package must still derive its real interface"
        );
    }

    // ── Engine-boot floor: `baml test --list` discovery cache ────────────────

    fn sample_discovery() -> TestDiscovery {
        TestDiscovery {
            legacy: vec![
                CachedLegacyTest {
                    function_name: "Greet".to_string(),
                    test_name: "hello".to_string(),
                    canonical_id: "root.Greet::hello".to_string(),
                    file_path: "greet.baml".to_string(),
                },
                CachedLegacyTest {
                    function_name: "Greet".to_string(),
                    test_name: "world".to_string(),
                    canonical_id: "root.Greet::world".to_string(),
                    file_path: "greet.baml".to_string(),
                },
            ],
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
        let r = resolved(&root, &[("a.baml", "function f() -> int {\n  1\n}\n")]);
        let ctx = CacheContext::open(&r, true).expect("cache opens");

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
        let r = resolved(&root, &[("a.baml", "function f() -> int {\n  1\n}\n")]);
        let ctx = CacheContext::open(&r, true).expect("cache opens");

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
    fn compare_test_discovery_bails_on_legacy_mismatch() {
        // A drifted legacy list is a hard error (the `gocacheverify` signal).
        let cached = sample_discovery();
        let mut honest = sample_discovery();
        honest.legacy.pop();
        let err = CacheContext::compare_test_discovery(&cached, &honest)
            .expect_err("a legacy-list mismatch must bail");
        assert!(
            err.to_string().contains("legacy-test discovery differs"),
            "the bail message must name the legacy divergence: {err}"
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
        let clean = ["a.baml", "b.baml", "c.baml", "d.baml"];
        // byte0 low 5 bits == 0 → sampled at the default gate; bytes 1..5 == 2
        // (little-endian) → index 2 of the 4-file sorted set.
        let mut sampled_key = [0u8; 32];
        sampled_key[1] = 2;
        // byte0 low bits non-zero → NOT sampled at the default gate.
        let mut unsampled_key = [0u8; 32];
        unsampled_key[0] = 1;

        assert_eq!(
            sampled_pick_from_key(&sampled_key, &clean, None),
            Some("c.baml"),
            "a gated key samples; bytes 1..5 select the sorted index"
        );
        assert_eq!(
            sampled_pick_from_key(&unsampled_key, &clean, None),
            None,
            "a non-gated key must NOT sample at the default 1/32 gate"
        );
        // Pure function of the key: identical inputs → identical pick.
        assert_eq!(
            sampled_pick_from_key(&sampled_key, &clean, None),
            sampled_pick_from_key(&sampled_key, &clean, None),
            "the sampling decision must be deterministic for a given key"
        );
        // Force overrides the gate both ways (the =1 / =0 knobs).
        assert_eq!(
            sampled_pick_from_key(&unsampled_key, &clean, Some(true)),
            Some("a.baml"),
            "force=Some(true) samples even a non-gated key"
        );
        assert_eq!(
            sampled_pick_from_key(&sampled_key, &clean, Some(false)),
            None,
            "force=Some(false) disables even a gated key"
        );
        // A different key re-rolls the index → sampling varies across states.
        let mut other = sampled_key;
        other[1] = 3;
        assert_ne!(
            sampled_pick_from_key(&sampled_key, &clean, Some(true)),
            sampled_pick_from_key(&other, &clean, Some(true)),
            "a different key must be able to select a different file"
        );
        // Nothing served → nothing to sample.
        assert_eq!(sampled_pick_from_key(&sampled_key, &[], Some(true)), None);
    }

    /// Compile+store a project, reopen it byte-identically, and return the
    /// reopened context, its all-clean reuse plan, and a FRESH un-seeded honest
    /// database — the three inputs `verify_sampled_artifact` compares. `None`
    /// when the on-disk cache is disabled by env.
    fn sampled_setup(
        files: &[(&str, &str)],
    ) -> Option<(PathBuf, CacheContext, ReusePlan, ProjectDatabase)> {
        if cache_disabled() {
            return None;
        }
        let root = bc_root();
        let _ = compile_and_store_v1(&root, files);

        let r = resolved(&root, files);
        let db2 = crate::project_load::build_db_from_sources(&r, |_| {});
        let ctx2 = CacheContext::open(&r, false).expect("cache reopens");
        let plan = ctx2.plan_reuse(&db2).expect("all-clean reuse plan");
        // The oracle DB must be fresh and un-seeded (no `prepare_reuse_plan`),
        // else the honest re-derivation would return the served seed verbatim.
        let honest = crate::project_load::build_db_from_sources(&r, |_| {});
        Some((root, ctx2, plan, honest))
    }

    #[test]
    fn verify_sampled_artifact_passes_on_faithful_cache() {
        let files = [
            ("a.baml", "function a() -> int {\n  1\n}\n"),
            ("b.baml", "function b() -> int {\n  2\n}\n"),
        ];
        let Some((root, ctx, plan, honest)) = sampled_setup(&files) else {
            return;
        };
        // A byte-identical reopen serves every file faithfully — both the
        // diagnostics blob and the throws fragment must verify clean.
        for rel in ["a.baml", "b.baml"] {
            ctx.verify_sampled_artifact(&honest, &plan, rel)
                .unwrap_or_else(|e| panic!("faithful cache must pass for {rel}: {e}"));
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn verify_sampled_artifact_bails_on_stale_diagnostics() {
        let files = [
            ("a.baml", "function a() -> int {\n  1\n}\n"),
            ("b.baml", "function b() -> int {\n  2\n}\n"),
        ];
        let Some((root, ctx, mut plan, honest)) = sampled_setup(&files) else {
            return;
        };
        // Replace a.baml's served (empty) diagnostics blob with one fabricating
        // an error the honest check never produces: served != honest → bail.
        plan.clean_diagnostics.insert(
            "a.baml".to_string(),
            crate::diagnostics_cache::one_fake_diagnostic_blob("a.baml"),
        );
        let err = ctx
            .verify_sampled_artifact(&honest, &plan, "a.baml")
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
        let files = [
            ("a.baml", "function a() -> int {\n  1\n}\n"),
            ("b.baml", "function b() -> int {\n  2\n}\n"),
        ];
        let Some((root, ctx, mut plan, honest)) = sampled_setup(&files) else {
            return;
        };
        // Corrupt a.baml's served callable-throws fragment (the manifest-
        // resident copy the seeds project from); honest bytes differ.
        let fragment = plan
            .clean_fragments
            .get_mut("a.baml")
            .expect("a.baml fragment present in the reuse plan");
        assert!(
            !fragment.is_empty(),
            "a plain function must carry a non-empty fragment for this test to bite"
        );
        *fragment = vec![0xde, 0xad, 0xbe, 0xef];
        let err = ctx
            .verify_sampled_artifact(&honest, &plan, "a.baml")
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
        // A whole-image cache hit passes `plan = None`: nothing is served
        // artifact-by-artifact, so the honest DB must never be built (sampling a
        // hit would need a full honest compile — the design forbids it).
        let Some((root, ctx, _plan, _honest)) =
            sampled_setup(&[("a.baml", "function a() -> int {\n  1\n}\n")])
        else {
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
        let builtin = baml_compiler2_hir::compiler2_all_files(db)
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
        let r = resolved(&root, &[("a.baml", "function f() -> int {\n  1\n}\n")]);
        let db = crate::project_load::build_db_from_sources(&r, |_| {});
        let ctx = CacheContext::open(&r, false).expect("cache opens");

        let honest = baml_project::collect_compiler2_diagnostics(&db);

        // No blob yet: the honest builtin check runs; merged equals honest.
        assert!(
            ctx.load_stdlib_diagnostics().is_none(),
            "no blob on a cold cache"
        );
        let before = ctx.collect_diagnostics_incremental(&db, None).merged;
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
        let after = ctx.collect_diagnostics_incremental(&db, None).merged;
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
        let db = build_db(&[("a.baml", "function f() -> int {\n  1\n}\n")]);
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
        let db = build_db(&[("a.baml", "function f() -> int {\n  1\n}\n")]);
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
        let db = build_db(&[("a.baml", "function f() -> int {\n  1\n}\n")]);
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
        let db = build_db(&[("a.baml", "function f() -> int {\n  1\n}\n")]);
        let honest = vec![fabricate_builtin_diag(&db)];
        assert!(
            CacheContext::compare_stdlib_diagnostics(&db, b"not-a-valid-blob", &honest).is_ok(),
            "an undecodable blob degrades to the honest check, not a verify bail"
        );
    }
}
