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
) -> MatrixResult {
    let export_doc = load_baml_surface(baml_surface_file);
    let ts_doc = load_ts_surface(ts_surface_file);
    let report = build_matrix_report(export_doc, ts_doc);
    MatrixResult {
        markdown: render_matrix_markdown(report),
        output_file: output_file,
        report: report,
    }
}

function main() -> MatrixResult {
    build_matrix()
}
