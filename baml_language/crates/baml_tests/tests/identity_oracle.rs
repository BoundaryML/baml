//! Compile-time identity oracle (observability design §4).
//!
//! Pins the §4.4 golden property — *"edit an unrelated file ⇒ all other
//! `def_content_hash`es byte-identical"* — plus the finalizer contract:
//! programs from public emit entries carry `ProgramIdentity`, dense ids
//! from `FIRST_POOL_FUNCTION_ID`, and a revision dictionary whose reserved
//! rows come first.

use std::path::Path;

use baml_compiler2_emit::{CompileOptions, generate_project_bytecode};
use baml_project::ProjectDatabase;
use bex_events::dict::build_revision_dictionary;
use bex_vm_types::{FIRST_POOL_FUNCTION_ID, Program};

const FILE_A: &str = r#"
function alpha_leaf(x: int) -> int {
    x * 2
}

function alpha_caller(x: int) -> int {
    alpha_leaf(x) + shared_helper(x)
}

function shared_helper(x: int) -> int {
    x + 1
}
"#;

const FILE_B_V1: &str = r#"
function beta_one(x: int) -> int {
    shared_helper(x) * 3
}
"#;

// The "unrelated edit": beta gains a function and changes a body. Nothing
// in FILE_A changed.
const FILE_B_V2: &str = r#"
function beta_one(x: int) -> int {
    shared_helper(x) * 4
}

function beta_two(x: int) -> int {
    beta_one(x) + 10
}
"#;

fn compile(root: &Path, file_b: &str) -> Program {
    let mut db = ProjectDatabase::new();
    db.set_project_root(root);
    db.add_or_update_file(&root.join("a.baml"), FILE_A);
    db.add_or_update_file(&root.join("b.baml"), file_b);
    generate_project_bytecode(
        &db,
        &CompileOptions {
            emit_test_cases: false,
        },
    )
    .expect("identity oracle project compiles")
}

fn hashes_by_key(program: &Program) -> std::collections::HashMap<String, Vec<u8>> {
    let dict = build_revision_dictionary(program).expect("finalized program has a dictionary");
    dict.functions
        .as_ref()
        .unwrap()
        .functions
        .iter()
        .filter(|row| !row.definition_key.is_empty())
        .map(|row| (row.definition_key.clone(), row.def_content_hash.clone()))
        .collect()
}

#[test]
fn unrelated_edit_keeps_other_def_content_hashes_byte_identical() {
    let root = std::env::temp_dir().join(format!("baml-idoracle-{}", std::process::id()));
    let before = compile(&root, FILE_B_V1);
    let after = compile(&root, FILE_B_V2);

    // Identity is present and revision-level ids moved with the edit.
    let id_before = before.identity.as_ref().expect("finalized");
    let id_after = after.identity.as_ref().expect("finalized");
    assert_ne!(
        id_before.revision_id, id_after.revision_id,
        "an edit must mint a new revision"
    );
    assert_ne!(
        id_before.source_snapshot_id, id_after.source_snapshot_id,
        "an edit must change the source snapshot"
    );

    let hashes_before = hashes_by_key(&before);
    let hashes_after = hashes_by_key(&after);

    // THE §4.4 golden property: every definition outside b.baml hashes
    // byte-identically across the unrelated edit — even though the edit
    // shifted whole-program pool and global indices.
    for key in [
        "function:user.alpha_leaf",
        "function:user.alpha_caller",
        "function:user.shared_helper",
    ] {
        let before_hash = hashes_before
            .get(key)
            .unwrap_or_else(|| panic!("{key} missing before"));
        let after_hash = hashes_after
            .get(key)
            .unwrap_or_else(|| panic!("{key} missing after"));
        assert_eq!(
            before_hash, after_hash,
            "{key}: unrelated edit must not churn def_content_hash"
        );
        assert_eq!(before_hash.len(), 32, "{key}: BLAKE3-256 hash");
    }

    // The edited definition's hash MUST change (body changed).
    assert_ne!(
        hashes_before.get("function:user.beta_one").unwrap(),
        hashes_after.get("function:user.beta_one").unwrap(),
        "beta_one's body changed — its hash must change"
    );
    assert!(
        hashes_after.contains_key("function:user.beta_two"),
        "new function appears in the dictionary"
    );
}

#[test]
fn finalized_programs_carry_dense_ids_and_reserved_dictionary_rows() {
    let root = std::env::temp_dir().join(format!("baml-idoracle2-{}", std::process::id()));
    let program = compile(&root, FILE_B_V1);

    // Dense pool ids from the reserved-range ceiling.
    let mut expected = FIRST_POOL_FUNCTION_ID;
    for obj in program.objects.iter() {
        if let bex_vm_types::types::Object::Function(func) = obj {
            assert_eq!(
                func.function_id, expected,
                "dense pool order for {}",
                func.name
            );
            expected += 1;
        }
    }
    assert_eq!(
        program.identity.as_ref().unwrap().function_count,
        expected - FIRST_POOL_FUNCTION_ID
    );

    // Dictionary: reserved rows first, then pool rows; source files hashed.
    let dict = build_revision_dictionary(&program).unwrap();
    let rows = &dict.functions.as_ref().unwrap().functions;
    assert_eq!(rows[0].function_id, 0);
    assert_eq!(rows[1].function_id, 1);
    assert_eq!(rows[2].function_id, FIRST_POOL_FUNCTION_ID);
    assert!(
        !dict.identity.as_ref().unwrap().fallback_identity,
        "compiler-finalized programs carry real source identity"
    );
    let files = &dict.files.as_ref().unwrap().files;
    assert!(
        files
            .iter()
            .any(|f| f.path.ends_with("a.baml") && f.content_hash.len() == 32),
        "user files carry BLAKE3 content hashes: {files:?}"
    );
}
