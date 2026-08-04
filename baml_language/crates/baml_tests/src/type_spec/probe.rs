//! Mismatch-channel probes (from the B-1136 sweep). The caret harness
//! cannot see `type_mismatches` yet, so behaviors that exist ONLY on
//! that channel are asserted here directly - Rust tests are justified
//! exactly because the behavior is not BAML-fixture-exercisable. The
//! S17 harness extension (fixture-level mismatch assertions) subsumes
//! these; fold them into fixtures when it lands.

use baml_compiler2_hir::body::BodyOwnerId;

fn mismatch_count(source: &str, function_name: &str) -> usize {
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.add_file("test.baml", source);
    for owner in baml_compiler2_ppir::file_body_owners(&db, file) {
        if let BodyOwnerId::Function(function) = owner {
            let name = baml_compiler2_ppir::item_data::function_data(&db, function)
                .name
                .clone();
            if name.as_str() == function_name {
                let result = baml_compiler2_hir_ty::infer::infer_body(&db, owner);
                return result.type_mismatches.len();
            }
        }
    }
    panic!("function {function_name} not found");
}

#[test]
fn b1082_rigid_var_throws_clause_still_checks() {
    // B-1082: a type variable in the clause must NOT disable the check -
    // `Kaboom` is undeclared (E does not cover it) and must record.
    let source = r#"
class Kaboom {
    m string
}

function f<E>(cb: () -> int throws E) -> int throws E {
    throw Kaboom { m: "x" };
    cb()
}
"#;
    assert_eq!(mismatch_count(source, "f"), 1, "Kaboom escape must record");
}

#[test]
fn b267_call_site_bound_violation_records() {
    // B-267.3's general mechanism: a declared bound the argument fails
    // must record (sort_by_key's `U extends Comparable` enforces through
    // the same path once the stdlib bound is consulted).
    let source = r#"
interface Need {
    function need(self) -> int throws never
}

function g<T extends Need>(x: T) -> T throws never {
    x
}

function h(n: int) -> int throws never {
    g(n)
}
"#;
    assert_eq!(mismatch_count(source, "h"), 1, "int fails `extends Need`");
}

#[test]
fn clean_program_records_no_mismatches() {
    // Control: the probe must not count phantom entries.
    let source = r#"
function ok(n: int) -> int throws never {
    n
}
"#;
    assert_eq!(mismatch_count(source, "ok"), 0);
}
