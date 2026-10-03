//! Emit determinism: identical inputs must produce byte-identical `Program`s.
//!
//! A content-addressed bytecode cache keys blobs by a hash of the inputs
//! (source contents, compiler version, options), so two compiles of the same
//! sources must serialize to the same borsh bytes — any nondeterminism
//! (e.g. `HashMap` iteration order leaking into an emitted table, or unstable
//! `FileId` assignment reaching serialized `Span`s) breaks the cache and this
//! test pinpoints it.

use std::path::{Path, PathBuf};

use baml_compiler2_emit::OptLevel;
use baml_db::{ProjectDatabase, compile_program, discover_baml_files};
use baml_tests::engine::TestDbExt;
use bex_vm_types::RuntimeCompileRequest;

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
fn compile_to_bytes(root: &Path, sources: &[(PathBuf, String)]) -> Vec<u8> {
    let mut db = ProjectDatabase::new();
    let package = db.workspace(root);
    for (path, content) in sources {
        db.file(path, content);
    }
    let program = compile_program(&db, package, OptLevel::Two)
        .unwrap_or_else(|e| panic!("compilation of {} failed: {e:?}", root.display()));
    borsh::to_vec(&program).expect("borsh serialization failed")
}

/// Compile `root` twice on fresh databases and assert byte-identical output.
fn assert_deterministic(root: &Path) {
    let sources = read_project(root);
    assert!(
        !sources.is_empty(),
        "no .baml files found under {}",
        root.display()
    );
    let first = compile_to_bytes(root, &sources);
    let second = compile_to_bytes(root, &sources);

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
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("projects/empty");
    assert_deterministic(&root);
}

// Whole-corpus determinism and serial/parallel comparisons live in
// `package_emit_oracle`: it compares both linked programs and package artifacts
// from each fresh database, without repeating the same corpus compilation.

/// A package unit with every database-local debug `FileId` normalized: the
/// two lanes number `main.baml` differently, and nothing else may differ.
fn normalize_package_unit(
    mut unit: baml_linker_types::CompilationUnit,
) -> baml_linker_types::CompilationUnit {
    for object in &mut unit.code {
        let bex_vm_types::Object::Function(function) = object else {
            continue;
        };
        let normalized = baml_base::FileId::new(0);
        function.span.file_id = normalized;
        for entry in &mut function.bytecode.line_table {
            entry.span.file_id = normalized;
        }
        for local in &mut function.debug_locals {
            local.scope_span.file_id = normalized;
        }
    }
    unit
}

/// The runtime compiler — its stdlib served from the embedded interface
/// blobs — emits the same package unit as a full-source compile of the same
/// package.
#[test]
fn package_compile_prefix_artifact_is_byte_identical_to_full_compile() {
    let source = r#"
class RuntimeValue {
  values string[]
}

function count(value: RuntimeValue) -> int throws never {
  baml.Array.length(value.values)
}
"#;
    let prefix_artifact = bex_project::runtime_compiler()
        .compile(RuntimeCompileRequest {
            files: [("main.baml".to_string(), source.to_string())]
                .into_iter()
                .collect(),
            ..RuntimeCompileRequest::default()
        })
        .expect("compile Package artifact from the embedded stdlib prefix");

    let root = Path::new("<runtime>");
    let mut full_db = ProjectDatabase::new();
    let full_package = full_db.workspace(root);
    full_db.file(root.join("main.baml"), source);
    let full = baml_compiler2_emit::emit_package(&full_db, full_package, OptLevel::One)
        .expect("compile Package artifact from full stdlib sources");

    assert_eq!(
        borsh::to_vec(&normalize_package_unit(prefix_artifact.emitted.unit))
            .expect("serialize normalized prefix unit"),
        borsh::to_vec(&normalize_package_unit(full.unit))
            .expect("serialize normalized full-compile unit"),
        "Package.compile artifact differs from the full-source compile",
    );
}
