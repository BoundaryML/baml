//! Cold-open cache seeding for the LSP's long-lived [`ProjectDatabase`].
//!
//! The CLI (`baml run` / `baml check`) already persists everything a cold
//! compile needs to skip most of the honest typecheck: the stdlib typed
//! interface blob (a build constant), a per-file manifest of throw facts, and a
//! per-file `callable_throws` fragment inside each compilation unit. This module
//! loads those blobs when a project is constructed and seeds them into the LSP
//! database, so the first diagnostics after an editor opens a project are served
//! from a warm typecheck instead of a cold one — the same seeds `run_command`
//! installs, applied on the LSP's long-lived database.
//!
//! # Why the LSP needs a different discipline from the CLI
//!
//! The CLI seeds a database that lives for exactly one compile. The LSP database
//! lives for hours across edits, and the seed short-circuits
//! (`file_throw_facts`, `callable_throws`) *hide a query's dependency on file
//! content from salsa*: a seeded query returns its cached value without reading
//! the body, so a later edit does not invalidate it. Serving a stale throw set
//! after an edit would be a silent wrong-diagnostics bug. Two properties keep
//! seeding sound on the long-lived database:
//!
//! 1. **Whole-project-clean gating for the per-file seeds.** `file_throw_facts`
//!    is a function of file content *and name resolution*, and `callable_throws`
//!    is transitive over the call graph (a caller's value embeds its callees'
//!    throws — see `builder.rs`, which reads `callable_throws` when resolving a
//!    call). A single file that differs from the manifest could therefore make a
//!    *matching* file's seed stale through a cross-file edge (a changed callee,
//!    a shadowed name). So the per-file seeds are installed only when the whole
//!    project matches the manifest — same file set, same content hashes. Then
//!    every transitive contributor is exactly the one the cache was built from,
//!    so every seed is the honest converged value. If any file differs (an
//!    editor opened with unsaved changes, a stray edit before build), the
//!    per-file seeds are skipped entirely and only the content-independent
//!    stdlib interface is seeded.
//!
//! 2. **Clear-all eviction on the first content edit.** `callable_throws(foo)`
//!    returns its seed verbatim, hiding not just `foo`'s own body but every
//!    transitive callee. Dropping only the *edited* file's entry would leave an
//!    unedited intermediary's seed stale — its callee's throws grew, but its own
//!    seed did not — a silent wrong-diagnostics bug for *that* intermediary's
//!    callers. Because the per-file maps were installed as one whole-project-
//!    clean unit, an edit breaks that invariant for the whole set, so both
//!    per-file seed maps are cleared entirely (and the seed source dropped) on
//!    the first content edit. Every query then recomputes honestly and salsa
//!    invalidates dependents normally. (Per-file eviction would be sound only
//!    for the stdlib interface and would leave the transitive
//!    `callable_throws`/name-resolution holes above open, so the conservative
//!    clear-all is used.) A full *reload* — discovery, didOpen, a watched-file
//!    refresh — is not an edit: it re-reads the sources, so it re-evaluates the
//!    seeds against the fresh content (whole-project-clean gated) rather than
//!    evicting, which is what lets them survive the discovery→didOpen reload
//!    pair and serve the very first diagnostics.
//!
//! The stdlib interface is a build constant — no user file contributes to a
//! stdlib package — so it is applied once at construction and never evicted.
//!
//! # Keying
//!
//! Seeds are keyed by compiler fingerprint + opt level + `emit_test_cases`. The
//! LSP runs as `baml lsp`, the same `baml-cli` executable that populates the
//! cache, so [`bex_cache::compiler_fingerprint`] (a hash of `current_exe`)
//! matches. The LSP compiles through `ProjectDatabase::get_bytecode`, which uses
//! `OptLevel::Two` and `emit_test_cases = false` — matching the manifest written
//! by `baml run` / `baml check` (not `baml test`, which uses `true`). If a
//! different binary populated the cache (a bundled extension server distinct
//! from the user's CLI), the fingerprint differs and no blob loads — a silent,
//! correct miss.
//!
//! # Read-only
//!
//! The LSP is a read-only cache consumer: it loads blobs but never stores cache
//! entries. `compiler_fingerprint` may refresh its own fingerprint memo and
//! `load_raw` may freshen an entry's mtime after ≥1h, but neither adds or
//! mutates a compilation artifact — the same benign metadata the CLI already
//! manages.
//!
//! # Opt-outs
//!
//! - `BAML_NO_BYTECODE_CACHE=1` / `BAML_NO_LSP_CACHE_SEED=1` — no seeds at all.
//! - `BAML_NO_STDLIB_INTERFACE_CACHE=1` — skip only the stdlib interface seed.
//! - `BAML_NO_DIAGNOSTICS_CACHE=1` — skip the per-file (throw-facts +
//!   `callable_throws`) seeds.
//! - `BAML_NO_CALLABLE_THROWS_CACHE=1` — skip only the `callable_throws` seed.
//! - `BAML_CACHE_DIR=<path>` — cache location override (mirrors the CLI).

use std::collections::BTreeMap;

use baml_project::ProjectDatabase;
use baml_type::{Ty, throw_facts::FunctionThrowFacts};

/// Optimization level the LSP compiles with (`ProjectDatabase::get_bytecode`
/// uses `generate_project_bytecode`, whose default is `OptLevel::Two`). The
/// cache keys bake in the opt level, so the LSP must key with the same value the
/// CLI used (also `OptLevel::Two`) to find the blobs.
#[cfg(not(target_arch = "wasm32"))]
const LSP_OPT_LEVEL: u8 = 2;

/// `emit_test_cases` the LSP compiles with. `get_bytecode` passes `false`, which
/// matches the manifest `baml run` / `baml check` write; `baml test` uses
/// `true` and keys a different manifest the LSP intentionally does not read.
#[cfg(not(target_arch = "wasm32"))]
const LSP_EMIT_TEST_CASES: bool = false;

/// Blobs loaded from the on-disk cache for one project, decoded and ready to
/// seed. Loading (all disk I/O and borsh decoding) happens at project
/// construction, before the database goes behind the source gate.
pub(crate) struct LspSeedCache {
    /// Stdlib typed-interface blob: package name -> `borsh(PackageInterface)`.
    /// Content-independent; applied once and never evicted.
    stdlib_interface: Option<BTreeMap<String, Vec<u8>>>,
    /// Per-file seeds awaiting the first source population (installed only when
    /// the project is whole-project-clean against the manifest).
    per_file: Option<PerFileSeeds>,
}

/// Per-file seeds, keyed by project-root-relative path, decoded from the
/// manifest and compilation units. Installed onto the database at the first
/// source population, whole-project-clean gated.
pub(crate) struct PerFileSeeds {
    /// `rel_path` -> manifest content hash (the whole-project-clean gate input).
    content_hash: BTreeMap<String, [u8; 32]>,
    /// `rel_path` -> throw facts (manifest `file_throw_facts` output).
    throw_facts: BTreeMap<String, Vec<FunctionThrowFacts>>,
    /// `rel_path` -> (item-tree `LocalItemId::as_u32` -> `callable_throws`),
    /// projected from each unit's interface fragment.
    callable_throws: BTreeMap<String, BTreeMap<u32, Ty>>,
}

impl LspSeedCache {
    /// Apply the content-independent stdlib interface seed to `db` immediately
    /// (before the database is shared behind the source gate) and return the
    /// per-file seeds to install at the first source population, if any.
    pub(crate) fn install_stdlib(self, db: &mut ProjectDatabase) -> Option<PerFileSeeds> {
        if let Some(blob) = self.stdlib_interface {
            db.set_seeded_stdlib_interface(blob);
        }
        self.per_file
    }
}

impl PerFileSeeds {
    /// Install the per-file throw-facts and `callable_throws` seeds onto `db`,
    /// gated on the project being whole-project-clean against the manifest: the
    /// current user file set must equal the manifest's, with matching content
    /// hashes. Returns `true` if the seeds were installed (so the caller arms
    /// eviction), `false` if the project differed and nothing was seeded.
    ///
    /// `root` is the canonical project root; rel paths are stripped from the
    /// database's (canonical) file paths exactly as the CLI built the manifest.
    /// No I/O happens here — the blobs were decoded at construction — so this is
    /// cheap enough to run under the source gate.
    pub(crate) fn apply(&self, db: &mut ProjectDatabase, root: &std::path::Path) -> bool {
        // (full_path_string, rel_path, content_hash) for every current user
        // file. `get_source_files` returns user files only (no builtins).
        let mut current: Vec<(String, String, [u8; 32])> = Vec::new();
        for file in db.get_source_files() {
            let path = file.path(db);
            let rel = rel_path(root, &path);
            let full = path.display().to_string();
            current.push((full, rel, content_hash(file.text(db))));
        }

        // Whole-project-clean: bijection between current files and manifest
        // files, every content hash matching. A count mismatch means a file was
        // added or removed; a rel/hash miss means a file changed. Any of these
        // makes a cross-file seed edge potentially stale, so seed nothing.
        if current.len() != self.content_hash.len() {
            return false;
        }
        for (_, rel, hash) in &current {
            match self.content_hash.get(rel) {
                Some(expected) if expected == hash => {}
                _ => return false,
            }
        }

        // Re-key by full path (how `file_throw_facts` / `callable_throws` look
        // their seeds up) and install. A file with no throw facts / no fragment
        // simply gets no entry and infers honestly.
        let mut throw_facts = BTreeMap::new();
        let mut callable_throws = BTreeMap::new();
        for (full, rel, _) in &current {
            if let Some(facts) = self.throw_facts.get(rel) {
                throw_facts.insert(full.clone(), facts.clone());
            }
            if let Some(by_id) = self.callable_throws.get(rel) {
                callable_throws.insert(full.clone(), by_id.clone());
            }
        }
        db.set_seeded_throw_facts(throw_facts);
        db.set_seeded_callable_throws(callable_throws);
        true
    }
}

/// Evict the per-file seeds: clear both the throw-facts and `callable_throws`
/// seed maps entirely. Called on a content edit (and before re-applying on a
/// full reload).
///
/// Clearing the *whole* maps (rather than only the changed file's entry) is
/// what keeps eviction sound: `callable_throws` seeds hide transitive callee
/// edges and `file_throw_facts` seeds hide name-resolution edges, so a single
/// edit can invalidate an unedited file's seed through a cross-file path. The
/// per-file maps were only ever installed as a whole-project-clean unit, so an
/// edit dissolves the whole unit back to honest recomputation. The stdlib
/// interface seed is a build constant and is deliberately left untouched.
pub(crate) fn evict_per_file_seeds(db: &mut ProjectDatabase) {
    db.set_seeded_throw_facts(BTreeMap::new());
    db.set_seeded_callable_throws(BTreeMap::new());
}

/// SHA-256 of a file's full content, matching the CLI's manifest writer so the
/// whole-project-clean gate compares like against like. Native routes through
/// the shared `bex_cache::content_hash`; WASM has no on-disk cache to agree
/// with, so it hashes locally.
#[cfg(not(target_arch = "wasm32"))]
fn content_hash(text: &str) -> [u8; 32] {
    bex_cache::content_hash(text)
}

#[cfg(target_arch = "wasm32")]
fn content_hash(text: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes()).into()
}

/// `path` relative to `root` — the project-root-relative form the CLI manifest
/// writer keys each file by, so the LSP's seed lookup matches it. Native shares
/// `bex_cache::rel_path`; WASM (no on-disk cache) formats locally.
#[cfg(not(target_arch = "wasm32"))]
fn rel_path(root: &std::path::Path, path: &std::path::Path) -> String {
    bex_cache::rel_path(root, path)
}

#[cfg(target_arch = "wasm32")]
fn rel_path(root: &std::path::Path, path: &std::path::Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// The seed opt-out knobs are all consulted on the native disk-loading path;
/// WASM has no on-disk cache, so this reader is native-only.
#[cfg(not(target_arch = "wasm32"))]
use bex_cache::env_flag;

// ── On-disk loading (native only; WASM has no filesystem cache) ─────────────

#[cfg(not(target_arch = "wasm32"))]
impl LspSeedCache {
    /// Load and decode the cache blobs for a project rooted at `root`. Returns
    /// `None` when seeding is disabled, the cache directory is absent, or
    /// nothing decodable is present. Any per-blob error degrades to "not
    /// present" — a corrupt cache never surfaces to the user, it just yields no
    /// seeds and today's cold build.
    pub(crate) fn load_for_root(root: &std::path::Path) -> Option<LspSeedCache> {
        use bex_cache::{BytecodeCache, compiler_fingerprint};

        if env_flag("BAML_NO_BYTECODE_CACHE") || env_flag("BAML_NO_LSP_CACHE_SEED") {
            return None;
        }
        // Canonicalize so the cache directory and manifest key match the CLI's
        // resolved (canonical) root exactly. A path that will not canonicalize
        // (missing directory) has no populated cache to read.
        let root = std::fs::canonicalize(root).ok()?;
        let cache_dir = cache_dir_for(&root);
        // Only touch disk when a cache directory actually exists: keeps MemoryFS
        // roots (tests) and never-built projects a cheap stat, and avoids
        // fingerprinting the executable when there is nothing to load.
        if !cache_dir.is_dir() {
            return None;
        }
        let fingerprint = compiler_fingerprint(&cache_dir);
        let cache = BytecodeCache::open(cache_dir);

        let stdlib_interface = load_stdlib_interface(&cache, &fingerprint);
        let per_file = load_per_file_seeds(&cache, &fingerprint, &root);
        if stdlib_interface.is_none() && per_file.is_none() {
            return None;
        }
        Some(LspSeedCache {
            stdlib_interface,
            per_file,
        })
    }
}

/// WASM has no on-disk cache; seeding is a no-op there.
#[cfg(target_arch = "wasm32")]
impl LspSeedCache {
    pub(crate) fn load_for_root(_root: &std::path::Path) -> Option<LspSeedCache> {
        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn cache_dir_for(root: &std::path::Path) -> std::path::PathBuf {
    std::env::var_os("BAML_CACHE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join(".baml").join("cache"))
}

#[cfg(not(target_arch = "wasm32"))]
fn load_stdlib_interface(
    cache: &bex_cache::BytecodeCache,
    fingerprint: &[u8; 32],
) -> Option<BTreeMap<String, Vec<u8>>> {
    if env_flag("BAML_NO_STDLIB_INTERFACE_CACHE") {
        return None;
    }
    let key = bex_cache::stdlib_interface_key(fingerprint, LSP_OPT_LEVEL);
    let bytes = cache.load_raw(&key)?;
    borsh::from_slice::<BTreeMap<String, Vec<u8>>>(&bytes).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn load_per_file_seeds(
    cache: &bex_cache::BytecodeCache,
    fingerprint: &[u8; 32],
    root: &std::path::Path,
) -> Option<PerFileSeeds> {
    use bex_cache::{ProjectManifest, manifest_key};

    if env_flag("BAML_NO_DIAGNOSTICS_CACHE") {
        return None;
    }
    // baml.toml is a manifest-key input (a config-only change must not splice
    // stale sources): read it exactly as the CLI's project_load does.
    let manifest_toml = std::fs::read_to_string(root.join("baml.toml")).ok();
    let key = manifest_key(
        fingerprint,
        LSP_OPT_LEVEL,
        LSP_EMIT_TEST_CASES,
        root,
        manifest_toml.as_deref(),
    );
    let bytes = cache.load_raw(&key)?;
    let manifest = borsh::from_slice::<ProjectManifest>(&bytes).ok()?;

    let mut content_hash = BTreeMap::new();
    let mut throw_facts = BTreeMap::new();
    for file in &manifest.files {
        content_hash.insert(file.rel_path.clone(), file.content_hash);
        throw_facts.insert(file.rel_path.clone(), file.throw_facts.clone());
    }
    let callable_throws = if env_flag("BAML_NO_CALLABLE_THROWS_CACHE") {
        BTreeMap::new()
    } else {
        load_callable_throws(cache, &manifest)
    };

    Some(PerFileSeeds {
        content_hash,
        throw_facts,
        callable_throws,
    })
}

/// Project each file's cached interface fragment into a per-function
/// `callable_throws` seed. Mirrors the CLI's `project_callable_throws_seeds`,
/// but reads every clean file's unit through the manifest pointer and is
/// strictly read-only: a content-key mismatch is skipped, never deleted (the
/// LSP does not write the cache). Any decode problem degrades that file to
/// honest inference.
#[cfg(not(target_arch = "wasm32"))]
fn load_callable_throws(
    cache: &bex_cache::BytecodeCache,
    manifest: &bex_cache::ProjectManifest,
) -> BTreeMap<String, BTreeMap<u32, Ty>> {
    use baml_db::baml_compiler2_hir_ty::package_interface::CallableThrowsFragment;
    use bex_cache::{CacheKey, unit_key};
    use bex_vm_types::CompilationUnit;

    let mut out = BTreeMap::new();
    for file in &manifest.files {
        let key = CacheKey::from_bytes(file.unit_key);
        let Some(payload) = cache.load_raw(&key) else {
            continue;
        };
        // Read-only content-address check: the payload must hash to its key.
        // `load_unit_shared` would delete a mismatch; the LSP must not.
        if unit_key(&payload) != key {
            continue;
        }
        let Ok(unit) = borsh::from_slice::<CompilationUnit>(&payload) else {
            continue;
        };
        if unit.source_file != file.rel_path || unit.callable_throws_fragment.is_empty() {
            continue;
        }
        let Ok(fragment) =
            borsh::from_slice::<CallableThrowsFragment>(&unit.callable_throws_fragment)
        else {
            continue;
        };
        if fragment.by_id.is_empty() {
            continue;
        }
        out.insert(file.rel_path.clone(), fragment.by_id);
    }
    out
}

#[cfg(test)]
mod tests {
    use baml_db::{
        Name, baml_compiler2_hir,
        baml_compiler2_hir_ty::{
            callable::callable_throws, package_interface, throw_facts::file_throw_facts,
        },
        baml_compiler2_ppir,
    };
    use baml_project::ProjectDatabase;

    use super::*;

    /// A synthetic project root that does not exist on disk, so
    /// `set_project_root` keeps it verbatim (canonicalization fails and falls
    /// back) — the in-memory correctness tests never touch the filesystem.
    const ROOT: &str = "/baml-lsp-seed-test-root";

    fn build_db(files: &[(&str, &str)]) -> ProjectDatabase {
        build_db_at(std::path::Path::new(ROOT), files)
    }

    fn build_db_at(root: &std::path::Path, files: &[(&str, &str)]) -> ProjectDatabase {
        let mut db = ProjectDatabase::new();
        db.set_project_root(root);
        for (name, content) in files {
            db.add_or_update_file(&root.join(name), content);
        }
        db
    }

    /// A stable, order-independent rendering of the whole-project diagnostic
    /// set — the "rendered set" the seeded and honest databases must agree on
    /// byte-for-byte.
    fn rendered_diagnostics(db: &ProjectDatabase) -> Vec<String> {
        let mut out: Vec<String> = baml_project::collect_compiler2_diagnostics(db)
            .iter()
            .map(|d| {
                let span = d
                    .primary_span()
                    .map(|s| (s.file_id.as_u32(), u32::from(s.range.start())));
                format!("{:?}|{}|{}|{span:?}", d.severity, d.code(), d.message)
            })
            .collect();
        out.sort();
        out
    }

    /// The project root the database recorded (canonical form), so seed
    /// extraction and application strip identical rel paths.
    fn project_root(db: &ProjectDatabase) -> std::path::PathBuf {
        db.get_project().expect("project set").root(db)
    }

    /// Extract the exact blobs the disk path would decode, straight from a warm
    /// reference database's queries (no disk round-trip): the stdlib interface,
    /// and per-file content hashes / throw facts / `callable_throws` fragments.
    fn extract_seeds(db: &ProjectDatabase) -> (LspSeedCache, PerFileSeeds) {
        let root = project_root(db);

        let mut stdlib = BTreeMap::new();
        for name in baml_builtins2::stdlib_package_names().iter().copied() {
            let pkg_id = baml_compiler2_hir::package::PackageId::new(db, Name::new(name));
            let iface = package_interface::package_interface(db, pkg_id);
            stdlib.insert(name.to_string(), borsh::to_vec(iface).unwrap());
        }

        let mut content_hash = BTreeMap::new();
        let mut throw_facts = BTreeMap::new();
        let mut callable = BTreeMap::new();
        for sf in db.get_source_files() {
            let path = sf.path(db);
            let rel = super::rel_path(&root, &path);
            content_hash.insert(rel.clone(), content_hash_of(sf.text(db)));
            throw_facts.insert(rel.clone(), file_throw_facts(db, sf).0.clone());
            let frag = package_interface::file_callable_throws_fragment(db, sf);
            if !frag.by_id.is_empty() {
                callable.insert(rel.clone(), frag.by_id.clone());
            }
        }
        let per_file = PerFileSeeds {
            content_hash,
            throw_facts,
            callable_throws: callable,
        };
        // A second, independently owned copy for the seed cache wrapper (so the
        // caller can both `apply` directly and drive `install_stdlib`).
        let per_file_for_cache = PerFileSeeds {
            content_hash: per_file.content_hash.clone(),
            throw_facts: per_file.throw_facts.clone(),
            callable_throws: per_file.callable_throws.clone(),
        };
        (
            LspSeedCache {
                stdlib_interface: Some(stdlib),
                per_file: Some(per_file_for_cache),
            },
            per_file,
        )
    }

    fn content_hash_of(text: &str) -> [u8; 32] {
        super::content_hash(text)
    }

    fn source_file(db: &ProjectDatabase, name: &str) -> baml_db::SourceFile {
        db.get_source_files()
            .into_iter()
            .find(|sf| sf.path(db).file_name().and_then(|f| f.to_str()) == Some(name))
            .expect("file present")
    }

    fn first_func_throws(db: &ProjectDatabase, file: &str) -> String {
        let sf = source_file(db, file);
        // The fixture defines exactly one function; the firewall enumeration is
        // source-ordered, so the first loc is that function.
        let loc = *baml_compiler2_ppir::item_data::file_functions(db, sf)
            .first()
            .expect("one function");
        format!("{:?}", callable_throws(db, loc))
    }

    fn file_facts(
        db: &ProjectDatabase,
        file: &str,
    ) -> Vec<baml_type::throw_facts::FunctionThrowFacts> {
        file_throw_facts(db, source_file(db, file)).0.clone()
    }

    // (a) A seeded database produces byte-for-byte identical diagnostics to an
    // honest one, on a multi-file fixture (valid code, a cross-file call, and a
    // deliberate type error).
    #[test]
    fn seeded_diagnostics_match_honest_byte_for_byte() {
        let files = &[
            (
                "a.baml",
                "class Point {\n  x: int\n  y: int\n}\n\nfunction mk() -> Point {\n  Point { x: 1, y: 2 }\n}\n",
            ),
            (
                "b.baml",
                "function useit() -> int {\n  let p = mk();\n  p.x\n}\n",
            ),
            ("c.baml", "function bad() -> int {\n  \"not an int\"\n}\n"),
        ];

        // Warm reference database → extract seeds.
        let reference = build_db(files);
        let honest = rendered_diagnostics(&reference);

        let (cache, per_file) = extract_seeds(&reference);

        // Fresh cold database, seeded via the real install path.
        let mut seeded = build_db(files);
        let root = project_root(&seeded);
        let pending = cache.install_stdlib(&mut seeded);
        assert!(pending.is_some(), "cache carries per-file seeds");
        assert!(
            per_file.apply(&mut seeded, &root),
            "whole-project-clean fixture must install per-file seeds"
        );

        assert_eq!(
            rendered_diagnostics(&seeded),
            honest,
            "seeded diagnostics must match honest byte-for-byte"
        );
    }

    // Per-file seeds are skipped when the project is not whole-project-clean:
    // one file differing from the manifest must yield no per-file seeds (only
    // the content-independent stdlib interface would still seed).
    #[test]
    fn per_file_seeds_skipped_when_a_file_differs() {
        let files = &[
            ("a.baml", "function a() -> int { 1 }\n"),
            ("b.baml", "function b() -> int { 2 }\n"),
        ];
        let reference = build_db(files);
        let (_cache, per_file) = extract_seeds(&reference);

        // Same set, but b.baml differs from the manifest content.
        let mut db = build_db(&[
            ("a.baml", "function a() -> int { 1 }\n"),
            ("b.baml", "function b() -> int { 99 }\n"),
        ]);
        let root = project_root(&db);
        assert!(
            !per_file.apply(&mut db, &root),
            "a differing file must block whole-project-clean seeding"
        );
    }

    // (b) After an edit to file F, eviction restores honest recomputation — and
    // the stale seed WOULD differ, so eviction is load-bearing. The fixture is a
    // three-level call chain (baz -> foo -> bar) so the *transitive* hole (an
    // unedited intermediary `foo`) is exercised, which per-file eviction of only
    // the edited file would miss.
    #[test]
    fn eviction_restores_honest_recomputation_and_stale_seed_would_differ() {
        let v1 = &[
            ("bar.baml", "function bar() -> int { 1 }\n"),
            ("foo.baml", "function foo() -> int { bar() }\n"),
            ("baz.baml", "function baz() -> int throws never { foo() }\n"),
        ];
        // bar grows a throw; foo (unedited) transitively throws it too.
        let bar_v2 = "function bar() -> int { throw \"boom\" }\n";

        // Reference (v1) → seeds.
        let reference = build_db(v1);
        let (_cache, per_file_a) = extract_seeds(&reference);
        let (_cache_b, per_file_b) = extract_seeds(&reference);

        // Honest v2 database (no seeds).
        let mut honest_v2 = build_db(v1);
        honest_v2.add_or_update_file(&std::path::PathBuf::from(ROOT).join("bar.baml"), bar_v2);
        let honest_v2_diags = rendered_diagnostics(&honest_v2);
        let honest_bar_facts = file_facts(&honest_v2, "bar.baml");
        let honest_foo_throws = first_func_throws(&honest_v2, "foo.baml");

        // Seeded, edited, EVICTED → must equal honest v2 everywhere.
        let mut evicted = build_db(v1);
        let root = project_root(&evicted);
        assert!(per_file_a.apply(&mut evicted, &root), "v1 is whole-clean");
        assert_eq!(
            rendered_diagnostics(&evicted),
            rendered_diagnostics(&reference),
            "seeded v1 diagnostics match honest v1"
        );
        evicted.add_or_update_file(&std::path::PathBuf::from(ROOT).join("bar.baml"), bar_v2);
        evict_per_file_seeds(&mut evicted);
        assert_eq!(
            file_facts(&evicted, "bar.baml"),
            honest_bar_facts,
            "after eviction bar's throw facts recompute honestly"
        );
        assert_eq!(
            first_func_throws(&evicted, "foo.baml"),
            honest_foo_throws,
            "after eviction foo's transitive callable_throws recompute honestly"
        );
        assert_eq!(
            rendered_diagnostics(&evicted),
            honest_v2_diags,
            "after eviction diagnostics match honest recomputation"
        );

        // Seeded, edited, NOT evicted → the stale seed persists (this is exactly
        // the silent-wrong bug eviction prevents), proving eviction is required.
        let mut stale = build_db(v1);
        assert!(per_file_b.apply(&mut stale, &root), "v1 is whole-clean");
        stale.add_or_update_file(&std::path::PathBuf::from(ROOT).join("bar.baml"), bar_v2);
        // bar was edited but its throw-facts seed still hides the new body.
        assert_ne!(
            file_facts(&stale, "bar.baml"),
            honest_bar_facts,
            "without eviction bar's seed hides its edited body"
        );
        // foo is the UNEDITED intermediary: its callable_throws seed still hides
        // bar's grown throws — the transitive hole a per-file eviction of only
        // `bar` would leave open, and which the clear-all eviction closes.
        assert_ne!(
            first_func_throws(&stale, "foo.baml"),
            honest_foo_throws,
            "without eviction foo's transitive seed is stale"
        );
    }

    // (c) With no cache present, seeding is a no-op and behavior is identical to
    // today's cold build.
    #[test]
    fn cache_absent_yields_no_seeds() {
        // A directory with no .baml/cache: load returns None.
        let dir = unique_temp_dir("absent");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(
            LspSeedCache::load_for_root(&dir).is_none(),
            "no cache dir → no seeds"
        );
        let _ = std::fs::remove_dir_all(&dir);

        // And a nonexistent root canonicalizes to nothing → None.
        assert!(
            LspSeedCache::load_for_root(std::path::Path::new("/baml-lsp-seed-test-does-not-exist"))
                .is_none()
        );
    }

    fn unique_temp_dir(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "baml_lsp_seed_{tag}_{}_{n}_{nanos}",
            std::process::id()
        ))
    }

    // The real on-disk path end-to-end: a stdlib interface blob and a project
    // manifest are written under the CLI's keys, then `load_for_root` reads and
    // decodes them and `apply` installs them. A *poisoned* (empty) throw-facts
    // seed for a file that honestly throws proves the served value comes from
    // the on-disk blob — the whole keying (fingerprint + opt level + root) and
    // read/decode/apply chain — not from a fresh computation.
    #[test]
    fn seeds_round_trip_from_disk_and_are_served() {
        // A real, canonical project directory so `load_for_root`'s
        // canonicalization and the manifest key agree.
        let root = std::fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("baml_lsp_seed_disk_{}", std::process::id()));
        let cache_dir = root.join(".baml").join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();

        // a.baml honestly throws, so its honest throw facts are non-empty.
        let files = &[("a.baml", "function a() -> int { throw \"boom\" }\n")];
        let reference = build_db_at(&root, files);
        let honest_facts = file_facts(&reference, "a.baml");
        assert!(!honest_facts.is_empty(), "a() honestly throws");

        // Write the stdlib blob under its key.
        let (cache_seed, _) = extract_seeds(&reference);
        let blob = cache_seed.stdlib_interface.unwrap();
        let fingerprint = bex_cache::compiler_fingerprint(&cache_dir);
        let cache = bex_cache::BytecodeCache::open(cache_dir);
        cache
            .store_raw(
                &bex_cache::stdlib_interface_key(&fingerprint, super::LSP_OPT_LEVEL),
                &borsh::to_vec(&blob).unwrap(),
            )
            .unwrap();

        // Write a manifest whose throw-facts seed for a.baml is POISONED (empty)
        // even though a() throws — so a served seed is observable as wrong.
        let manifest = bex_cache::ProjectManifest {
            program_key: [0u8; 32],
            files: vec![bex_cache::ManifestFile {
                rel_path: "a.baml".to_string(),
                content_hash: super::content_hash(files[0].1),
                signature_hash: [0u8; 32],
                layout_hash: [0u8; 32],
                defined_names: Vec::new(),
                referenced_names: Vec::new(),
                sig_referenced_names: Vec::new(),
                throw_facts: Vec::new(), // poison: honest is non-empty
                diagnostics: Vec::new(),
                // Non-empty sentinel bytes: the manifest's verbatim fragment
                // carry must survive the disk round-trip untouched.
                callable_throws_fragment: vec![0xca, 0xfe, 0xf0, 0x0d],
                unit_key: [0u8; 32], // no unit → callable_throws seed empty
            }],
        };
        cache
            .store_raw(
                &bex_cache::manifest_key(
                    &fingerprint,
                    super::LSP_OPT_LEVEL,
                    super::LSP_EMIT_TEST_CASES,
                    &root,
                    None,
                ),
                &borsh::to_vec(&manifest).unwrap(),
            )
            .unwrap();
        let manifest_bytes = cache
            .load_raw(&bex_cache::manifest_key(
                &fingerprint,
                super::LSP_OPT_LEVEL,
                super::LSP_EMIT_TEST_CASES,
                &root,
                None,
            ))
            .expect("manifest reloads from disk");
        let reloaded: bex_cache::ProjectManifest =
            borsh::from_slice(&manifest_bytes).expect("manifest decodes");
        assert_eq!(
            reloaded.files[0].callable_throws_fragment,
            vec![0xca, 0xfe, 0xf0, 0x0d],
            "the fragment blob round-trips through disk serialization verbatim"
        );

        // Load from disk: both the stdlib and per-file seeds must decode.
        let loaded = LspSeedCache::load_for_root(&root).expect("blobs must load from disk");
        assert!(loaded.stdlib_interface.is_some());
        let per_file = loaded.per_file.expect("manifest decodes to per-file seeds");

        // Apply to a fresh, whole-project-clean database and observe the served
        // (poisoned) throw facts — proof the disk-loaded seed is used.
        let mut db = build_db_at(&root, files);
        assert!(
            per_file.apply(&mut db, &root),
            "matching content is whole-clean"
        );
        assert_eq!(
            file_facts(&db, "a.baml"),
            Vec::new(),
            "the poisoned on-disk throw-facts seed is served verbatim"
        );
        assert_ne!(file_facts(&db, "a.baml"), honest_facts);

        // Eviction restores the honest (non-empty) throw facts.
        evict_per_file_seeds(&mut db);
        assert_eq!(file_facts(&db, "a.baml"), honest_facts);

        let _ = std::fs::remove_dir_all(&root);
    }
}
