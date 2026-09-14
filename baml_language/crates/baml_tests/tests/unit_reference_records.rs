//! Emit-recorded reference edges (`CompilationUnit::referenced_names` /
//! `bakes_type_layout`): every reference codegen statically resolves must be
//! recorded on the resolving file's unit, at the last-segment name grain the
//! CLI's dirty partition matches against `defined_names`.
//!
//! The records are produced AT the resolution sites — never reconstructed
//! from compiled operands joined against the runtime name maps. The pins
//! below cover exactly the reference shapes that reconstruction lost or
//! would lose again:
//!
//! - a direct call to an interface-machinery body (the `default.<method>()`
//!   bypass) — bodies own global slots but no runtime name, so the old
//!   operand reversal produced NO edge and a change to the interface's
//!   default body never dirtied the impl file that calls it;
//! - a function referenced as a VALUE — the pooled `GenericFunction` wrapper
//!   is a `Function`-kind pool object, which the reversal skipped as "a
//!   file's own lambda", so `let f = other_fn` never edged to `other_fn`;
//! - the ordinary shapes (named calls, class construction, enum variants,
//!   lambda interiors) that must not regress while the production channel
//!   changes.
//!
//! Not fixture-able here: top-level `let` bindings are rejected in user
//! source (clients and synthesized bindings are the only inhabitants), so
//! the `Constant::GlobalItem` read lane and the `$init`-helper attribution
//! lane are covered by the recording code sharing the one codegen channel,
//! and by the determinism oracles that byte-compare units.

mod common;

use baml_compiler2_emit::{OptLevel, emit_units};
use bex_vm_types::CompilationUnit;
use common::build_db;

/// The fixture's workspace root — `emit_units` emits from a viewing package.
fn package(db: &baml_db::ProjectDatabase) -> baml_db::SourceRoot {
    db.workspace_root()
        .unwrap_or_else(|| unreachable!("the fixture builder adds one workspace root"))
}

const ROOT: &str = "/unit-reference-records";

/// Emit `files` and return the unit for `file`, first asserting the fixture
/// type-checks — emit on an error-laden fixture produces junk records, so a
/// broken fixture must fail HERE, not as a mysterious missing edge.
fn unit_for(files: &[(&str, &str)], file: &str) -> CompilationUnit {
    let db = build_db(ROOT, files);
    let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
    assert!(
        diagnostics.is_empty(),
        "fixture must type-check cleanly; got {diagnostics:#?}"
    );
    let units = emit_units(&db, package(&db), OptLevel::Two).expect("emit_units succeeds");
    units
        .iter()
        .find(|u| u.source_file == file)
        .unwrap_or_else(|| panic!("no unit for `{file}`"))
        .clone()
}

fn has(unit: &CompilationUnit, name: &str) -> bool {
    unit.referenced_names.iter().any(|n| n == name)
}

#[test]
fn named_call_class_and_enum_references_are_recorded() {
    let unit = unit_for(
        &[
            (
                "defs.baml",
                "class Point {\n  x int\n  y int\n}\n\
                 enum Color {\n  Red\n  Green\n}\n\
                 function mk() -> int {\n  7\n}\n",
            ),
            (
                "user.baml",
                "function pick() -> Color {\n  Color.Red\n}\n\
                 function build() -> Point {\n  \
                 Point { x: mk(), y: 0 }\n}\n",
            ),
        ],
        "user.baml",
    );
    for name in ["Point", "Color", "mk"] {
        assert!(
            has(&unit, name),
            "user.baml resolves `{name}` and must record it; got {:?}",
            unit.referenced_names
        );
    }
}

#[test]
fn interface_body_direct_reference_is_recorded() {
    // `default.speak()` compiles to a direct `Call` of the interface's
    // default body slot — a slot with NO runtime name. The edge must still
    // exist, spelled as the method's name (what the interface's declaring
    // file's `defined_names` produces), so a change to the default body
    // dirties the impl file that delegates to it.
    let unit = unit_for(
        &[
            (
                "iface.baml",
                "interface Speaker {\n  \
                 function speak(self) -> string throws never {\n    \"quiet\"\n  }\n}\n",
            ),
            (
                "impl.baml",
                "class Wrap {\n  implements Speaker {\n    \
                 function speak(self) -> string throws never {\n      \
                 \"wrapped \" + default.speak()\n    }\n  }\n}\n",
            ),
        ],
        "impl.baml",
    );
    assert!(
        has(&unit, "speak"),
        "the `default.speak()` bypass direct-calls the interface body and must \
         record `speak`; got {:?}",
        unit.referenced_names
    );
}

#[test]
fn function_value_reference_is_recorded() {
    // `let f = other_fn` pools a `GenericFunction` wrapper over the callee's
    // global slot; the call itself is indirect (no name baked). The edge
    // comes from the wrapper's resolution.
    let unit = unit_for(
        &[
            ("def.baml", "function other_fn() -> int {\n  7\n}\n"),
            (
                "holder.baml",
                "function use_val() -> int {\n  let f = other_fn\n  f()\n}\n",
            ),
        ],
        "holder.baml",
    );
    assert!(
        has(&unit, "other_fn"),
        "a function referenced as a value must record its target; got {:?}",
        unit.referenced_names
    );
}

#[test]
fn lambda_interior_references_are_recorded() {
    let unit = unit_for(
        &[
            ("def.baml", "function helper(x: int) -> int {\n  x + 1\n}\n"),
            (
                "user.baml",
                "function run() -> int {\n  let f = (x: int) -> int { helper(x) }\n  f(1)\n}\n",
            ),
        ],
        "user.baml",
    );
    assert!(
        has(&unit, "helper"),
        "a reference made inside a lambda body belongs to the defining file's \
         record; got {:?}",
        unit.referenced_names
    );
}

#[test]
fn layout_flag_tracks_layout_baking_bytecode() {
    let files = [
        (
            "defs.baml",
            "class Point {\n  x int\n  y int\n}\n\
             function mk() -> Point {\n  Point { x: 1, y: 2 }\n}\n",
        ),
        ("reader.baml", "function read_x() -> int {\n  mk().x\n}\n"),
        ("plain.baml", "function forty_two() -> int {\n  42\n}\n"),
    ];
    let db = build_db(ROOT, &files);
    let units = emit_units(&db, package(&db), OptLevel::Two).expect("emit_units succeeds");
    let unit = |file: &str| {
        units
            .iter()
            .find(|u| u.source_file == file)
            .unwrap_or_else(|| panic!("no unit for `{file}`"))
    };
    // `mk().x` bakes Point's field offset through `LoadField` — an operand
    // with no recoverable type identity, so only the flag can track it.
    assert!(
        unit("reader.baml").bakes_type_layout,
        "an inferred-receiver field read bakes layout"
    );
    assert!(
        !unit("plain.baml").bakes_type_layout,
        "pure arithmetic bakes no layout"
    );
}

#[test]
fn recorded_names_are_sorted_and_deduplicated() {
    // The record is wire data compared byte-for-byte by the determinism and
    // cache-verify oracles: a set, sorted, no duplicates.
    let unit = unit_for(
        &[
            ("def.baml", "function twice(x: int) -> int {\n  x * 2\n}\n"),
            (
                "user.baml",
                "function run() -> int {\n  twice(twice(twice(1)))\n}\n",
            ),
        ],
        "user.baml",
    );
    let mut sorted = unit.referenced_names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        unit.referenced_names, sorted,
        "referenced_names must be sorted and deduplicated"
    );
}
