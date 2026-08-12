//! Per-file relink oracle: reusing a clean file's compiled
//! unit from a previous compile's decomposed image must produce byte-identical
//! output to a full compile.
//!
//! Each scenario edits one file of a three-file project, full-compiles the
//! edited sources, and relinks the same sources routing through symbolic units:
//! clean files' units come verbatim from the *previous* compile's
//! `emit_units`, dirty files are emitted fresh, and `link(clean + fresh)` must
//! equal the full compile. Relocation is genuinely exercised — adding items
//! shifts `GlobalIndex`/`ObjectIndex` layouts, so a reused unit only matches the
//! full compile if the linker rewrote every cross-unit reference correctly.

mod common;

use std::collections::HashSet;

use baml_compiler2_emit::{
    CompileOptions, OptLevel, emit_units, generate_project_bytecode_with_reuse_units,
    generate_project_bytecode_with_stdlib, generate_stdlib_program, take_lowered_files,
};
use baml_project::ProjectDatabase;
use bex_vm_types::{CompilationUnit, Object, Program};
use common::{A_BAML, B_BAML, C_BAML, assert_programs_byte_identical, build_db};

const ROOT: &str = "/relink-oracle";

fn compile_full(files: &[(&str, &str)], base: &Program) -> Program {
    generate_project_bytecode_with_stdlib(
        &build_db(ROOT, files),
        &CompileOptions {
            emit_test_cases: false,
        },
        OptLevel::Two,
        base,
    )
    .expect("full compile failed")
}

/// The previous compile's symbolic image: the units the reuse path draws clean
/// files from (in the CLI this is what `plan_reuse` loads from the cache; here
/// it is produced in-process by `emit_units` over the previous sources).
fn prev_units(files: &[(&str, &str)]) -> Vec<CompilationUnit> {
    emit_units(
        &build_db(ROOT, files),
        &CompileOptions {
            emit_test_cases: false,
        },
        OptLevel::Two,
    )
    .expect("emit_units for previous compile failed")
}

fn relink(
    files: &[(&str, &str)],
    base: &Program,
    prev_units: &[CompilationUnit],
    clean: &[&str],
) -> Program {
    relink_with_db(build_db(ROOT, files), base, prev_units, clean)
}

/// Relink exactly as the CLI does: throw facts for the clean files are
/// extracted from the previous compile's database and seeded, so throw
/// inference never re-walks their bodies.
fn relink_seeded(
    files: &[(&str, &str)],
    prev_files: &[(&str, &str)],
    base: &Program,
    prev_units: &[CompilationUnit],
    clean: &[&str],
) -> Program {
    use baml_compiler2_hir_ty::throw_facts::file_throw_facts;
    let prev_db = build_db(ROOT, prev_files);
    let mut seeds = std::collections::BTreeMap::new();
    for sf in prev_db.get_source_files() {
        let path = sf.path(&prev_db).display().to_string();
        let rel = path.trim_start_matches(&format!("{ROOT}/")).to_string();
        if clean.contains(&rel.as_str()) {
            seeds.insert(path, file_throw_facts(&prev_db, sf).0.clone());
        }
    }
    let mut db = build_db(ROOT, files);
    db.set_seeded_throw_facts(seeds);
    relink_with_db(db, base, prev_units, clean)
}

fn relink_with_db(
    db: ProjectDatabase,
    base: &Program,
    prev_units: &[CompilationUnit],
    clean: &[&str],
) -> Program {
    let clean_files: HashSet<String> = clean.iter().map(ToString::to_string).collect();
    generate_project_bytecode_with_reuse_units(
        &db,
        &CompileOptions {
            emit_test_cases: false,
        },
        OptLevel::Two,
        base,
        prev_units,
        &clean_files,
    )
    .expect("relink failed")
}

fn assert_relink_matches(
    label: &str,
    edited: &[(&str, &str)],
    base: &Program,
    prev_units: &[CompilationUnit],
    clean: &[&str],
) {
    assert_programs_byte_identical(
        label,
        &compile_full(edited, base),
        &relink(edited, base, prev_units, clean),
    );
}

#[test]
fn relink_is_byte_identical_to_full_compile() {
    let files = [("a.baml", A_BAML), ("b.baml", B_BAML), ("c.baml", C_BAML)];
    let base = generate_stdlib_program(&build_db(ROOT, &files), OptLevel::Two).expect("stdlib");
    let prev = prev_units(&files);

    // 1. Body-only edit in b: a and c reuse with identical layout.
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

    // 4b. Edit c, whose FIRST function leads with a string literal: the
    //    literal object precedes c's first Function in the pool. The stored
    //    unit format keeps that literal inside c's own `code` bucket, so a
    //    reused b never absorbs it and dirty c re-interns its own.
    let c_body_edit = C_BAML.replace("make_point(3, 4)", "make_point(5, 6)");
    let edited = [
        ("a.baml", A_BAML),
        ("b.baml", B_BAML),
        ("c.baml", c_body_edit.as_str()),
    ];
    assert_relink_matches(
        "leading-literal dirty file",
        &edited,
        &base,
        &prev,
        &["a.baml", "b.baml"],
    );

    // 5. Deleted file: c disappears; a and b reuse into the smaller layout.
    let edited = [("a.baml", A_BAML), ("b.baml", B_BAML)];
    assert_relink_matches("deleted file", &edited, &base, &prev, &["a.baml", "b.baml"]);

    // 6. Body edit that changes INFERRED interface: `throws` is inferred
    //    from bodies, so this edit changes the transitive throws of c's
    //    main() (which calls magnitude_ish) even though c is untouched and
    //    no *written* signature changed. A naive reuse would keep c's stale
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
    let base = generate_stdlib_program(&build_db(ROOT, &files), OptLevel::Two).expect("stdlib");
    let prev = prev_units(&files);

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

/// An incremental compile lowers ONLY the dirty files.
/// `take_lowered_files` drains the per-thread record of files whose bodies hit
/// Pass 4 (MIR/bytecode lowering); after a reuse compile it must contain the
/// dirty file and none of the clean files.
#[test]
fn relink_lowers_only_dirty_files() {
    use baml_compiler2_emit::take_lowered_files;

    let files = [("a.baml", A_BAML), ("b.baml", B_BAML), ("c.baml", C_BAML)];
    let base = generate_stdlib_program(&build_db(ROOT, &files), OptLevel::Two).expect("stdlib");
    let prev = prev_units(&files);

    // Body-only edit in b; a and c stay clean.
    let b_body_edit = B_BAML.replace("v * factor", "factor * v");
    let edited = [
        ("a.baml", A_BAML),
        ("b.baml", b_body_edit.as_str()),
        ("c.baml", C_BAML),
    ];

    // Drain everything the setup (stdlib + prev_units) lowered, then relink.
    let _ = take_lowered_files();
    let _ = relink(&edited, &base, &prev, &["a.baml", "c.baml"]);
    let lowered = take_lowered_files();

    assert!(
        lowered.iter().any(|f| f == "b.baml"),
        "dirty b.baml must be lowered, got {lowered:?}"
    );
    assert!(
        !lowered.iter().any(|f| f == "a.baml" || f == "c.baml"),
        "clean files must NOT be lowered, got {lowered:?}"
    );
}

/// Assert an edit is handled fully INCREMENTALLY — the reuse path does NOT return
/// [`baml_compiler2_emit::LoweringError::ReuseUnsupported`] (no fallback to a full
/// compile), the result is byte-identical to a full compile, and only the `dirty`
/// files were MIR/bytecode-lowered (clean files' units — including their `let`
/// helpers and `$init_test` contributions — came from `prev_units`).
///
/// This is the design §9 R1/R2 gate: the three scenarios below (dirty `test`
/// block, dirty top-level `let`, dirty generic-function value shadowing a clean
/// owner) are handled incrementally and stay byte-identical to a full compile.
fn assert_incremental_matches(
    label: &str,
    edited: &[(&str, &str)],
    base: &Program,
    prev_units: &[CompilationUnit],
    clean: &[&str],
    dirty: &[&str],
) {
    let full = compile_full(edited, base);

    let clean_files: HashSet<String> = clean.iter().map(ToString::to_string).collect();
    let db = build_db(ROOT, edited);
    // Drain everything the setup (stdlib, prev_units, the full compile above)
    // lowered, so the counter reflects only the reuse call below.
    let _ = take_lowered_files();
    let reused = generate_project_bytecode_with_reuse_units(
        &db,
        &CompileOptions {
            emit_test_cases: false,
        },
        OptLevel::Two,
        base,
        prev_units,
        &clean_files,
    );
    let lowered = take_lowered_files();

    let reused = match reused {
        Ok(program) => program,
        Err(e) => panic!(
            "{label}: reuse fell back (ReuseUnsupported / error) instead of staying \
             incremental: {e}"
        ),
    };
    assert_programs_byte_identical(label, &full, &reused);

    for d in dirty {
        assert!(
            lowered.iter().any(|f| f == d),
            "{label}: dirty file `{d}` must be lowered, got {lowered:?}"
        );
    }
    for c in clean {
        assert!(
            !lowered.iter().any(|f| f == c),
            "{label}: clean file `{c}` must NOT be lowered, got {lowered:?}"
        );
    }
}

/// R2 (design §9): a dirty file with a top-level `test` block. Editing it changes
/// the per-package `$init_test` chainer tail; the reuse path resynthesizes it
/// incrementally and stays byte-identical.
#[test]
fn incremental_dirty_test_block() {
    const CLEAN: &str = r#"function clean_fn() -> int {
  41
}
"#;
    const DIRTY: &str = r#"function dirty_fn() -> int {
  1
}

test "t_dirty" {
  assert.equal(dirty_fn(), 1)
}
"#;
    let files = [("t_clean.baml", CLEAN), ("t_dirty.baml", DIRTY)];
    let base = generate_stdlib_program(&build_db(ROOT, &files), OptLevel::Two).expect("stdlib");
    let prev = prev_units(&files);

    let dirty_edit = DIRTY.replace("  1\n}", "  2\n}");
    assert_ne!(dirty_edit, DIRTY, "edit must apply");
    let edited = [
        ("t_clean.baml", CLEAN),
        ("t_dirty.baml", dirty_edit.as_str()),
    ];
    assert_incremental_matches(
        "dirty test block",
        &edited,
        &base,
        &prev,
        &["t_clean.baml"],
        &["t_dirty.baml"],
    );
}

/// R2 (design §9): a dirty file with a top-level `let` (client-like). Editing it
/// re-participates in the package `$init` synthesis, resynthesized incrementally.
#[test]
fn incremental_dirty_top_level_let() {
    const CLEAN: &str = r#"function clean_fn() -> int {
  7
}
"#;
    const DIRTY: &str = r#"let greeting = "hi";

function use_greeting() -> string {
  greeting
}
"#;
    let files = [("l_clean.baml", CLEAN), ("l_dirty.baml", DIRTY)];
    let base = generate_stdlib_program(&build_db(ROOT, &files), OptLevel::Two).expect("stdlib");
    let prev = prev_units(&files);

    let dirty_edit = DIRTY.replace("  greeting\n}", "  greeting\n  // edited\n}");
    assert_ne!(dirty_edit, DIRTY, "edit must apply");
    let edited = [
        ("l_clean.baml", CLEAN),
        ("l_dirty.baml", dirty_edit.as_str()),
    ];
    assert_incremental_matches(
        "dirty top-level let",
        &edited,
        &base,
        &prev,
        &["l_clean.baml"],
        &["l_dirty.baml"],
    );
}

/// R1 (design §9): a dirty file emits a pooled generic-function *value*
/// (`ident<int>`) that a *clean* file also uses. The clean file is the
/// first-referencer, so it owns the canonical pooled object; the dirty-only emit
/// re-interns a local copy. The linker must dedup them (keep the clean owner,
/// redirect the dirty reference) to stay byte-identical.
#[test]
fn incremental_dirty_generic_value_shadows_clean() {
    // File order (sorted): gen_a_base < gen_b_use < gen_c_use. `gen_b_use` is the
    // first VALUE-referencer of `ident<int>` → canonical owner; `gen_c_use` (dirty)
    // re-emits a shadow copy the linker must fold into the canonical.
    const BASE: &str = r#"function ident<T>(x: T) -> T {
  x
}
"#;
    const USE_A: &str = r#"function use_a() -> int {
  let f = ident<int>;
  f(1)
}
"#;
    const USE_B: &str = r#"function use_b() -> int {
  let f = ident<int>;
  f(2)
}
"#;
    let files = [
        ("gen_a_base.baml", BASE),
        ("gen_b_use.baml", USE_A),
        ("gen_c_use.baml", USE_B),
    ];
    let base = generate_stdlib_program(&build_db(ROOT, &files), OptLevel::Two).expect("stdlib");
    let prev = prev_units(&files);

    let dirty_edit = USE_B.replace("  f(2)", "  f(2) + 0");
    assert_ne!(dirty_edit, USE_B, "edit must apply");
    let edited = [
        ("gen_a_base.baml", BASE),
        ("gen_b_use.baml", USE_A),
        ("gen_c_use.baml", dirty_edit.as_str()),
    ];
    assert_incremental_matches(
        "dirty generic value shadows clean",
        &edited,
        &base,
        &prev,
        &["gen_a_base.baml", "gen_b_use.baml"],
        &["gen_c_use.baml"],
    );
}

/// Prove the reuse path actually draws the clean file from `prev_units`
/// (byte-equality alone would also pass if "clean" files were silently
/// recompiled): plant a probe in a cosmetic, serialized field of the previous
/// image's clean-file function and find it in the relink output.
#[test]
fn relink_actually_reuses_prev_unit() {
    let files = [("a.baml", A_BAML), ("b.baml", B_BAML), ("c.baml", C_BAML)];
    let base = generate_stdlib_program(&build_db(ROOT, &files), OptLevel::Two).expect("stdlib");
    let mut prev = prev_units(&files);

    // Plant the probe inside b's unit — the `code` bucket entry that compiled
    // `user.magnitude_ish`. It survives to the linked output only if b's unit
    // is reused verbatim, not recompiled.
    let mut planted = false;
    for unit in &mut prev {
        if unit.source_file != "b.baml" {
            continue;
        }
        for obj in &mut unit.code {
            if let Object::Function(function) = obj
                && function.name == "user.magnitude_ish"
            {
                function.display_return_type = "PROBE_FROM_PREV".to_string();
                planted = true;
            }
        }
    }
    assert!(planted, "failed to plant probe in b's unit");

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
        "clean file was recompiled instead of reused from prev_units"
    );
}
