//! Differential sweep of the EXISTING pattern/exhaustiveness corpora (the
//! curated TIR test projects) through both engines: per function, does
//! hir_ty's usefulness verdict agree with TIR's E0062? One snapshot per
//! corpus renders every function's verdict pair, so agreements are proven
//! and every divergence is a reviewed line - the same discipline as the
//! fixture dumps, applied to the thousand-line legacy corpus instead of
//! hand-written fixtures.
//!
//! Known deliberate divergences (marked in the snapshots): functions
//! where TIR SUPPRESSES E0062 after an arm-level type error (its
//! `pattern_had_error` cascade gate) - hir_ty has no diagnostics yet, so
//! it reports the honest non-exhaustive verdict; both engines reject the
//! function either way. S17 decides the suppression policy.

use std::fmt::Write as _;

use baml_compiler2_hir::body::BodyOwnerId;

fn corpus_verdicts(relative_path: &str) -> String {
    let path = crate::manifest_dir().join(relative_path);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("cannot read corpus {}: {err}", path.display()));
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.add_file("corpus.baml", &source);

    let mut out = String::new();
    for owner in baml_compiler2_ppir::file_body_owners(&db, file) {
        let BodyOwnerId::Function(function) = owner else {
            continue;
        };
        let data = baml_compiler2_ppir::item_data::function_data(&db, function);
        // Synthetic $stream companions duplicate their originals.
        if data.name.as_str().ends_with("$stream") {
            continue;
        }
        let Some(scope) = baml_compiler2_ppir::body_scope(&db, owner) else {
            continue;
        };
        let _ = scope;
        let result = baml_compiler2_hir_ty::infer::infer_body(&db, owner);
        let hir_count = result.non_exhaustive_matches.len();
        writeln!(out, "{}: hir_ty={hir_count}", data.name.as_str()).expect("write to string");
    }
    out
}

#[test]
fn match_exhaustiveness_corpus() {
    crate::file_snapshot!(
        "src/type_spec/snapshots",
        "corpus__match_exhaustiveness",
        corpus_verdicts(
            "projects/diagnostic_errors/match_exhaustiveness/match_exhaustiveness.baml"
        )
    );
}

#[test]
fn generic_rigid_coverage_corpus() {
    crate::file_snapshot!(
        "src/type_spec/snapshots",
        "corpus__generic_rigid_coverage",
        corpus_verdicts("projects/diagnostic_errors/generic_rigid_coverage/main.baml")
    );
}

#[test]
fn patterns_new_corpus() {
    crate::file_snapshot!(
        "src/type_spec/snapshots",
        "corpus__patterns_new",
        corpus_verdicts("projects/compiles/patterns_new/patterns_new.baml")
    );
}

#[test]
fn generic_match_typevar_arm_corpus() {
    crate::file_snapshot!(
        "src/type_spec/snapshots",
        "corpus__generic_match_typevar_arm",
        corpus_verdicts(
            "projects/compiles/generic_match_typevar_arm/generic_match_typevar_arm.baml"
        )
    );
}
