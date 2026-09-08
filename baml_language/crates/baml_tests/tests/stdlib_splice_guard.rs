//! The precompiled-stdlib splice pairs each builtin declaration with a pooled
//! object by ORDINAL replay of Pass 1's enumeration. That is only sound while
//! this compiler and the compiler that produced the artifact enumerate
//! identically, so the replay verifies every pairing by the one spelling both
//! derive from the declaration — and refuses a base whose objects sit at the
//! wrong slots instead of silently mapping every later declaration onto the
//! wrong function (and then overwriting correct spliced defaults with the
//! interface-default backfill).

mod common;

use baml_compiler2_emit::{
    LoweringError, OptLevel, generate_project_bytecode_with_stdlib, generate_stdlib_program,
};
use bex_vm_types::{ConstValue, Object};
use common::build_db;

const ROOT: &str = "/stdlib-splice-guard";

/// The fixture's workspace root — the splice emits from a viewing package.
fn package(db: &baml_db::ProjectDatabase) -> baml_db::SourceRoot {
    db.workspace_root()
        .unwrap_or_else(|| unreachable!("the fixture builder adds one workspace root"))
}

#[test]
fn splice_rejects_a_base_whose_slot_holds_the_wrong_declaration() {
    let files = [(
        "main.baml",
        "function forty_two() -> int throws never {\n  42\n}\n",
    )];
    let db = build_db(ROOT, &files);
    let mut base = generate_stdlib_program(&db, OptLevel::Two).expect("stdlib compiles");
    // Positive control: the genuine base splices.
    generate_project_bytecode_with_stdlib(&db, package(&db), OptLevel::Two, &base)
        .expect("the genuine stdlib base splices");

    // Corrupt the artifact: the object behind the FIRST builtin slot now
    // claims to be a different declaration — the shape a stale artifact that
    // passed the header checks, or a skip-set drift between builds, produces.
    let ConstValue::Object(first) = base.globals[0] else {
        panic!("the first builtin slot holds a function object");
    };
    let Object::Function(function) = &mut base.objects[first] else {
        panic!("the first builtin slot's object is a function");
    };
    let genuine = function.name.clone();
    let corrupted = format!("{genuine}$not_this_declaration");
    function.name = corrupted.clone();

    match generate_project_bytecode_with_stdlib(&db, package(&db), OptLevel::Two, &base) {
        // The corrupted spelling CONTAINS the genuine one, so a bare
        // `contains(&genuine)` would also pass on a message that named only
        // what it FOUND. Pin each name in its own position instead.
        Err(LoweringError::Internal(message)) => assert!(
            message.contains("declaration order disagrees")
                && message.contains(&format!("enumerates `{genuine}`"))
                && message.contains(&format!("holds `{corrupted}`")),
            "the rejection must name the enumeration skew, the expected declaration, \
             and the one it found; got: {message}"
        ),
        Ok(_) => panic!("a base whose slot holds the wrong declaration must not splice"),
        Err(other) => panic!("expected an internal lowering error, got {other:?}"),
    }
}
