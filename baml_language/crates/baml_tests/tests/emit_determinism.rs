//! Emit determinism: identical inputs must produce byte-identical `Program`s.
//!
//! A content-addressed bytecode cache keys blobs by a hash of the inputs
//! (source contents, compiler version, options), so two compiles of the same
//! sources must serialize to the same borsh bytes — any nondeterminism
//! (e.g. `HashMap` iteration order leaking into an emitted table, or unstable
//! `FileId` assignment reaching serialized `Span`s) breaks the cache and this
//! test pinpoints it.

use std::path::{Path, PathBuf};

use baml_compiler2_emit::{
    CompileOptions, OptLevel, emit_units, generate_project_bytecode,
    generate_project_bytecode_with_stdlib, generate_stdlib_program,
};
use baml_db::{ProjectDatabase, discover_baml_files};
use baml_tests::engine::TestDbExt;
use bex_vm_types::{CompilationUnit, RuntimeCompileRequest};

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
    db.workspace(root);
    for (path, content) in sources {
        db.file(path, content);
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

/// The default (parallel) emit must produce byte-identical output to the
/// serial reference pass: parallel emit compiles each function into a
/// watermark-based fragment pool and the serial merge replays the exact
/// serial pool layout (including cross-function `GenericFunction` interning,
/// which the `ns_instantiation_expr` fixtures in this corpus exercise).
/// The serial path is selected the same way a user would get it — a
/// single-threaded rayon pool (`RAYON_NUM_THREADS=1`).
#[test]
fn parallel_emit_is_byte_identical_to_serial() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src");
    let sources = read_project(&root);
    // Both paths are pinned to explicit pools so the test exercises what it
    // claims regardless of the ambient pool's size: 1 thread forces the serial
    // reference emitter, >1 forces `emit_functions_parallel`. (On a single-core
    // CI runner the default pool is 1 thread, which would otherwise make the
    // "parallel" call silently take the serial path and test nothing.)
    let run_with = |threads: usize| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("build rayon pool")
            .install(|| compile_to_bytes(&root, &sources, true))
    };
    let serial = run_with(1);
    let parallel = run_with(4);

    if serial != parallel {
        let diff_at = serial
            .iter()
            .zip(parallel.iter())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| serial.len().min(parallel.len()));
        panic!(
            "parallel emit diverges from serial for {}: lengths {} vs {}, first difference at \
             byte {} (context: {:02x?} vs {:02x?})",
            root.display(),
            serial.len(),
            parallel.len(),
            diff_at,
            &serial[diff_at.saturating_sub(8)..(diff_at + 8).min(serial.len())],
            &parallel[diff_at.saturating_sub(8)..(diff_at + 8).min(parallel.len())],
        );
    }
}

fn build_db(root: &Path, sources: &[(PathBuf, String)]) -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.workspace(root);
    for (path, content) in sources {
        db.file(path, content);
    }
    db
}

/// The precompiled-stdlib splice oracle: compiling on top of a stdlib
/// `Program` slice must be byte-identical to a full compile.
///
/// The base is built from the *empty* project's database and spliced into the
/// *full baml_src* compile — proving the stdlib slice is genuinely
/// user-independent (same bytes regardless of which project's db produced
/// it), not merely reusable within one project.
#[test]
fn stdlib_splice_is_byte_identical_to_full_compile() {
    let empty_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("projects/compiles/__baml_std__");
    let empty_sources = read_project(&empty_root);
    let base = generate_stdlib_program(&build_db(&empty_root, &empty_sources), OptLevel::Two)
        .expect("stdlib compile failed");
    let base_bytes = borsh::to_vec(&base).expect("serialize stdlib base");

    for (root, emit_test_cases) in [
        (empty_root.clone(), false),
        (Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src"), true),
    ] {
        let sources = read_project(&root);
        let full = compile_to_bytes(&root, &sources, emit_test_cases);

        let spliced = generate_project_bytecode_with_stdlib(
            &build_db(&root, &sources),
            &CompileOptions { emit_test_cases },
            OptLevel::Two,
            &base,
        )
        .unwrap_or_else(|e| panic!("splice compile of {} failed: {e:?}", root.display()));
        let spliced = borsh::to_vec(&spliced).expect("serialize spliced program");

        assert_eq!(
            full.len(),
            spliced.len(),
            "splice output length differs from full compile for {}",
            root.display()
        );
        assert!(
            full == spliced,
            "splice output differs from full compile for {} (first diff at byte {})",
            root.display(),
            full.iter()
                .zip(spliced.iter())
                .position(|(a, b)| a != b)
                .unwrap_or(0),
        );

        // The stdlib slice must also be reproducible from THIS project's db.
        let rebuilt = generate_stdlib_program(&build_db(&root, &sources), OptLevel::Two)
            .expect("stdlib recompile failed");
        let rebuilt = borsh::to_vec(&rebuilt).expect("serialize rebuilt base");
        assert!(
            rebuilt == base_bytes,
            "stdlib slice is not user-independent: differs when built from {}",
            root.display()
        );
    }
}

fn normalize_unit_file_ids(mut unit: CompilationUnit) -> CompilationUnit {
    let normalize_object = |object: &mut bex_vm_types::Object| {
        let bex_vm_types::Object::Function(function) = object else {
            return;
        };
        let normalized = baml_base::FileId::new(0);
        function.span.file_id = normalized;
        for entry in &mut function.bytecode.line_table {
            entry.span.file_id = normalized;
        }
        for local in &mut function.debug_locals {
            local.scope_span.file_id = normalized;
        }
    };
    for object in &mut unit.code {
        normalize_object(object);
    }
    if let Some(tail) = &mut unit.init_tail {
        for object in &mut tail.objects {
            normalize_object(object);
        }
    }
    unit
}

/// Exercise the actual source-less-stdlib runtime compiler branch and compare
/// its Package.compile artifact with the legacy full-source compile oracle.
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
    full_db.workspace(root);
    full_db.file(root.join("main.baml"), source);
    let full_user_units = emit_units(
        &full_db,
        &CompileOptions {
            emit_test_cases: false,
        },
        OptLevel::One,
    )
    .expect("compile Package artifact from full stdlib sources")
    .into_iter()
    .filter(|unit| unit.package.as_str() == "user")
    .collect::<Vec<_>>();

    assert_eq!(
        prefix_artifact.units.len(),
        1,
        "expected one prefix user unit"
    );
    assert_eq!(
        full_user_units.len(),
        1,
        "expected one full-compile user unit"
    );
    assert_eq!(
        borsh::to_vec(&normalize_unit_file_ids(prefix_artifact.units[0].clone()))
            .expect("serialize normalized prefix unit"),
        borsh::to_vec(&normalize_unit_file_ids(full_user_units[0].clone()))
            .expect("serialize normalized full-compile unit"),
        "Package.compile prefix artifact differs from the old full-source path",
    );
}
