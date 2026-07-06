//! Emit determinism: identical inputs must produce byte-identical `Program`s.
//!
//! A content-addressed bytecode cache keys blobs by a hash of the inputs
//! (source contents, compiler version, options), so two compiles of the same
//! sources must serialize to the same borsh bytes — any nondeterminism
//! (e.g. `HashMap` iteration order leaking into an emitted table, or unstable
//! `FileId` assignment reaching serialized `Span`s) breaks the cache and this
//! test pinpoints it.

use std::path::{Path, PathBuf};

use baml_compiler2_emit::{CompileOptions, generate_project_bytecode};
use baml_project::ProjectDatabase;
use baml_workspace::discover_baml_files;

/// Read every `.baml` file under `root` into memory, in discovery order.
fn read_project(root: &Path) -> Vec<(PathBuf, String)> {
    discover_baml_files(root)
        .into_iter()
        .map(|path| {
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            (path, content)
        })
        .collect()
}

/// Build a fresh `ProjectDatabase` (mirroring CLI project loading) and compile
/// it to serialized bytecode.
fn compile_to_bytes(root: &Path, sources: &[(PathBuf, String)], emit_test_cases: bool) -> Vec<u8> {
    let mut db = ProjectDatabase::new();
    db.set_project_root(root);
    for (path, content) in sources {
        db.add_or_update_file(path, content);
    }
    let program = generate_project_bytecode(&db, &CompileOptions { emit_test_cases })
        .unwrap_or_else(|e| panic!("compilation of {} failed: {e:?}", root.display()));
    borsh::to_vec(&program).expect("borsh serialization failed")
}

/// Compile `root` twice on fresh databases and assert byte-identical output.
fn assert_deterministic(root: &Path, emit_test_cases: bool) {
    let sources = read_project(root);
    assert!(
        !sources.is_empty(),
        "no .baml files found under {}",
        root.display()
    );
    let first = compile_to_bytes(root, &sources, emit_test_cases);
    let second = compile_to_bytes(root, &sources, emit_test_cases);

    if first != second {
        let diff_at = first
            .iter()
            .zip(second.iter())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| first.len().min(second.len()));
        panic!(
            "emit is nondeterministic for {}: lengths {} vs {}, first difference at byte {} \
             (context: {:02x?} vs {:02x?})",
            root.display(),
            first.len(),
            second.len(),
            diff_at,
            &first[diff_at.saturating_sub(8)..(diff_at + 8).min(first.len())],
            &second[diff_at.saturating_sub(8)..(diff_at + 8).min(second.len())],
        );
    }
}

/// Fixed-cost baseline: stdlib-only project. Covers builtin lowering, the
/// empty-program emit path, and every stdlib-derived table.
#[test]
fn empty_project_emit_is_deterministic() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("projects/compiles/__baml_std__");
    assert_deterministic(&root, false);
}

/// Realistic multi-file workload: the full `baml_src/` test project exercises
/// classes, enums, interfaces, match tables, clients, and template strings.
/// `emit_test_cases: true` additionally covers the `test_cases` table.
#[test]
fn baml_src_project_emit_is_deterministic() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src");
    assert_deterministic(&root, true);
}
