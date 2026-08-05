// Entry points. The `run` wrapper regenerates the two data files (they are
// gitignored intermediates), invokes build_matrix with absolute paths, and
// writes `markdown` to `output_file`.
//
// BUG(baml compiler): this file is named entry.baml instead of pipeline.baml
// because compilation is sensitive to file order: with identical content
// named pipeline.baml (sorting mid-package), a cold `baml check` livelocks
// (>4.5 min, killed) in a salsa fixpoint iteration between infer_scope_types
// and callable_throws (single busy thread under execute_maybe_iterate;
// throws-inference cycle that never converges). Named entry.baml (sorting
// first) or zz_pipeline.baml (sorting last), the same package checks in 2s.
// Deterministic 3/3 either way. Repro: rename this file to pipeline.baml,
// delete .baml/, run `baml check`. The throws-inference rework should retire
// this; until then, keep this file first in sort order.

function build_matrix(
    baml_surface_file: string = "tools/stdlib-matrix/data/baml_surface.json",
    ts_surface_file: string = "tools/stdlib-matrix/data/ts_surface.json",
    output_file: string = "tools/stdlib-matrix/report/matrix.md",
    json_file: string = "tools/stdlib-matrix/report/matrix.json",
    baml_surface_sha256: string = "",
) -> MatrixResult {
    if (baml_surface_sha256.length() == 0) {
        throw baml.errors.InvalidArgument {
            message: "baml_surface_sha256 is required — it is the ratchet key; the run wrapper computes it",
        };
    }
    let export_doc = load_baml_surface(baml_surface_file);
    let ts_doc = load_ts_surface(ts_surface_file);
    let report = build_matrix_report(export_doc, ts_doc, baml_surface_sha256);
    MatrixResult {
        markdown: render_matrix_markdown(report),
        output_file: output_file,
        json_file: json_file,
        report: report,
    }
}

/// Ratchet mode: build the matrix and compare it against `baseline_file` —
/// any previously generated canonical report — without writing anything. The
/// wrapper turns `comparison` into an exit code.
///
/// The baseline is supplied, never discovered: reports are CI artifacts rather
/// than checked-in files, so there is no well-known path to compare against.
/// CI passes the previous run's artifact; a human can pass any two reports.
function check_matrix(
    baml_surface_file: string = "tools/stdlib-matrix/data/baml_surface.json",
    ts_surface_file: string = "tools/stdlib-matrix/data/ts_surface.json",
    output_file: string = "tools/stdlib-matrix/report/matrix.md",
    json_file: string = "tools/stdlib-matrix/report/matrix.json",
    baml_surface_sha256: string = "",
    baseline_file: string = "",
) -> CheckResult {
    if (baseline_file.length() == 0) {
        throw baml.errors.InvalidArgument {
            message: "baseline_file is required — pass a previously generated report to compare against",
        };
    }
    let result = build_matrix(
        baml_surface_file = baml_surface_file,
        ts_surface_file = ts_surface_file,
        output_file = output_file,
        json_file = json_file,
        baml_surface_sha256 = baml_surface_sha256,
    );
    let baseline: MatrixReport? = if (baml.fs.exists(baseline_file)) {
        load_report(baseline_file)
    } else {
        null
    };
    CheckResult {
        markdown: result.markdown,
        output_file: output_file,
        json_file: json_file,
        comparison: compare_to_baseline(baseline, result.report),
        report: result.report,
    }
}

/// The full pass: structure-only matching, then LLM alignment over the
/// residue. Reads ANTHROPIC_API_KEY from the environment.
function build_matrix_aligned(
    baml_surface_file: string = "tools/stdlib-matrix/data/baml_surface.json",
    ts_surface_file: string = "tools/stdlib-matrix/data/ts_surface.json",
    output_file: string = "tools/stdlib-matrix/report/matrix.md",
    json_file: string = "tools/stdlib-matrix/report/matrix.json",
    baml_surface_sha256: string = "",
) -> AlignedMatrixResult {
    let base = build_matrix(
        baml_surface_file = baml_surface_file,
        ts_surface_file = ts_surface_file,
        output_file = output_file,
        json_file = json_file,
        baml_surface_sha256 = baml_surface_sha256,
    );
    //# enrich the structure-only report with model-judged equivalences
    let outcome = align_report(base.report);
    AlignedMatrixResult {
        markdown: render_matrix_markdown(outcome.report),
        output_file: output_file,
        json_file: json_file,
        report: outcome.report,
        applied: outcome.applied,
        rejected: outcome.rejected,
    }
}

function main() -> MatrixResult {
    build_matrix()
}
