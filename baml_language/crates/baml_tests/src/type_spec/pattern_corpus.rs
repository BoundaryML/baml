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
use std::path::Path;

use baml_compiler2_hir::body::BodyOwnerId;
use baml_compiler2_tir::infer_context::{DiagnosticSeverity, TirTypeError};
use baml_compiler2_tir::inference::render_scope_diagnostics;

fn corpus_verdicts(relative_path: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("cannot read corpus {}: {err}", path.display()));
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.add_file("corpus.baml", &source);

    // TIR verdicts: E0062 spans, mapped to functions by range containment.
    let index = baml_compiler2_ppir::file_semantic_index(&db, file);
    let mut tir_non_exhaustive: Vec<text_size::TextRange> = Vec::new();
    for scope_id in &index.scope_ids {
        for diag in render_scope_diagnostics(&db, *scope_id) {
            if diag.severity == DiagnosticSeverity::Error
                && matches!(diag.error, TirTypeError::NonExhaustiveMatch { .. })
            {
                tir_non_exhaustive.push(diag.range);
            }
        }
    }

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
        let range = index.scopes[scope.file_scope_id(&db).index() as usize].range;
        let result = baml_compiler2_hir_ty::infer::infer_body(&db, owner);
        let hir_count = result.non_exhaustive_matches.len();
        let tir_count = tir_non_exhaustive
            .iter()
            .filter(|diag| range.contains_range(**diag))
            .count();
        let marker = if hir_count == tir_count {
            ""
        } else {
            "   <-- DIVERGES"
        };
        writeln!(
            out,
            "{}: hir_ty={hir_count} tir={tir_count}{marker}",
            data.name.as_str()
        )
        .expect("write to string");
    }
    out
}

#[test]
fn match_exhaustiveness_corpus() {
    insta::assert_snapshot!(
        "corpus__match_exhaustiveness",
        corpus_verdicts(
            "projects/diagnostic_errors/match_exhaustiveness/match_exhaustiveness.baml"
        )
    );
}

#[test]
fn generic_rigid_coverage_corpus() {
    insta::assert_snapshot!(
        "corpus__generic_rigid_coverage",
        corpus_verdicts("projects/diagnostic_errors/generic_rigid_coverage/main.baml")
    );
}

#[test]
fn patterns_new_corpus() {
    insta::assert_snapshot!(
        "corpus__patterns_new",
        corpus_verdicts("projects/compiles/patterns_new/patterns_new.baml")
    );
}

#[test]
fn generic_match_typevar_arm_corpus() {
    insta::assert_snapshot!(
        "corpus__generic_match_typevar_arm",
        corpus_verdicts(
            "projects/compiles/generic_match_typevar_arm/generic_match_typevar_arm.baml"
        )
    );
}
