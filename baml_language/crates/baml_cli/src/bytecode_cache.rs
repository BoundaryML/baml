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

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use baml_db::{
    SourceFile,
    baml_compiler2_emit::{
        CompileOptions, LoweringError, OptLevel, decompose_units, generate_project_bytecode,
        generate_project_bytecode_with_reuse_units, generate_project_bytecode_with_stdlib,
        generate_stdlib_program,
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

use crate::{file_signature::file_signature_hash, project_load::ResolvedProject};

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
        Name,
        baml_compiler2_hir::package::PackageId,
        baml_compiler2_tir::package_interface::{STDLIB_PACKAGE_NAMES, package_interface},
    };
    let mut out = std::collections::BTreeMap::new();
    for name in STDLIB_PACKAGE_NAMES {
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
    let Some(ctx) = cache else {
        return generate_project_bytecode(db, options);
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
        match generate_project_bytecode_with_reuse_units(
            db,
            options,
            CLI_OPT_LEVEL,
            &base,
            &plan.prev_units,
            &plan.clean_files,
        ) {
            Ok(program) => return Ok(program),
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
    generate_project_bytecode_with_stdlib(db, options, CLI_OPT_LEVEL, &base)
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
    /// Per-file throw facts for the clean files (full path → facts), to be
    /// seeded into the database before compiling so their bodies are never
    /// re-walked by throw inference.
    pub(crate) seeded_throw_facts:
        std::collections::BTreeMap<String, Vec<baml_type::throw_facts::FunctionThrowFacts>>,
    /// Clean files' opaque diagnostics blobs carried from the previous manifest,
    /// by rel_path. Rehydrated to serve those files' diagnostics without
    /// re-checking, and copied verbatim into the next manifest.
    pub(crate) clean_diagnostics: std::collections::BTreeMap<String, Vec<u8>>,
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
    }
    for imp in &item_tree.implements_for {
        add_type_display(&imp.interface_target, &mut names);
        add_type_display(&imp.for_target, &mut names);
    }
    names
}

/// Whether `file` declares any type whose *layout* another file's bytecode may
/// bake in: a class (field order), an enum (discriminants), an interface
/// (dispatch), or a type alias (inline expansion). Gates the `LAYOUT_SENTINEL`:
/// only a change to a file defining such a type can move a layout, so a
/// function-only edit never trips the conservative fallback.
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
    !item_tree.impls.is_empty()
        || !item_tree.implements_for.is_empty()
        || item_tree.classes.values().any(|c| !c.implements.is_empty())
}

/// Whether `function`'s bytecode bakes a type's layout through an operand that
/// carries no recoverable type reference: a positional field offset
/// (`LoadField`/`StoreField`/`InitField`/`InitSpread`/`InitInstance`), an enum
/// discriminant (`Discriminant`/`JumpTable`), a type-tag/union dispatch
/// (`TypeTag`/`DenseTag`/`IsType`/`LoadType`), or a virtual-dispatch slot
/// (`VirtualCall`/`VirtualCallWithRuntimeId`). Such a function must be
/// re-lowered whenever the (possibly type-inferred, hence un-named) receiver
/// type's layout changes; the name-based `referenced`/`changed` match can't see
/// these, so the file is tagged with `LAYOUT_SENTINEL` as a fallback.
///
/// NOTE: keep this in step with the layout-affecting opcodes in
/// `bex_vm_types::bytecode::Instruction`. A missed opcode *under*-approximates
/// (a correctness risk); when in doubt, include it — over-inclusion only costs
/// reuse.
fn bakes_type_layout(function: &bex_vm_types::types::Function) -> bool {
    use bex_vm_types::Instruction as I;
    function.bytecode.instructions.iter().any(|inst| {
        matches!(
            inst,
            I::LoadField(_)
                | I::StoreField(_)
                | I::InitField(_)
                | I::InitSpread(_)
                | I::InitInstance(_)
                | I::Discriminant
                | I::JumpTable(_)
                | I::TypeTag
                | I::DenseTag(_)
                | I::IsType(_)
                | I::LoadType(_)
                | I::VirtualCall { .. }
                | I::VirtualCallWithRuntimeId { .. }
        )
    })
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

    let mut by_file: HashMap<String, HashSet<String>> = HashMap::new();
    for obj in program.objects.iter() {
        let Object::Function(function) = obj else {
            continue;
        };
        if function.source_file.is_empty() || function.source_file.starts_with("<builtin>/") {
            continue;
        }
        let names = by_file.entry(function.source_file.clone()).or_default();
        // The walker needs &mut; a clone is cheap next to having compiled it.
        let mut scratch = function.clone();
        relink::visit_index_operands(&mut scratch, |operand| match operand {
            relink::IndexOperand::Global(slot) => {
                if let Some(name) = slot_names.get(&slot.raw()) {
                    names.insert(last_segment(name).to_string());
                }
            }
            relink::IndexOperand::Object(obj_idx) => {
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
        if bakes_type_layout(function) {
            names.insert(LAYOUT_SENTINEL.to_string());
        }
    }
    by_file
        .into_iter()
        .map(|(file, names)| {
            let mut names: Vec<String> = names.into_iter().collect();
            names.sort_unstable();
            (file, names)
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

impl CacheContext {
    /// Decide which files can reuse their previously compiled bytecode.
    ///
    /// `None` means no reuse is possible (first compile, compiler changed,
    /// previous blob trimmed, or everything is dirty) — callers take the
    /// stdlib-splice path and gate diagnostics on all files.
    ///
    /// Dirty = content-changed ∪ added ∪ (files whose referenced names
    /// intersect Δ), where Δ is the last-segment names defined by any file
    /// whose *signature* changed, was added, or was removed. Name matching is
    /// deliberately conservative: it catches shadowing (an added `foo`
    /// changing what existing `foo` references resolve to) and impl-method
    /// additions without needing resolution-aware dependency edges. One
    /// round suffices — signature changes only originate from actual edits,
    /// never from recompilation.
    ///
    /// A file's referenced names include both its bytecode operands and the
    /// types named in its source annotations (`syntactic_type_names`), plus a
    /// `LAYOUT_SENTINEL` when its bytecode bakes a type's layout. Δ gains the
    /// same sentinel on any type-signature change. Together these close the
    /// layout holes the bytecode-only set missed: a positional field read
    /// (`p.x`), an enum variant match, a virtual call, or an inline-expanded
    /// type alias whose defining type's layout shifts.
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

        let prev_files: HashMap<&str, &ManifestFile> = manifest
            .files
            .iter()
            .map(|f| (f.rel_path.as_str(), f))
            .collect();

        let current = user_files_with_rel_paths(db);
        let current_rels: HashSet<&str> = current.iter().map(|(_, rel)| rel.as_str()).collect();

        // Δ: names whose definitions may have changed meaning.
        let mut changed_names: HashSet<String> = HashSet::new();
        // Files needing recompilation for their own sake.
        let mut dirty: HashSet<String> = HashSet::new();
        // Did any *type* (class/enum/interface/alias) signature change? Gates
        // the `LAYOUT_SENTINEL`: a layout can only move when a type-defining
        // file's signature changes, so a function-only edit leaves it false.
        let mut type_signature_changed = false;
        // Did the package's interface-`impl` set change? Gates the
        // `IMPL_SENTINEL`: a coherence verdict can only move when an
        // impl-bearing file is added, edited, or removed.
        let mut impl_set_changed = false;

        for (sf, rel) in &current {
            match prev_files.get(rel.as_str()) {
                None => {
                    // Added file: recompiles, and its names may shadow
                    // existing references anywhere.
                    dirty.insert(rel.clone());
                    changed_names.extend(defined_names(db, *sf));
                    if file_defines_type(db, *sf) {
                        type_signature_changed = true;
                    }
                    if file_has_impl_construct(db, *sf) {
                        impl_set_changed = true;
                    }
                }
                Some(entry) => {
                    if entry.content_hash == content_hash(sf.text(db)) {
                        continue;
                    }
                    dirty.insert(rel.clone());
                    // Throws propagation (Risk 1): `throws` is body-inferred but
                    // interface-visible. A body-only edit that grows this file's
                    // inferred throws leaves its `signature_hash` unchanged, so a
                    // caller's throws-contract (E0096/E0097) and catch-exhaustiveness
                    // (E0094/E0095) diagnostics would be served stale. Compare fresh
                    // facts to the stored ones; on any change treat this file's names
                    // as changed so referencing files are re-checked. Free: a dirty
                    // file is body-walked anyway, and the query is salsa-memoized.
                    if baml_db::baml_compiler2_tir::throw_inference::file_throw_facts(db, *sf).0
                        != entry.throw_facts
                    {
                        changed_names.extend(defined_names(db, *sf));
                    }
                    if file_has_impl_construct(db, *sf) {
                        impl_set_changed = true;
                    }
                    if entry.signature_hash != file_signature_hash(db, *sf) {
                        changed_names.extend(entry.defined_names.iter().cloned());
                        changed_names.extend(defined_names(db, *sf));
                        // A type's layout may have moved. (Over-approximate: a
                        // function-only edit in a file that also defines a type
                        // trips this too — sound, only costs reuse.)
                        if file_defines_type(db, *sf) {
                            type_signature_changed = true;
                        }
                    }
                }
            }
        }
        for (rel, entry) in &prev_files {
            if !current_rels.contains(rel) {
                // Removed file: its names vanish from resolution. Its item kind
                // isn't recoverable from the manifest, so conservatively assume
                // a type (and thus a layout) may have vanished.
                changed_names.extend(entry.defined_names.iter().cloned());
                type_signature_changed = true;
                // A removed impl file (its stored refs carry the sentinel)
                // changes the package's impl set — re-check coherence peers.
                if entry.referenced_names.iter().any(|n| n == IMPL_SENTINEL) {
                    impl_set_changed = true;
                }
            }
        }
        // Conservative fallback for layout dependencies whose receiver type is
        // inferred (named in no signature): any type-layout change re-lowers
        // every file whose bytecode baked a layout (`LAYOUT_SENTINEL`).
        if type_signature_changed {
            changed_names.insert(LAYOUT_SENTINEL.to_string());
        }
        // Conservative fallback for whole-package coherence: any impl-set change
        // re-checks every coherence-participating file (`IMPL_SENTINEL`).
        if impl_set_changed {
            changed_names.insert(IMPL_SENTINEL.to_string());
        }

        // Propagate: unchanged files referencing a changed name are dirty.
        for (_, rel) in &current {
            if dirty.contains(rel) {
                continue;
            }
            let Some(entry) = prev_files.get(rel.as_str()) else {
                continue;
            };
            if entry
                .referenced_names
                .iter()
                .any(|name| changed_names.contains(name))
            {
                dirty.insert(rel.clone());
            }
        }

        let clean_files: HashSet<String> = current
            .iter()
            .filter(|(_, rel)| !dirty.contains(rel))
            .map(|(_, rel)| rel.clone())
            .collect();
        if clean_files.is_empty() {
            cache_debug(format_args!("all files dirty — full compile"));
            return None;
        }
        let dirty_files: Vec<SourceFile> = current
            .iter()
            .filter(|(_, rel)| dirty.contains(rel))
            .map(|(sf, _)| *sf)
            .collect();
        cache_debug(format_args!(
            "reuse plan: {} clean, {} dirty",
            clean_files.len(),
            dirty_files.len()
        ));

        // Seed throw facts for clean files only: a clean file's content is
        // unchanged (facts are content-derived) and nothing it references
        // changed signature (resolution-derived parts are stable too).
        let root = db.get_project().map(|p| p.root(db));
        let seeded_throw_facts = manifest
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
        self.store(program)?;

        // Store the symbolic image alongside the Program blob (B-693 Stage 3) so
        // the next incremental compile can reuse clean files' units. Best-effort:
        // if the decomposition fails, the next compile simply full-compiles.
        let options = CompileOptions {
            emit_test_cases: self.emit_test_cases,
        };
        match decompose_units(db, &options, program) {
            Ok(units) => match borsh::to_vec(&LinkableImage { units }) {
                Ok(payload) => {
                    if let Err(e) = self.cache.store_raw(&self.image_key, &payload) {
                        cache_debug(format_args!("image store failed: {e}"));
                    }
                }
                Err(e) => cache_debug(format_args!("image serialize failed: {e}")),
            },
            Err(e) => cache_debug(format_args!("image decompose failed: {e}")),
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
                set.extend(syntactic_type_names(db, sf));
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
                    defined_names: defined_names(db, sf),
                    referenced_names,
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
    /// the honest full check; this compares every previous-manifest file that is
    /// still content-clean (i.e. would have been served from cache) against a
    /// fresh `check_file`. A mismatch means the cached diagnostics are a stale
    /// substitute that would change what a warm incremental run reports — a hard
    /// error, and a tighter signal than the whole-`Program` byte-compare.
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
        for entry in &manifest.files {
            let full = root.join(&entry.rel_path);
            let Some(sf) = db.get_file(&full) else {
                continue; // file removed — never served
            };
            if entry.content_hash != content_hash(sf.text(db)) {
                continue; // changed — always re-checked, never served from cache
            }
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
    //! Soundness of the dirty-set computation for the layout dependencies the
    //! bytecode-operand set alone misses (field reorder, enum variant reorder,
    //! type-alias change, and inferred-receiver field access). Each `plan_reuse`
    //! test caches an initial compile, edits one type, and asserts the affected
    //! consumer is dirtied while an unrelated file stays clean — the exact
    //! verdicts that were wrong before the fix.

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
        // These integration tests round-trip through the on-disk cache and run
        // on Linux (the primary CI platform). They are skipped on macOS/Windows,
        // where the on-disk round-trip is environment-sensitive (temp-dir
        // canonicalization, filesystem timestamp granularity, path separators —
        // see B-748). The dirty-set *mechanism* they exercise is asserted
        // directly, and platform-independently, by the unit tests below
        // (`defined_names_includes_type_aliases`,
        // `syntactic_type_names_capture_signature_and_alias_types`,
        // `referenced_names_carry_layout_sentinel_for_field_reader`).
        if !cfg!(target_os = "linux") {
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
        for name in baml_db::baml_compiler2_tir::package_interface::STDLIB_PACKAGE_NAMES {
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

    /// Bound the isolated cost of the stdlib interface derivation cluster
    /// (parse + type-lower + throw-infer over all six stdlib packages) that
    /// B-694 short-circuits. Run with:
    ///   cargo test --release -p baml_cli -- --ignored --nocapture \
    ///       stdlib_interface_derivation_cost
    /// This is an *upper bound* on the wall-clock win: in a real compile the
    /// stdlib parse happens regardless (via `package_items` for cross-package
    /// value resolution), so the marginal win is smaller (see the incremental
    /// with/without measurement).
    #[test]
    #[ignore = "timing harness; run explicitly in --release"]
    fn stdlib_interface_derivation_cost() {
        use baml_db::{
            Name, baml_compiler2_hir::package::PackageId,
            baml_compiler2_tir::package_interface::STDLIB_PACKAGE_NAMES,
        };

        // Parse-only: time `package_items` for the six stdlib packages on a
        // fresh db (parse + symbol table). This work happens in a real compile
        // regardless of the interface seed — B-694 does NOT cache it (design R4
        // residual), so it is the shared floor, not part of the realized win.
        let parse_db = build_db(&[("a.baml", "function f() -> int {\n  1\n}\n")]);
        let tp = std::time::Instant::now();
        for name in STDLIB_PACKAGE_NAMES {
            let pkg_id = PackageId::new(&parse_db, Name::new(name));
            let _ = baml_db::baml_compiler2_ppir::package_items(&parse_db, pkg_id);
        }
        let parse_ms = tp.elapsed().as_secs_f64() * 1000.0;

        // Cold: a fresh db derives every stdlib interface from source
        // (parse + type-lower + throw-infer).
        let cold = build_db(&[("a.baml", "function f() -> int {\n  1\n}\n")]);
        let t0 = std::time::Instant::now();
        let cold_blob = extract_stdlib_interface(&cold);
        let cold_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // Warm: a fresh db seeded with those bytes short-circuits all six.
        let mut warm = build_db(&[("a.baml", "function f() -> int {\n  1\n}\n")]);
        warm.set_seeded_stdlib_interface(cold_blob);
        let t1 = std::time::Instant::now();
        let _ = extract_stdlib_interface(&warm);
        let warm_ms = t1.elapsed().as_secs_f64() * 1000.0;

        #[allow(clippy::print_stderr)]
        {
            eprintln!(
                "stdlib interface: parse(package_items)={parse_ms:.1}ms (shared floor, not cached) \
                 | cold_interface={cold_ms:.1}ms (parse+lower+throw) | warm_seeded={warm_ms:.3}ms | \
                 isolated_upper_bound={:.1}ms | realized_win≈cold-parse={:.1}ms (lower+throw only, \
                 since parse happens regardless)",
                cold_ms - warm_ms,
                cold_ms - parse_ms,
            );
        }
    }
}
