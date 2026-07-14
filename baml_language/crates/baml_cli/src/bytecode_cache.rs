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
//!   diagnostics oracles.
//! - `BAML_NO_DIAGNOSTICS_CACHE=1` — check every file instead of serving clean
//!   files from the per-file diagnostics cache (reuse / throws-seed unaffected),
//!   to isolate that feature's win.
//! - `BAML_NO_CALLABLE_THROWS_CACHE=1` — empty the per-function `callable_throws`
//!   seed so every function infers its throws honestly (reuse / diagnostics
//!   serving unaffected), to isolate the Phase 2 fragment seed's win.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use baml_db::{
    SourceFile,
    baml_compiler2_emit::{
        CompileOptions, LoweringError, OptLevel, decompose_units, generate_project_bytecode,
        generate_project_bytecode_with_reuse_artifacts, generate_project_bytecode_with_stdlib,
        generate_stdlib_program, reuse_throws_mismatches,
    },
    baml_compiler2_hir,
};
use baml_project::ProjectDatabase;
use bex_cache::{
    BytecodeCache, CacheKey, KeyInputs, ManifestFile, ProjectManifest, compiler_fingerprint,
    compute_key, image_key, manifest_key, stdlib_interface_key,
};
use bex_vm_types::{CompilationUnit, LinkableImage, Object, Program, relink};
use sha2::{Digest, Sha256};

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
    /// Symbolic [`LinkableImage`] accompanying the Program blob (B-693 Stage 3),
    /// derived from `key` — the source of `prev_units` for the next compile.
    image_key: CacheKey,
    /// Precompiled stdlib slice, keyed by compiler build + opt level only.
    stdlib_key: CacheKey,
    /// Cached stdlib typed-interface blob (B-694), keyed by compiler build +
    /// opt level only — like `stdlib_key`, the stdlib is a build constant.
    stdlib_interface_key: CacheKey,
    /// Latest-compile manifest, fixed per (project root, options, build).
    manifest_key: CacheKey,
    /// Whether test cases are emitted — needed to decompose the compiled
    /// Program back into units for the image blob.
    emit_test_cases: bool,
}

impl CacheContext {
    /// `None` when caching is disabled via `BAML_NO_BYTECODE_CACHE=1`.
    pub(crate) fn open(resolved: &ResolvedProject, emit_test_cases: bool) -> Option<Self> {
        if std::env::var_os("BAML_NO_BYTECODE_CACHE").is_some_and(|v| v == "1") {
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
            image_key: image_key(key.as_bytes()),
            stdlib_key: bex_cache::stdlib_key(&fingerprint, CLI_OPT_LEVEL as u8),
            stdlib_interface_key: stdlib_interface_key(&fingerprint, CLI_OPT_LEVEL as u8),
            manifest_key: manifest_key(
                &fingerprint,
                CLI_OPT_LEVEL as u8,
                emit_test_cases,
                &resolved.root,
                resolved.manifest.as_deref(),
            ),
            emit_test_cases,
        })
    }

    /// Tripwire mode: force a real compile even on a hit, then byte-compare.
    pub(crate) fn verify_enabled() -> bool {
        std::env::var_os("BAML_CACHE_VERIFY").is_some_and(|v| v == "1")
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
        std::env::var_os("BAML_NO_STDLIB_INTERFACE_CACHE").is_some_and(|v| v == "1")
    }

    /// Isolation toggle for measuring the per-file diagnostics cache's win:
    /// `BAML_NO_DIAGNOSTICS_CACHE=1` drops the serve plan so every file is
    /// re-checked (reuse / throws-seed still active), leaving `plan_reuse`
    /// otherwise intact. `BAML_NO_BYTECODE_CACHE` already disables the whole
    /// cache, so this is the finer-grained knob.
    fn diagnostics_cache_disabled() -> bool {
        std::env::var_os("BAML_NO_DIAGNOSTICS_CACHE").is_some_and(|v| v == "1")
    }

    /// Isolation toggle for measuring the `callable_throws` seed's win:
    /// `BAML_NO_CALLABLE_THROWS_CACHE=1` empties `plan.seeded_callable_throws`
    /// so every function infers its throws honestly (the last cold
    /// `infer_scope_types` pull a dirty file forces on its clean callees),
    /// leaving reuse / diagnostics serving intact. `BAML_NO_BYTECODE_CACHE`
    /// already disables the whole cache, so this is the finer-grained knob.
    fn callable_throws_cache_disabled() -> bool {
        std::env::var_os("BAML_NO_CALLABLE_THROWS_CACHE").is_some_and(|v| v == "1")
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
        let bytes = self.cache.load_raw(&self.stdlib_interface_key)?;
        match borsh::from_slice::<std::collections::BTreeMap<String, Vec<u8>>>(&bytes) {
            Ok(map) => Some(map),
            Err(e) => {
                cache_debug(format_args!("stdlib interface undecodable: {e}"));
                None
            }
        }
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
        match borsh::to_vec(&blob) {
            Ok(payload) => {
                if let Err(e) = self.cache.store_raw(&self.stdlib_interface_key, &payload) {
                    cache_debug(format_args!("stdlib interface store failed: {e}"));
                }
            }
            Err(e) => cache_debug(format_args!("stdlib interface serialize failed: {e}")),
        }
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
        let bytes = self.cache.load_raw(&self.stdlib_interface_key)?;
        borsh::from_slice::<std::collections::BTreeMap<String, Vec<u8>>>(&bytes).ok()
    }
}

/// Derive and borsh-serialize each stdlib package's `PackageInterface` from a
/// compiled database, keyed by package name. On a warm database the query
/// returns the seed verbatim, so re-serializing reproduces the same bytes
/// (idempotent); on a cold database it materializes the interface once.
fn extract_stdlib_interface(db: &ProjectDatabase) -> std::collections::BTreeMap<String, Vec<u8>> {
    use baml_db::{
        Name, baml_compiler2_hir::package::PackageId,
        baml_compiler2_tir::package_interface::package_interface,
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
    pub(crate) image: Option<LinkableImage>,
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
            image: None,
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
        match generate_project_bytecode_with_reuse_artifacts(
            db,
            options,
            CLI_OPT_LEVEL,
            &base,
            &plan.prev_units,
            &plan.clean_files,
        ) {
            Ok((program, units)) => {
                return Ok(CompiledArtifacts {
                    program,
                    image: Some(LinkableImage { units }),
                });
            }
            // A real compile error must surface — it is not a reuse problem.
            Err(err @ LoweringError::ProjectHasErrors { .. }) => return Err(err),
            // A corrupt/incompatible previous image or an unrelocatable
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
            image: None,
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
    let mismatches = reuse_throws_mismatches(db, &plan.prev_units, &plan.clean_files);
    if mismatches.is_empty() {
        db.set_seeded_callable_throws(callable_seeds);
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

/// Cache diagnostics to stderr, gated on `BAML_CACHE_DEBUG=1`. For support
/// and perf triage: shows plan sizes, fallback reasons, and store failures
/// without affecting normal output.
#[allow(clippy::print_stderr)] // opt-in debug channel (BAML_CACHE_DEBUG=1)
pub(crate) fn cache_debug(args: std::fmt::Arguments<'_>) {
    if std::env::var_os("BAML_CACHE_DEBUG").is_some_and(|v| v == "1") {
        eprintln!("[baml-cache] {args}");
    }
}

/// Last path segment of a dotted fq name (`user.ns.foo` → `foo`).
fn last_segment(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn content_hash(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

/// Last-segment names of every item `file` defines, from the HIR item tree.
fn defined_names(db: &ProjectDatabase, file: SourceFile) -> Vec<String> {
    use baml_compiler2_hir::{
        contributions::Definition,
        loc::{ClassLoc, EnumLoc, FunctionLoc, InterfaceLoc, LetLoc, TypeAliasLoc},
    };
    use baml_db::baml_compiler2_mir::def_to_item_ref;
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let mut names: Vec<String> = Vec::new();
    for local_id in item_tree.functions.keys() {
        let fq = def_to_item_ref(
            db,
            Definition::Function(FunctionLoc::new(db, file, *local_id)),
        );
        names.push(last_segment(&fq.to_string()).to_string());
    }
    for local_id in item_tree.lets.keys() {
        let fq = def_to_item_ref(db, Definition::Let(LetLoc::new(db, file, *local_id)));
        names.push(last_segment(&fq.to_string()).to_string());
    }
    for local_id in item_tree.classes.keys() {
        let fq = def_to_item_ref(db, Definition::Class(ClassLoc::new(db, file, *local_id)));
        names.push(last_segment(&fq.to_string()).to_string());
    }
    for local_id in item_tree.enums.keys() {
        let fq = def_to_item_ref(db, Definition::Enum(EnumLoc::new(db, file, *local_id)));
        names.push(last_segment(&fq.to_string()).to_string());
    }
    for local_id in item_tree.interfaces.keys() {
        let fq = def_to_item_ref(
            db,
            Definition::Interface(InterfaceLoc::new(db, file, *local_id)),
        );
        names.push(last_segment(&fq.to_string()).to_string());
    }
    // Type aliases are erased into their consumers (a non-recursive alias is
    // expanded inline at every use), so an alias whose RHS changes must reach
    // the change-propagation set by *name*. Omitting them (the original bug)
    // left an alias edit invisible: consumers that named the alias spliced the
    // stale expansion. `def_to_item_ref` handles `TypeAlias` like any other
    // named item.
    for local_id in item_tree.type_aliases.keys() {
        let fq = def_to_item_ref(
            db,
            Definition::TypeAlias(TypeAliasLoc::new(db, file, *local_id)),
        );
        names.push(last_segment(&fq.to_string()).to_string());
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
fn syntactic_type_names(db: &ProjectDatabase, file: SourceFile) -> HashSet<String> {
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let mut names: HashSet<String> = HashSet::new();

    for func in item_tree.functions.values() {
        for te in func.params.iter().filter_map(|p| p.type_expr.as_ref()) {
            add_type_display(te, &mut names);
        }
        if let Some(te) = &func.return_type {
            add_type_display(te, &mut names);
        }
        if let Some(te) = &func.throws {
            add_type_display(te, &mut names);
        }
        for te in func.generic_param_bounds.iter().flatten() {
            add_type_display(te, &mut names);
        }
    }
    for ts in item_tree.template_strings.values() {
        for te in ts.params.iter().filter_map(|p| p.type_expr.as_ref()) {
            add_type_display(te, &mut names);
        }
    }
    for class in item_tree.classes.values() {
        for te in class.fields.iter().filter_map(|f| f.type_expr.as_ref()) {
            add_type_display(te, &mut names);
        }
        for te in class.generic_param_bounds.iter().flatten() {
            add_type_display(te, &mut names);
        }
        for block in &class.implements {
            add_type_display(&block.target, &mut names);
        }
    }
    for iface in item_tree.interfaces.values() {
        for te in iface.fields.iter().filter_map(|f| f.type_expr.as_ref()) {
            add_type_display(te, &mut names);
        }
        for te in &iface.requires {
            add_type_display(te, &mut names);
        }
        for te in iface.generic_param_bounds.iter().flatten() {
            add_type_display(te, &mut names);
        }
        for method in &iface.required_methods {
            for te in method.params.iter().filter_map(|p| p.type_expr.as_ref()) {
                add_type_display(te, &mut names);
            }
            if let Some(te) = &method.return_type {
                add_type_display(te, &mut names);
            }
            if let Some(te) = &method.throws {
                add_type_display(te, &mut names);
            }
            for te in method.generic_param_bounds.iter().flatten() {
                add_type_display(te, &mut names);
            }
        }
        for assoc in &iface.associated_types {
            if let Some(te) = &assoc.bound {
                add_type_display(te, &mut names);
            }
            if let Some(te) = &assoc.default {
                add_type_display(te, &mut names);
            }
        }
    }
    for alias in item_tree.type_aliases.values() {
        if let Some(te) = &alias.type_expr {
            add_type_display(te, &mut names);
        }
    }
    for imp in item_tree.impls.values() {
        add_type_display(&imp.interface_target, &mut names);
        // Out-of-body (`Free`) impls carry an explicit for-target `TypeExpr`; an
        // in-class impl's for-target is the class itself (no `TypeExpr` to
        // display). `names` is a set, so the interface_target added above is not
        // double-counted for free impls.
        if let baml_compiler2_hir::item_tree::ImplSubject::Free { for_target, .. } = &imp.subject {
            add_type_display(for_target, &mut names);
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
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    !item_tree.classes.is_empty()
        || !item_tree.enums.is_empty()
        || !item_tree.interfaces.is_empty()
        || !item_tree.type_aliases.is_empty()
}

/// Whether `file` declares any interface-`impl` construct — an `impl` block, an
/// out-of-body `implements … for …`, or a class `implements` block. Gates the
/// `IMPL_SENTINEL`: only a change to such a file can move the package's impl set
/// (and thus a coherence verdict), so an impl-free edit never trips the fallback.
fn file_has_impl_construct(db: &ProjectDatabase, file: SourceFile) -> bool {
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    // The unified `impls` map holds both in-class and out-of-body impl blocks, so
    // it subsumes the removed flat `implements_for` view; a class `implements`
    // block is a distinct construct still worth a separate check.
    !item_tree.impls.is_empty() || item_tree.classes.values().any(|c| !c.implements.is_empty())
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

/// User source files with their root-relative paths (skipping legacy
/// `<builtin>/` entries the v1 compiler still sees).
fn user_files_with_rel_paths(db: &ProjectDatabase) -> Vec<(SourceFile, String)> {
    let Some(project) = db.get_project() else {
        return Vec::new();
    };
    let root = project.root(db);
    db.get_source_files()
        .into_iter()
        .filter_map(|sf| {
            let path = sf.path(db);
            if path.to_string_lossy().starts_with("<builtin>/") {
                return None;
            }
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();
            Some((sf, rel))
        })
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
/// The throws-taint closure is the Phase 2 upgrade over the one-hop throws
/// propagation. `callable_throws` is transitive over the call graph, so a throws
/// change can flow through a *content-clean intermediary* — whose own
/// `file_throw_facts` are byte-identical, only its solved transitive throws grew
/// — to a caller the one-hop pass never reaches. The closure seeds a worklist
/// with the last-segment names of every function whose facts changed (or was
/// added/removed), then walks reverse call edges — over-approximated by
/// `referenced_names` — marking each referencing file dirty and re-tainting its
/// own functions, stopping at closed-`throws` contracts (whose `callable_throws`
/// is the declared set, independent of callees). It strictly subsumes the
/// one-hop version (direct callers ⊂ transitive callers) and over-dirties only,
/// so a seeded function's body and transitive throw contributors are always
/// stable — the invariant the `callable_throws` seed rests on.
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
                let fresh = baml_db::baml_compiler2_tir::throw_inference::file_throw_facts(db, *sf);
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
                let fresh = baml_db::baml_compiler2_tir::throw_inference::file_throw_facts(db, *sf);
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
    }
}

/// Project each clean file's cached interface fragment into a per-function
/// `callable_throws` seed map, keyed by full source path then by item-tree
/// `LocalItemId::as_u32` (the fragment's key form). A unit whose fragment is
/// empty or fails to decode is skipped — its functions then infer honestly
/// (degrade, never miscompile). Empty under `BAML_NO_CALLABLE_THROWS_CACHE=1`.
fn project_callable_throws_seeds(
    prev_units: &[CompilationUnit],
    clean_files: &HashSet<String>,
    root: Option<&PathBuf>,
) -> std::collections::BTreeMap<String, std::collections::BTreeMap<u32, baml_type::Ty>> {
    if CacheContext::callable_throws_cache_disabled() {
        return std::collections::BTreeMap::new();
    }
    use baml_db::baml_compiler2_tir::package_interface::CallableThrowsFragment;
    let mut by_path = std::collections::BTreeMap::new();
    for unit in prev_units {
        if !clean_files.contains(&unit.source_file) || unit.callable_throws_fragment.is_empty() {
            continue;
        }
        let fragment: CallableThrowsFragment =
            match borsh::from_slice(&unit.callable_throws_fragment) {
                Ok(f) => f,
                Err(e) => {
                    cache_debug(format_args!(
                        "interface fragment for `{}` undecodable: {e}",
                        unit.source_file
                    ));
                    continue;
                }
            };
        if fragment.by_id.is_empty() {
            continue;
        }
        let full = root
            .map(|r| r.join(&unit.source_file).display().to_string())
            .unwrap_or_else(|| unit.source_file.clone());
        by_path.insert(full, fragment.by_id);
    }
    by_path
}

impl CacheContext {
    /// Decide which files can reuse their previously compiled bytecode.
    ///
    /// `None` means no reuse is possible (first compile, compiler changed,
    /// previous blob trimmed, or everything is dirty) — callers take the
    /// stdlib-splice path and gate diagnostics on all files.
    ///
    /// The clean/dirty partition (with the throws-taint closure) is computed by
    /// [`compute_dirty_partition`]; this method loads the manifest and previous
    /// image, then attaches the reuse seeds (throw facts, `callable_throws`
    /// fragments) and clean-file diagnostics blobs for the clean set.
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
        // Load the previous compile's symbolic image (B-693 Stage 3), keyed off
        // the Program blob's key. A missing / undecodable image means no reuse.
        let Some(image_bytes) = self.cache.load_raw(&image_key(&manifest.program_key)) else {
            cache_debug(format_args!("previous image missing — full compile"));
            return None;
        };
        let Ok(prev_image) = borsh::from_slice::<LinkableImage>(&image_bytes) else {
            cache_debug(format_args!("previous image undecodable — full compile"));
            return None;
        };
        let prev_units = prev_image.units;

        let partition = compute_dirty_partition(db, &manifest);
        if partition.clean_files.is_empty() {
            cache_debug(format_args!("all files dirty — full compile"));
            return None;
        }
        let DirtyPartition {
            clean_files,
            dirty_files,
            fresh_throw_facts,
        } = partition;
        cache_debug(format_args!(
            "reuse plan: {} clean, {} dirty",
            clean_files.len(),
            dirty_files.len()
        ));

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

        // Project clean files' cached interface fragments into a per-function
        // `callable_throws` seed (Phase 2): a clean function's throws — and hence
        // any dirty caller's throws-dependent inference over it — are served
        // without walking its body. The throws-taint closure guarantees a seeded
        // function's transitive throw contributors are all unchanged.
        let seeded_callable_throws =
            project_callable_throws_seeds(&prev_units, &clean_files, root.as_ref());

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
            seeded_throw_facts,
            seeded_callable_throws,
            clean_diagnostics,
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
    ) -> std::io::Result<()> {
        self.store_artifacts_with_manifest(db, program, None, fresh_by_file, plan)
    }

    pub(crate) fn store_artifacts_with_manifest(
        &self,
        db: &ProjectDatabase,
        program: &Program,
        image: Option<&LinkableImage>,
        fresh_by_file: &std::collections::BTreeMap<String, Vec<u8>>,
        plan: Option<&ReusePlan>,
    ) -> std::io::Result<()> {
        self.store(program)?;

        // Store the symbolic image alongside the Program blob (B-693 Stage 3) so
        // the next incremental compile can reuse clean files' units. Best-effort:
        // if the decomposition fails, the next compile simply full-compiles.
        let store_image = |image: &LinkableImage| match borsh::to_vec(image) {
            Ok(payload) => {
                if let Err(e) = self.cache.store_raw(&self.image_key, &payload) {
                    cache_debug(format_args!("image store failed: {e}"));
                }
            }
            Err(e) => cache_debug(format_args!("image serialize failed: {e}")),
        };
        if let Some(image) = image {
            store_image(image);
        } else {
            let options = CompileOptions {
                emit_test_cases: self.emit_test_cases,
            };
            match decompose_units(db, &options, program) {
                Ok(units) => store_image(&LinkableImage { units }),
                Err(e) => cache_debug(format_args!("image decompose failed: {e}")),
            }
        }

        let mut referenced = referenced_names_by_file(program);
        let mut files: Vec<ManifestFile> = user_files_with_rel_paths(db)
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
                    throw_facts: baml_db::baml_compiler2_tir::throw_inference::file_throw_facts(
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
        self.cache.store_raw(&self.manifest_key, &payload)
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
        let plan = plan.filter(|_| !Self::diagnostics_cache_disabled());
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
        if let Some(plan) = plan {
            for (rel, blob) in &plan.clean_diagnostics {
                match crate::diagnostics_cache::rehydrate_file_blob(db, &root, blob) {
                    Some(mut diags) => precomputed.append(&mut diags),
                    None => {
                        degrade.insert(rel.clone());
                    }
                }
            }
        }

        let rel_of = |sf: SourceFile| -> Option<String> {
            let path = sf.path(db);
            if path.to_string_lossy().starts_with("<builtin>/") {
                return None;
            }
            Some(
                path.strip_prefix(&root)
                    .unwrap_or(&path)
                    .display()
                    .to_string(),
            )
        };
        let should_check = |sf: SourceFile| -> bool {
            match rel_of(sf) {
                // Builtins are never in the manifest / clean set — always check.
                None => true,
                Some(rel) => match plan {
                    Some(plan) => !plan.clean_files.contains(&rel) || degrade.contains(&rel),
                    None => true,
                },
            }
        };

        let narrowed =
            baml_project::collect_compiler2_diagnostics_narrowed(db, &should_check, precomputed);

        let mut fresh_by_file =
            crate::diagnostics_cache::fresh_blobs_by_file(db, &root, &narrowed.fresh);
        // Ensure every re-checked user file has an entry (empty if it produced
        // no diagnostics) so `store_with_manifest` overwrites a stale/poison
        // carry for a degraded-but-now-clean file rather than re-carrying it.
        for (sf, rel) in user_files_with_rel_paths(db) {
            if should_check(sf) {
                fresh_by_file
                    .entry(rel)
                    .or_insert_with(crate::diagnostics_cache::empty_blob);
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
        let bytes = self.cache.load_raw(&self.manifest_key)?;
        borsh::from_slice::<ProjectManifest>(&bytes).ok()
    }

    /// Load the previous compile's symbolic units for the verify oracle,
    /// bypassing the `plan_reuse` verify short-circuit.
    fn load_prev_units_for_verify(
        &self,
        manifest: &ProjectManifest,
    ) -> Option<Vec<CompilationUnit>> {
        let image_bytes = self.cache.load_raw(&image_key(&manifest.program_key))?;
        borsh::from_slice::<LinkableImage>(&image_bytes)
            .ok()
            .map(|image| image.units)
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
        let Some(prev_units) = self.load_prev_units_for_verify(&manifest) else {
            return Ok(());
        };
        let Some(root) = db.get_project().map(|p| p.root(db).clone()) else {
            return Ok(());
        };
        // Exactly the files a served warm compile would have seeded.
        let clean_files = compute_dirty_partition(db, &manifest).clean_files;

        for unit in &prev_units {
            if !clean_files.contains(&unit.source_file) || unit.callable_throws_fragment.is_empty()
            {
                continue;
            }
            let full = root.join(&unit.source_file);
            let Some(sf) = db.get_file(&full) else {
                continue; // file removed — never seeded
            };
            let honest =
                baml_db::baml_compiler2_tir::package_interface::file_callable_throws_fragment(
                    db, sf,
                );
            let honest_bytes = borsh::to_vec(honest).map_err(|e| {
                anyhow::anyhow!(
                    "honest interface fragment for `{}` failed to serialize: {e}",
                    unit.source_file
                )
            })?;
            if honest_bytes != unit.callable_throws_fragment {
                anyhow::bail!(
                    "BAML_CACHE_VERIFY: cached interface fragment for `{}` differs from a fresh \
                     derivation ({} cached vs {} fresh bytes). A clean file's stored fragment is \
                     a stale substitute — the throws-taint closure failed to dirty a file whose \
                     `callable_throws` changed, so the seeded value would be \
                     wrong. Please report this.",
                    unit.source_file,
                    unit.callable_throws_fragment.len(),
                    honest_bytes.len(),
                );
            }
        }
        Ok(())
    }

    /// Test hook: overwrite one file's cached diagnostics with an empty blob,
    /// simulating a stale cache that dropped a diagnostic. Used by the verify
    /// oracle's negative test.
    #[cfg(test)]
    pub(crate) fn poison_manifest_diagnostics_for_test(&self, rel_path: &str) {
        let bytes = self
            .cache
            .load_raw(&self.manifest_key)
            .expect("manifest present");
        let mut manifest: ProjectManifest = borsh::from_slice(&bytes).expect("manifest decodes");
        for f in &mut manifest.files {
            if f.rel_path == rel_path {
                f.diagnostics = crate::diagnostics_cache::empty_blob();
            }
        }
        let payload = borsh::to_vec(&manifest).expect("manifest serializes");
        self.cache
            .store_raw(&self.manifest_key, &payload)
            .expect("manifest re-stored");
    }

    /// Test hook: overwrite one file's stored interface fragment in the previous
    /// image, simulating a stale/corrupt fragment (as a mis-projected seed would
    /// be). Used by the fragment verify oracle's negative test.
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
        let manifest: ProjectManifest =
            borsh::from_slice(&manifest_bytes).expect("manifest decodes");
        let img_key = image_key(&manifest.program_key);
        let image_bytes = self.cache.load_raw(&img_key).expect("image present");
        let mut image: LinkableImage = borsh::from_slice(&image_bytes).expect("image decodes");
        for unit in &mut image.units {
            if unit.source_file == rel_path {
                unit.callable_throws_fragment = fragment.clone();
            }
        }
        let payload = borsh::to_vec(&image).expect("image serializes");
        self.cache
            .store_raw(&img_key, &payload)
            .expect("image re-stored");
    }
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

    use std::{
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn opts() -> CompileOptions {
        CompileOptions {
            emit_test_cases: false,
        }
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

    fn cache_disabled() -> bool {
        std::env::var_os("BAML_NO_BYTECODE_CACHE").is_some()
            || std::env::var_os("BAML_CACHE_VERIFY").is_some()
    }

    fn unique_root() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("baml-bc-cache-test-{}-{n}", std::process::id()))
    }

    fn resolved(root: &Path, files: &[(&str, &str)]) -> ResolvedProject {
        ResolvedProject {
            root: root.to_path_buf(),
            manifest: None,
            files: files
                .iter()
                .map(|(name, content)| (root.join(name), (*content).to_string()))
                .collect(),
        }
    }

    /// Compile+cache `initial`, then plan a reuse against `edited`; return the
    /// set of dirty file names (`None` when caching is disabled by env).
    fn dirty_after_edit(
        initial: &[(&str, &str)],
        edited: &[(&str, &str)],
    ) -> Option<HashSet<String>> {
        if cache_disabled() {
            return None;
        }
        let root = unique_root();
        let _ = std::fs::remove_dir_all(&root);

        let r1 = resolved(&root, initial);
        let db1 = crate::project_load::build_db_from_sources(&r1, |_| {});
        let ctx1 = CacheContext::open(&r1, false).expect("cache opens");
        let program1 =
            compile_program(&db1, &opts(), Some(&ctx1), None).expect("initial compile succeeds");
        let fresh1 = ctx1
            .collect_diagnostics_incremental(&db1, None)
            .fresh_by_file;
        ctx1.store_with_manifest(&db1, &program1, &fresh1, None)
            .expect("manifest stored");

        let r2 = resolved(&root, edited);
        let db2 = crate::project_load::build_db_from_sources(&r2, |_| {});
        let ctx2 = CacheContext::open(&r2, false).expect("cache reopens");
        let plan = ctx2.plan_reuse(&db2).expect("reuse plan available");

        let dirty: HashSet<String> = plan
            .dirty_files
            .iter()
            .filter_map(|sf| {
                sf.path(&db2)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .collect();
        let _ = std::fs::remove_dir_all(&root);
        Some(dirty)
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
        let root = unique_root();
        let _ = std::fs::remove_dir_all(&root);

        let r1 = resolved(&root, initial);
        let db1 = crate::project_load::build_db_from_sources(&r1, |_| {});
        let ctx1 = CacheContext::open(&r1, false).expect("cache opens");
        let program1 =
            compile_program(&db1, &opts(), Some(&ctx1), None).expect("initial compile succeeds");
        let fresh1 = ctx1
            .collect_diagnostics_incremental(&db1, None)
            .fresh_by_file;
        ctx1.store_with_manifest(&db1, &program1, &fresh1, None)
            .expect("manifest stored");

        let r2 = resolved(&root, edited);
        let db2 = crate::project_load::build_db_from_sources(&r2, |_| {});
        let ctx2 = CacheContext::open(&r2, false).expect("cache reopens");
        let plan = ctx2.plan_reuse(&db2).expect("reuse plan available");

        let dirty: HashSet<String> = plan
            .dirty_files
            .iter()
            .filter_map(|sf| {
                sf.path(&db2)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .collect();
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
        let root = unique_root();
        let _ = std::fs::remove_dir_all(&root);

        // v1: compile and store the manifest + image.
        let r1 = resolved(&root, initial);
        let db1 = crate::project_load::build_db_from_sources(&r1, |_| {});
        let ctx1 = CacheContext::open(&r1, false).expect("cache opens");
        let program1 =
            compile_program(&db1, &opts(), Some(&ctx1), None).expect("initial compile succeeds");
        let fresh1 = ctx1
            .collect_diagnostics_incremental(&db1, None)
            .fresh_by_file;
        ctx1.store_with_manifest(&db1, &program1, &fresh1, None)
            .expect("manifest stored");

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
            dirty: plan
                .dirty_files
                .iter()
                .filter_map(|sf| {
                    sf.path(&db2)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                })
                .collect(),
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
        let root = unique_root();
        let _ = std::fs::remove_dir_all(&root);

        let r1 = resolved(&root, initial);
        let db1 = crate::project_load::build_db_from_sources(&r1, |_| {});
        let ctx1 = CacheContext::open(&r1, false).expect("cache opens");
        let program1 =
            compile_program(&db1, &opts(), Some(&ctx1), None).expect("initial compile succeeds");
        let fresh1 = ctx1
            .collect_diagnostics_incremental(&db1, None)
            .fresh_by_file;
        ctx1.store_with_manifest(&db1, &program1, &fresh1, None)
            .expect("manifest stored");

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
            dirty: plan
                .dirty_files
                .iter()
                .filter_map(|sf| {
                    sf.path(&db2)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                })
                .collect(),
            clean: plan.clean_files.iter().map(|r| basename(r)).collect(),
            seeded,
        };
        let _ = std::fs::remove_dir_all(&root);
        Some((summary, diags_match, byte_identical))
    }

    // ── Phase 2 follow-up: layout-scoped sentinel (mixed class+function files) ─

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

    /// Verification bar #4: editing a class's generic-parameter list is a layout
    /// change — it must fire the sentinel (the generic-param count feeds the
    /// class object), dirtying layout-baking files, byte-identity preserved.
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
        let root = unique_root();
        let _ = std::fs::remove_dir_all(&root);

        let r1 = resolved(&root, &initial);
        let db1 = crate::project_load::build_db_from_sources(&r1, |_| {});
        let ctx1 = CacheContext::open(&r1, false).expect("cache opens");
        let program1 =
            compile_program(&db1, &opts(), Some(&ctx1), None).expect("v1 compile succeeds");
        let fresh1 = ctx1
            .collect_diagnostics_incremental(&db1, None)
            .fresh_by_file;
        ctx1.store_with_manifest(&db1, &program1, &fresh1, None)
            .expect("v1 manifest stored");

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
        use baml_db::baml_compiler2_tir::callable::callable_throws;

        let mut db = build_db(&[(
            "a.baml",
            "class MyErr {\n  msg string\n}\n\
             function f() -> int {\n  1\n}\n\
             function g() -> int {\n  throw MyErr { msg: \"x\" }\n}\n",
        )]);
        let file = file_named(&db, "a.baml");
        let (f_id, g_id) = {
            let item_tree = baml_compiler2_hir::file_item_tree(&db, file);
            let mut f_id = None;
            let mut g_id = None;
            for (lid, func) in &item_tree.functions {
                match func.name.as_str() {
                    "f" => f_id = Some(*lid),
                    "g" => g_id = Some(*lid),
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
                callable_throws(&db, f_loc).clone(),
                callable_throws(&db, g_loc).clone(),
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
            *callable_throws(&db, f_loc),
            g_throws,
            "the seed must short-circuit: f returns the path-keyed seeded value"
        );
        assert_eq!(
            *callable_throws(&db, g_loc),
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
        let root = unique_root();
        let _ = std::fs::remove_dir_all(&root);
        let files = [
            ("a.baml", "function a() -> int {\n  1\n}\n"),
            ("b.baml", "function b() -> int {\n  2\n}\n"),
        ];
        let r = resolved(&root, &files);
        let db = crate::project_load::build_db_from_sources(&r, |_| {});
        let ctx = CacheContext::open(&r, false).expect("cache opens");
        let program = compile_program(&db, &opts(), Some(&ctx), None).expect("compile succeeds");
        let fresh = ctx.collect_diagnostics_incremental(&db, None).fresh_by_file;
        ctx.store_with_manifest(&db, &program, &fresh, None)
            .expect("manifest stored");

        // A faithful fragment cache passes the oracle (all files content-clean).
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
        let root = unique_root();
        let _ = std::fs::remove_dir_all(&root);
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

        let r1 = resolved(&root, &initial);
        let db1 = crate::project_load::build_db_from_sources(&r1, |_| {});
        let ctx1 = CacheContext::open(&r1, false).expect("cache opens");
        let program1 = compile_program(&db1, &opts(), Some(&ctx1), None).expect("initial compile");
        let fresh = ctx1
            .collect_diagnostics_incremental(&db1, None)
            .fresh_by_file;
        ctx1.store_with_manifest(&db1, &program1, &fresh, None)
            .expect("manifest stored");

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
        stable_fn.throws_type = Some(baml_type::RuntimeTy::String {
            attr: baml_type::TyAttr::default(),
        });

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

    // ── B-694: stdlib typed-interface cache ("export data") ──────────────────

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
            baml_compiler2_tir::{
                package_interface::{PackageInterface, package_interface},
                throw_inference::FunctionThrowSets,
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
}
