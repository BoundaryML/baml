//! Shared scaffolding for the on-disk bytecode-cache tests — the disk
//! round-trip helpers used by both `bytecode_cache`'s unit tests and the
//! `diagnostics_cache_oracle`. Keeping them here means the platform-canonical
//! `unique_root` rationale and the v1 compile+store setup live in exactly one
//! place instead of drifting between per-file copies.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use baml_db::{SourceFile, baml_compiler2_emit::CompileOptions};
use baml_project::ProjectDatabase;

use crate::{
    bytecode_cache::{CacheContext, compile_program},
    project_load::{self, ResolvedProject},
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The on-disk cache is disabled (so a disk-round-trip test must skip) whenever
/// caching is turned off wholesale or the verify tripwire forces a full compile.
pub(crate) fn cache_disabled() -> bool {
    std::env::var_os("BAML_NO_BYTECODE_CACHE").is_some()
        || std::env::var_os("BAML_CACHE_VERIFY").is_some()
}

/// The compile options every disk-round-trip test uses.
pub(crate) fn opts() -> CompileOptions {
    CompileOptions {
        emit_test_cases: false,
    }
}

/// A unique on-disk project root named `<prefix>-<pid>-<n>`, anchored beneath
/// the *canonical* temp base so it is already in the OS's resolved form (macOS
/// resolves the `/var` -> `/private/var` symlink; Windows adds the `\\?\`
/// verbatim prefix). `ProjectDatabase` canonicalizes both the project root and
/// every source path, but only when they exist on disk: db1 is built before the
/// cache dir exists (root canonicalize is a no-op fallback), then
/// `store_with_manifest` materializes the root, so db2's `set_project_root`
/// *does* canonicalize it — while the in-memory `.baml` files never exist to
/// canonicalize. If the base held an unresolved symlink, the root would then
/// gain a resolved prefix the file paths lack, `strip_prefix` would fail, every
/// rel_path would come out absolute, the reuse plan would collapse to `None`,
/// and `plan.dirty_files` would be empty (macOS/Windows only; `/tmp` on Linux
/// has no symlink so it was silently fine). Canonicalizing the base up front
/// keeps the root idempotent under `canonicalize`, so db1 and db2 agree on every
/// platform.
pub(crate) fn unique_root(prefix: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let base = std::env::temp_dir();
    let base = base.canonicalize().unwrap_or(base);
    base.join(format!("{prefix}-{}-{n}", std::process::id()))
}

/// A [`ResolvedProject`] for `files` rooted at `root`, with no manifest.
pub(crate) fn resolved(root: &Path, files: &[(&str, &str)]) -> ResolvedProject {
    ResolvedProject {
        root: root.to_path_buf(),
        manifest: None,
        files: files
            .iter()
            .map(|(name, content)| (root.join(name), (*content).to_string()))
            .collect(),
    }
}

/// Compile `files` at `root` and persist the v1 manifest + per-file units — the
/// setup every reuse-plan scenario runs before editing. Returns the v1 database
/// and cache context for the callers that assert against them.
pub(crate) fn compile_and_store_v1(
    root: &Path,
    files: &[(&str, &str)],
) -> (ProjectDatabase, CacheContext) {
    let _ = std::fs::remove_dir_all(root);
    let r1 = resolved(root, files);
    let db1 = project_load::build_db_from_sources(&r1, |_| {});
    let ctx1 = CacheContext::open(&r1, false).expect("cache opens");
    let program1 = compile_program(&db1, &opts(), Some(&ctx1), None).expect("v1 compile succeeds");
    let fresh1 = ctx1
        .collect_diagnostics_incremental(&db1, None)
        .fresh_by_file;
    ctx1.store_with_manifest(&db1, &program1, &fresh1, None)
        .expect("v1 manifest stored");
    (db1, ctx1)
}

/// The basenames of a reuse plan's dirty files.
pub(crate) fn dirty_basenames(dirty_files: &[SourceFile], db: &ProjectDatabase) -> HashSet<String> {
    dirty_files
        .iter()
        .filter_map(|sf| {
            sf.path(db)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect()
}
