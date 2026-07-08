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
//!   cache-key inputs).

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
    compute_key, image_key, manifest_key,
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
                }
                Some(entry) => {
                    if entry.content_hash == content_hash(sf.text(db)) {
                        continue;
                    }
                    dirty.insert(rel.clone());
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
            }
        }
        // Conservative fallback for layout dependencies whose receiver type is
        // inferred (named in no signature): any type-layout change re-lowers
        // every file whose bytecode baked a layout (`LAYOUT_SENTINEL`).
        if type_signature_changed {
            changed_names.insert(LAYOUT_SENTINEL.to_string());
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

        Some(ReusePlan {
            clean_files,
            dirty_files,
            prev_units,
            seeded_throw_facts,
        })
    }

    /// Write the Program blob plus the manifest describing it. Called after
    /// every successful compile; best-effort like all cache writes.
    pub(crate) fn store_with_manifest(
        &self,
        db: &ProjectDatabase,
        program: &Program,
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
        ctx1.store_with_manifest(&db1, &program1)
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
}
