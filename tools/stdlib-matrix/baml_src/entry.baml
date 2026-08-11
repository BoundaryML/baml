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

/// `build_matrix_aligned`'s result: the same artifacts, what the sessions
/// contributed, and what was refused.
class AlignedMatrixResult {
    markdown: string,
    output_file: string,
    json_file: string,
    report: SymbolMatrix,
    carried: int,
    sessions: int,
    judged: int,
    rejected: string[],
    failures: int,
}

/// The full pass: the surfaces, then a session per TypeScript container over
/// everything a previous revision did not already answer.
///
/// Reads OPENAI_API_KEY from the environment.
function build_matrix_aligned(
    baml_surface_file: string = "tools/stdlib-matrix/data/baml_surface.json",
    ts_surface_file: string = "tools/stdlib-matrix/data/ts_surface.json",
    output_file: string = "tools/stdlib-matrix/report/matrix.md",
    json_file: string = "tools/stdlib-matrix/report/matrix.json",
    baml_surface_sha256: string = "",
    previous_file: string = "",
    review_all: bool = false,
    concurrency: int = 0,
    repo_root: string = ".",
    type_system_file: string = "baml_language/TYPE_SYSTEM.md",
    skill_cache: string = "tools/stdlib-matrix/data/baml_skill.md",
    baml_bin: string = "baml",
    scratch_root: string = "tools/stdlib-matrix/data/scratch",
    only: string = "",
) -> AlignedMatrixResult {
    // The sessions see the previous revision too: a symbol that changed still
    // arrives with what was concluded about it last time, so the model can
    // repeat a conclusion rather than rediscover it. Read once, used twice.
    let revision: Revision? = if (previous_file.length() > 0) {
        load_revision(previous_file)
    } else {
        null
    };
    let base = build_matrix(
        baml_surface_file = baml_surface_file,
        ts_surface_file = ts_surface_file,
        output_file = output_file,
        json_file = json_file,
        baml_surface_sha256 = baml_surface_sha256,
        review_all = review_all,
        previous = revision,
    );
    // Fetched before anything is spent. A run that loses its documentation
    // produces worse judgements that look exactly like good ones, so this is
    // the right place to fail.
    let skill = load_skill(skill_cache);
    report_progress(`skill loaded, ${skill.length()} bytes`);
    let box = build_toolbox(repo_root, type_system_file, baml_bin, skill, scratch_root);
    let outcome = align_matrix(base.report, box, revision, concurrency, only);
    AlignedMatrixResult {
        markdown: render_matrix_markdown(outcome.matrix),
        output_file: output_file,
        json_file: json_file,
        report: outcome.matrix,
        carried: base.carried,
        sessions: outcome.sessions,
        judged: outcome.judged,
        rejected: outcome.rejected,
        failures: outcome.matrix.failures.length(),
    }
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
    scratch_root: string = "tools/stdlib-matrix/data/scratch",
    tool: string = "",
    argument: string = "",
) -> ToolReport {
    // Loaded once and given to the toolbox, exactly as a real run does. Not
    // tolerated when it fails: the skill is what a session is told BAML *is*,
    // and inspecting the tools against a toolbox that never got it would be
    // inspecting something other than what runs.
    let skill = load_skill(skill_cache);
    let box = build_toolbox(repo_root, type_system_file, baml_bin, skill, scratch_root);
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
    ToolReport { doc_sections: box.doc_headings, skill_bytes: box.skill.length(), answers: answers }
}

/// What the sessions would be asked, without asking. Inspection only — this is
/// how a prompt change is reviewed before it costs anything.
class PlanReport {
    sessions: int,
    symbols_queued: int,
    largest: string,
    /// The whole first turn of one session, rendered exactly as it will be sent.
    sample: string,
}

function dump_plan(
    baml_surface_file: string = "tools/stdlib-matrix/data/baml_surface.json",
    ts_surface_file: string = "tools/stdlib-matrix/data/ts_surface.json",
    subject: string = "(globals).Date",
) -> PlanReport {
    let matrix = build_symbol_matrix(load_baml_surface(baml_surface_file), load_ts_surface(ts_surface_file), "inspection");
    let requests = session_requests(matrix, null);
    let queued = 0;
    let largest = "";
    let most = 0;
    // The first matching session; a chunked container has several and they
    // differ only in which symbols they carry.
    let sample = "";
    for (let request in requests) {
        queued += request.ts_lines.length();
        if (request.ts_lines.length() > most) {
            most = request.ts_lines.length();
            largest = `${request.subject} (${request.ts_lines.length()} symbols)`;
        }
        // The same rule `--only` uses, so the two select sessions alike. Equality
        // was wrong for exactly the default: `(globals).Date` chunks into
        // `(globals).Date (part 1 of 2)` and matched nothing.
        if (request.subject.to_lower_case().includes(subject.trim().to_lower_case())) {
            // The skill is fetched at run time and is the same for every
            // session, so it is left out here: what changes between runs, and
            // what a review is actually looking at, is the rest.
            sample
                = render_turns(
                    [
                        Turn { role: "system", text: session_rules() },
                        Turn { role: "user", text: session_user(request) },
                    ],
                );
        }
    }
    PlanReport {
        sessions: requests.length(),
        symbols_queued: queued,
        largest: largest,
        sample: sample,
    }
}
