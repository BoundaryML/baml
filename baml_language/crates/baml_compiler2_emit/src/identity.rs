//! `finalize_program_identity` — the single finalizer every runnable
//! `Program` passes through (observability design §4.1/§4.3).
//!
//! Stamps dense function ids in final pool order and attaches
//! `ProgramIdentity` (revision = source × toolchain × options) plus the
//! source-file table the revision dictionary needs. Idempotent; runs at the
//! tail of every public entry that materializes a runnable program (full,
//! stdlib-seeded, splice, reuse-linked) — never on partial dirty-only
//! programs, which are not runnable.
//!
//! The bytecode cache stores `Program` via borsh, which skips identity —
//! cache loaders re-finalize after load (cheap: id stamping is one walk and
//! the per-file hashes are memoized salsa queries).

use baml_compiler2_hir::file_blake3;
use bex_vm_types::{
    Program, ProgramIdentity, SourceFileIdentity,
    identity::{revision_id, source_snapshot_id},
};

use crate::OptLevel;

/// Options that reach revision identity (§4.3). `emit_test_cases` is the
/// only `CompileOptions` field today; taking the flag (not the struct)
/// keeps this callable from cache-reload sites that don't hold options.
pub fn finalize_program_identity(
    db: &dyn crate::Db,
    opt: OptLevel,
    emit_test_cases: bool,
    program: &mut Program,
) {
    let function_count = bex_vm_types::assign_function_ids(program);

    // User source files, sorted by project-relative path, hashed through
    // the memoized salsa query (an LSP edit re-hashes only the edited
    // file). Builtin stubs are toolchain — covered by compiler_id.
    let root = db.project().root(db);
    let mut files: Vec<(String, [u8; 32])> = db
        .project()
        .files(db)
        .iter()
        .copied()
        .filter(|file| !file.path(db).to_string_lossy().starts_with("<builtin>/"))
        .map(|file| {
            let path = file.path(db);
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();
            (rel, file_blake3(db, file))
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));

    // baml.toml participates in the snapshot (§4.3) but is not a salsa
    // input today; read from disk at finalize time. Finalization happens
    // per materialized program (never per keystroke), so the read is cold,
    // and a manifest edit lands in the next compile's snapshot.
    let manifest_hash = std::fs::read(root.join("baml.toml"))
        .ok()
        .map(|bytes| *blake3::hash(&bytes).as_bytes());

    let snapshot = source_snapshot_id(&files, manifest_hash);
    let revision = revision_id(
        snapshot,
        bex_cache::compiler_id(),
        opt_level_discriminant(opt),
        emit_test_cases,
    );
    program.identity = Some(ProgramIdentity {
        revision_id: revision,
        source_snapshot_id: snapshot,
        compiler_id: bex_cache::compiler_id().to_string(),
        function_count,
    });
    program.source_files = files
        .into_iter()
        .map(|(path, content_hash)| SourceFileIdentity { path, content_hash })
        .collect();
}

/// Stable u8 for the revision hash (§4.3). Never reuse a value for a new
/// optimization level.
fn opt_level_discriminant(opt: OptLevel) -> u8 {
    match opt {
        OptLevel::Zero => 0,
        OptLevel::One => 1,
        OptLevel::Two => 2,
    }
}
