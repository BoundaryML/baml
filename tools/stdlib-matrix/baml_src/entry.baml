// Entry points. The `run` wrapper regenerates the two data files (they are
// gitignored intermediates), invokes one of these with absolute paths, and
// writes the canonical JSON plus its markdown view.
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

class MatrixResult {
    markdown: string,
    output_file: string,
    json_file: string,
    report: SymbolMatrix,
    /// Judgements taken from a previous revision rather than made again.
    carried: int,
    /// BAML symbols left for the passes to judge.
    queued: int,
}

class CheckResult {
    output_file: string,
    json_file: string,
    comparison: BaselineComparison,
    report: SymbolMatrix,
}

/// Builds the report.
///
/// `previous` is an earlier revision whose judgements are carried forward for
/// every symbol that has not changed — itself or anything it names.
/// `review_all` keeps the revision but carries nothing from it: every symbol is
/// put to the passes again, with its previous conclusion shown so a model can
/// bless it. That is the difference between trusting a judgement because
/// nothing moved and asking for it to be made again.
function build_matrix(
    baml_surface_file: string = "tools/stdlib-matrix/data/baml_surface.json",
    ts_surface_file: string = "tools/stdlib-matrix/data/ts_surface.json",
    output_file: string = "tools/stdlib-matrix/report/matrix.md",
    json_file: string = "tools/stdlib-matrix/report/matrix.json",
    baml_surface_sha256: string = "",
    previous_file: string = "",
    review_all: bool = false,
    previous: Revision? = null,
) -> MatrixResult {
    if (baml_surface_sha256.length() == 0) {
        throw baml.errors.InvalidArgument {
            message: "baml_surface_sha256 is required — it is the ratchet key; the run wrapper computes it",
        };
    }
    let matrix = build_symbol_matrix(load_baml_surface(baml_surface_file), load_ts_surface(ts_surface_file), baml_surface_sha256);
    let carried = 0;
    let queued = matrix.baml.length();
    // Loaded once when a caller already has it: the previous report is a
    // megabyte and a half, and the passes need the same one.
    let revision: Revision? = if (previous == null && previous_file.length() > 0) {
        load_revision(previous_file)
    } else {
        previous
    };
    if (!review_all) {
        if let before: Revision = revision {
            let outcome = apply_revision(matrix, before);
            matrix = outcome.matrix;
            carried = outcome.carried;
            queued = outcome.queued;
        }
    }
    MatrixResult {
        markdown: render_matrix_markdown(matrix),
        output_file: output_file,
        json_file: json_file,
        report: matrix,
        carried: carried,
        queued: queued,
    }
}

/// Ratchet mode: build the matrix and compare it against `baseline_file` — any
/// previously generated report — without writing anything. The wrapper turns
/// `comparison` into an exit code.
///
/// The baseline is supplied, never discovered: reports are artifacts rather
/// than checked-in files, so there is no well-known path to compare against.
function check_matrix(
    baml_surface_file: string = "tools/stdlib-matrix/data/baml_surface.json",
    ts_surface_file: string = "tools/stdlib-matrix/data/ts_surface.json",
    output_file: string = "tools/stdlib-matrix/report/matrix.md",
    json_file: string = "tools/stdlib-matrix/report/matrix.json",
    baml_surface_sha256: string = "",
    baseline_file: string = "",
    previous_file: string = "",
    review_all: bool = false,
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
        previous_file = previous_file,
        review_all = review_all,
    );
    let baseline: ReportHeader? = if (baml.fs.exists(baseline_file)) {
        load_report(baseline_file)
    } else {
        null
    };
    CheckResult {
        output_file: output_file,
        json_file: json_file,
        comparison: compare_to_baseline(baseline, result.report),
        report: result.report,
    }
}

function main() -> MatrixResult {
    build_matrix()
}

/// What the tools answer, without a model.
///
/// Inspection only, and the point of it: a tool that answers badly is a session
/// that wanders, and a session that wanders is paid for. This is how a lookup
/// is reviewed while it is still free.
class ToolReport {
    doc_sections: string[],
    skill_bytes: int,
    answers: string[],
}

function dump_tools(
    repo_root: string = ".",
    type_system_file: string = "baml_language/TYPE_SYSTEM.md",
    skill_cache: string = "tools/stdlib-matrix/data/baml_skill.md",
    baml_bin: string = "baml",
    tool: string = "",
    argument: string = "",
) -> ToolReport {
    let box = build_toolbox(repo_root, type_system_file, baml_bin);
    let answers: string[] = [];
    if (tool.length() > 0) {
        answers.push(run_tool(box, tool, argument));
    } else {
        // One of each, over questions the old pipeline got wrong: a name that
        // suggests nothing (`child_process.spawn` → `baml.sys.exec`), a type
        // whose members are the answer, and a concept with no symbol at all.
        answers.push(run_tool(box, "search", "run a subprocess"));
        answers.push(run_tool(box, "describe", "baml.sys.exec"));
        answers.push(run_tool(box, "describe", "baml.time"));
        answers.push(run_tool(box, "describe", "spaw"));
        answers.push(run_tool(box, "examples", "baml.sys.exec"));
        answers.push(run_tool(box, "docs", "Pattern Matching"));
        answers.push(run_tool(box, "invented", "anything"));
    }
    ToolReport {
        doc_sections: box.doc_headings,
        skill_bytes: load_skill(skill_cache).length(),
        answers: answers,
    }
}
