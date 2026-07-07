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
        CompileOptions, LoweringError, OptLevel, generate_project_bytecode,
        generate_project_bytecode_with_reuse, generate_project_bytecode_with_stdlib,
        generate_stdlib_program,
    },
    baml_compiler2_hir,
};
use baml_project::ProjectDatabase;
use bex_cache::{
    BytecodeCache, CacheKey, KeyInputs, ManifestFile, ProjectManifest, compiler_fingerprint,
    compute_key, manifest_key,
};
use bex_vm_types::{Object, Program, relink};
use sha2::{Digest, Sha256};

use crate::{file_signature::file_signature_hash, project_load::ResolvedProject};

/// The optimization level every CLI compile uses (the emit default).
const CLI_OPT_LEVEL: OptLevel = OptLevel::Two;

/// An opened cache plus the keys for one resolved project + compile config.
pub(crate) struct CacheContext {
    cache: BytecodeCache,
    /// Whole-project Program, keyed by sources + options + compiler build.
    key: CacheKey,
    /// Precompiled stdlib slice, keyed by compiler build + opt level only.
    stdlib_key: CacheKey,
    /// Latest-compile manifest, fixed per (project root, options, build).
    manifest_key: CacheKey,
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
            stdlib_key: bex_cache::stdlib_key(&fingerprint, CLI_OPT_LEVEL as u8),
            manifest_key: manifest_key(
                &fingerprint,
                CLI_OPT_LEVEL as u8,
                emit_test_cases,
                &resolved.root,
                resolved.manifest.as_deref(),
            ),
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
        match generate_project_bytecode_with_reuse(
            db,
            options,
            CLI_OPT_LEVEL,
            &base,
            &plan.prev,
            &plan.clean_files,
        ) {
            Ok(program) => return Ok(program),
            // A construct the splice can't relocate: fall back to the full
            // (stdlib-spliced) compile. Never an error the user should see.
            Err(LoweringError::ReuseUnsupported(reason)) => {
                cache_debug(format_args!("relink fell back: {reason}"));
            }
            Err(other) => return Err(other),
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
    /// The previous compile's Program, splice source.
    pub(crate) prev: Program,
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
        loc::{ClassLoc, EnumLoc, FunctionLoc, InterfaceLoc, LetLoc},
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
    names.sort_unstable();
    names.dedup();
    names
}

/// Last-segment names referenced by each user file's compiled bytecode,
/// grouped by root-relative path.
///
/// Extracted from the Program (not source), so desugared references — a
/// `for` loop's `next()`, injected guards — are all included. Object refs
/// resolve through the pool (classes/enums/interfaces by name; function
/// objects are a file's own lambdas — internal, skipped); global slots
/// resolve through the inverted name maps.
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
        let Some(prev) = self.cache.load(&CacheKey::from_bytes(manifest.program_key)) else {
            cache_debug(format_args!("previous blob missing — full compile"));
            return None;
        };

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

        for (sf, rel) in &current {
            match prev_files.get(rel.as_str()) {
                None => {
                    // Added file: recompiles, and its names may shadow
                    // existing references anywhere.
                    dirty.insert(rel.clone());
                    changed_names.extend(defined_names(db, *sf));
                }
                Some(entry) => {
                    if entry.content_hash == content_hash(sf.text(db)) {
                        continue;
                    }
                    dirty.insert(rel.clone());
                    if entry.signature_hash != file_signature_hash(db, *sf) {
                        changed_names.extend(entry.defined_names.iter().cloned());
                        changed_names.extend(defined_names(db, *sf));
                    }
                }
            }
        }
        for (rel, entry) in &prev_files {
            if !current_rels.contains(rel) {
                // Removed file: its names vanish from resolution.
                changed_names.extend(entry.defined_names.iter().cloned());
            }
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

        Some(ReusePlan {
            clean_files,
            dirty_files,
            prev,
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

        let mut referenced = referenced_names_by_file(program);
        let mut files: Vec<ManifestFile> = user_files_with_rel_paths(db)
            .into_iter()
            .map(|(sf, rel)| ManifestFile {
                content_hash: content_hash(sf.text(db)),
                signature_hash: file_signature_hash(db, sf),
                defined_names: defined_names(db, sf),
                referenced_names: referenced.remove(&rel).unwrap_or_default(),
                rel_path: rel,
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
