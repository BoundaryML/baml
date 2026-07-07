//! Per-file relink oracle: reusing a clean file's compiled bytecode from a
//! previous `Program` must produce byte-identical output to a full compile.
//!
//! Each scenario edits one file of a three-file project, full-compiles the
//! edited sources, and relinks the same sources against the *previous*
//! program with the unchanged files marked clean. Relocation is genuinely
//! exercised: adding items shifts `GlobalIndex`/`ObjectIndex` layouts, so
//! spliced functions only match the full compile if every cross-function
//! reference was correctly rewritten.

use std::{collections::HashSet, path::Path};

use baml_compiler2_emit::{
    CompileOptions, OptLevel, generate_project_bytecode_with_reuse,
    generate_project_bytecode_with_stdlib, generate_stdlib_program,
};
use baml_project::ProjectDatabase;
use bex_vm_types::{Object, Program};

const ROOT: &str = "/relink-oracle";

const A_BAML: &str = r#"class Point {
  x int
  y int
}

function make_point(x: int, y: int) -> Point {
  Point { x: x, y: y }
}

function origin() -> Point {
  make_point(0, 0)
}
"#;

const B_BAML: &str = r#"function scale(p: Point, factor: int) -> Point {
  let mul = (v: int) -> int { v * factor }
  Point { x: mul(p.x), y: mul(p.y) }
}

function magnitude_ish(p: Point) -> int {
  p.x * p.x + p.y * p.y
}
"#;

const C_BAML: &str = r#"function main() -> int {
  let p = make_point(3, 4);
  let doubled = scale(p, 2);
  magnitude_ish(doubled)
}
"#;

fn build_db(files: &[(&str, &str)]) -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new(ROOT));
    for (name, content) in files {
        db.add_or_update_file(&Path::new(ROOT).join(name), content);
    }
    db
}

fn compile_full(files: &[(&str, &str)], base: &Program) -> Program {
    generate_project_bytecode_with_stdlib(
        &build_db(files),
        &CompileOptions {
            emit_test_cases: false,
        },
        OptLevel::Two,
        base,
    )
    .expect("full compile failed")
}

fn relink(files: &[(&str, &str)], base: &Program, prev: &Program, clean: &[&str]) -> Program {
    relink_with_db(build_db(files), base, prev, clean)
}

/// Relink exactly as the CLI does: throw facts for the clean files are
/// extracted from the previous compile's database and seeded, so throw
/// inference never re-walks their bodies.
fn relink_seeded(
    files: &[(&str, &str)],
    prev_files: &[(&str, &str)],
    base: &Program,
    prev: &Program,
    clean: &[&str],
) -> Program {
    use baml_compiler2_tir::throw_inference::file_throw_facts;
    let prev_db = build_db(prev_files);
    let mut seeds = std::collections::BTreeMap::new();
    for sf in prev_db.get_source_files() {
        let path = sf.path(&prev_db).display().to_string();
        let rel = path.trim_start_matches(&format!("{ROOT}/")).to_string();
        if clean.contains(&rel.as_str()) {
            seeds.insert(path, file_throw_facts(&prev_db, sf).0.clone());
        }
    }
    let mut db = build_db(files);
    db.set_seeded_throw_facts(seeds);
    relink_with_db(db, base, prev, clean)
}

fn relink_with_db(db: ProjectDatabase, base: &Program, prev: &Program, clean: &[&str]) -> Program {
    let clean_files: HashSet<String> = clean.iter().map(ToString::to_string).collect();
    generate_project_bytecode_with_reuse(
        &db,
        &CompileOptions {
            emit_test_cases: false,
        },
        OptLevel::Two,
        base,
        prev,
        &clean_files,
    )
    .expect("relink failed")
}

fn assert_relink_matches(
    label: &str,
    edited: &[(&str, &str)],
    base: &Program,
    prev: &Program,
    clean: &[&str],
) {
    let full = borsh::to_vec(&compile_full(edited, base)).expect("serialize full");
    let relinked = borsh::to_vec(&relink(edited, base, prev, clean)).expect("serialize relink");
    assert!(
        full == relinked,
        "{label}: relink differs from full compile (lengths {} vs {}, first diff at {:?})",
        full.len(),
        relinked.len(),
        full.iter().zip(relinked.iter()).position(|(a, b)| a != b),
    );
}

#[test]
fn relink_is_byte_identical_to_full_compile() {
    let files = [("a.baml", A_BAML), ("b.baml", B_BAML), ("c.baml", C_BAML)];
    let base = generate_stdlib_program(&build_db(&files), OptLevel::Two).expect("stdlib");
    let prev = compile_full(&files, &base);

    // 1. Body-only edit in b: a and c splice with identical layout.
    let b_body_edit = B_BAML.replace("v * factor", "factor * v");
    let edited = [
        ("a.baml", A_BAML),
        ("b.baml", b_body_edit.as_str()),
        ("c.baml", C_BAML),
    ];
    assert_relink_matches("body edit", &edited, &base, &prev, &["a.baml", "c.baml"]);

    // 2. Added function in b: global slots and object ranges shift; c's
    //    calls into a and b relocate across the shift.
    let b_added_fn = format!("{B_BAML}\nfunction extra(v: int) -> int {{\n  v + 1\n}}\n");
    let edited = [
        ("a.baml", A_BAML),
        ("b.baml", b_added_fn.as_str()),
        ("c.baml", C_BAML),
    ];
    assert_relink_matches(
        "added function",
        &edited,
        &base,
        &prev,
        &["a.baml", "c.baml"],
    );

    // 3. Added class in a: every user class/enum/function object shifts;
    //    b and c's AllocInstance references translate by class name.
    let a_added_class = format!("class Extra {{\n  z int\n}}\n\n{A_BAML}");
    let edited = [
        ("a.baml", a_added_class.as_str()),
        ("b.baml", B_BAML),
        ("c.baml", C_BAML),
    ];
    assert_relink_matches("added class", &edited, &base, &prev, &["b.baml", "c.baml"]);

    // 4. Lambda added to a body in b: b recompiles with a larger object
    //    range, shifting c's function objects.
    let b_added_lambda = B_BAML.replace(
        "p.x * p.x + p.y * p.y",
        "let sq = (v: int) -> int { v * v }\n  sq(p.x) + sq(p.y)",
    );
    let edited = [
        ("a.baml", A_BAML),
        ("b.baml", b_added_lambda.as_str()),
        ("c.baml", C_BAML),
    ];
    assert_relink_matches("added lambda", &edited, &base, &prev, &["a.baml", "c.baml"]);

    // 5. Deleted file: c disappears; a and b splice into the smaller layout.
    let edited = [("a.baml", A_BAML), ("b.baml", B_BAML)];
    assert_relink_matches("deleted file", &edited, &base, &prev, &["a.baml", "b.baml"]);

    // 6. Body edit that changes INFERRED interface: `throws` is inferred
    //    from bodies, so this edit changes the transitive throws of c's
    //    main() (which calls magnitude_ish) even though c is untouched and
    //    no *written* signature changed. A naive splice would keep c's stale
    //    throws metadata; the relink must detect the mismatch and demote c.
    let b_added_throw = B_BAML.replace(
        "p.x * p.x + p.y * p.y",
        "if p.x == 999 {\n    throw \"boom\"\n  }\n  p.x * p.x + p.y * p.y",
    );
    assert_ne!(b_added_throw, B_BAML, "throw edit must apply");
    let edited = [
        ("a.baml", A_BAML),
        ("b.baml", b_added_throw.as_str()),
        ("c.baml", C_BAML),
    ];
    assert_relink_matches(
        "throws-changing body edit",
        &edited,
        &base,
        &prev,
        &["a.baml", "c.baml"],
    );
}

/// The CLI's real configuration: clean files' throw facts seeded from the
/// previous compile. The throws-changing edit is the sharp scenario — the
/// dirty file's facts are extracted fresh, the seeded solve must still
/// propagate the new transitive throws into the clean caller, and the gate
/// must demote it for an honest recompile. Byte-identity proves the seeded
/// solve is indistinguishable from walking every body.
#[test]
fn seeded_relink_is_byte_identical_to_full_compile() {
    let files = [("a.baml", A_BAML), ("b.baml", B_BAML), ("c.baml", C_BAML)];
    let base = generate_stdlib_program(&build_db(&files), OptLevel::Two).expect("stdlib");
    let prev = compile_full(&files, &base);

    // Body edit.
    let b_body_edit = B_BAML.replace("v * factor", "factor * v");
    let edited = [
        ("a.baml", A_BAML),
        ("b.baml", b_body_edit.as_str()),
        ("c.baml", C_BAML),
    ];
    let full = borsh::to_vec(&compile_full(&edited, &base)).expect("serialize");
    let seeded = borsh::to_vec(&relink_seeded(
        &edited,
        &files,
        &base,
        &prev,
        &["a.baml", "c.baml"],
    ))
    .expect("serialize");
    assert!(full == seeded, "seeded relink differs (body edit)");

    // Throws-changing body edit: c (clean, seeded) transitively gains the
    // new throw through the solve and must be demoted by the gate.
    let b_added_throw = B_BAML.replace(
        "p.x * p.x + p.y * p.y",
        "if p.x == 999 {\n    throw \"boom\"\n  }\n  p.x * p.x + p.y * p.y",
    );
    let edited = [
        ("a.baml", A_BAML),
        ("b.baml", b_added_throw.as_str()),
        ("c.baml", C_BAML),
    ];
    let full = borsh::to_vec(&compile_full(&edited, &base)).expect("serialize");
    let seeded = borsh::to_vec(&relink_seeded(
        &edited,
        &files,
        &base,
        &prev,
        &["a.baml", "c.baml"],
    ))
    .expect("serialize");
    assert!(full == seeded, "seeded relink differs (throws edit)");
}

/// Prove the splice path actually runs (byte-equality alone would also pass
/// if "clean" files were silently recompiled): plant a probe in a cosmetic,
/// serialized field of the previous program's clean-file function and find
/// it in the relink output.
#[test]
fn relink_actually_splices_from_prev() {
    let files = [("a.baml", A_BAML), ("b.baml", B_BAML), ("c.baml", C_BAML)];
    let base = generate_stdlib_program(&build_db(&files), OptLevel::Two).expect("stdlib");
    let mut prev = compile_full(&files, &base);

    let idx = prev.function_indices["user.magnitude_ish"];
    let Some(Object::Function(function)) = prev.objects.get_mut(idx) else {
        panic!("user.magnitude_ish not found");
    };
    function.display_return_type = "PROBE_FROM_PREV".to_string();

    // Edit c so that b (home of the probed function) stays clean.
    let c_body_edit = C_BAML.replace("scale(p, 2)", "scale(p, 3)");
    let edited = [
        ("a.baml", A_BAML),
        ("b.baml", B_BAML),
        ("c.baml", c_body_edit.as_str()),
    ];
    let relinked = relink(&edited, &base, &prev, &["a.baml", "b.baml"]);
    let idx = relinked.function_indices["user.magnitude_ish"];
    let Some(Object::Function(function)) = relinked.objects.get(idx) else {
        panic!("user.magnitude_ish not found in relinked program");
    };
    assert_eq!(
        function.display_return_type, "PROBE_FROM_PREV",
        "clean file was recompiled instead of spliced"
    );
}
