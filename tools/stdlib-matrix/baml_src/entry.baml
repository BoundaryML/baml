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

/// `build_matrix_aligned`'s result: the same artifacts, what each pass
/// contributed, and what was refused.
class AlignedMatrixResult {
    markdown: string,
    output_file: string,
    json_file: string,
    report: SymbolMatrix,
    carried: int,
    proposed: int,
    swept: int,
    upheld: int,
    refuted: int,
    rejected: string[],
    failures: int,
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

/// The full pass: name matching, then model alignment over what is left.
/// Reads ANTHROPIC_API_KEY from the environment.
function build_matrix_aligned(
    baml_surface_file: string = "tools/stdlib-matrix/data/baml_surface.json",
    ts_surface_file: string = "tools/stdlib-matrix/data/ts_surface.json",
    output_file: string = "tools/stdlib-matrix/report/matrix.md",
    json_file: string = "tools/stdlib-matrix/report/matrix.json",
    baml_surface_sha256: string = "",
    previous_file: string = "",
    review_all: bool = false,
    concurrency: int = 0,
) -> AlignedMatrixResult {
    // The passes see the previous revision too: a symbol that changed still
    // arrives with what was concluded about it last time, which is what makes
    // `bless` an answer the model can give. Read once, used twice.
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
    //# propose, sweep, then verify everything either produced
    let outcome = align_matrix(base.report, revision, concurrency);
    AlignedMatrixResult {
        markdown: render_matrix_markdown(outcome.matrix),
        output_file: output_file,
        json_file: json_file,
        report: outcome.matrix,
        carried: base.carried,
        proposed: outcome.proposed,
        swept: outcome.swept,
        upheld: outcome.upheld,
        refuted: outcome.refuted,
        rejected: outcome.rejected,
        failures: outcome.matrix.failures.length(),
    }
}

function main() -> MatrixResult {
    build_matrix()
}

/// What the passes would be asked, without asking. Inspection only.
class PoolReport {
    owners: int,
    total_candidates: int,
    dropped: int,
    empty_owners: int,
    pools: CandidatePool[],
}

function dump_pools(
    baml_surface_file: string = "tools/stdlib-matrix/data/baml_surface.json",
    ts_surface_file: string = "tools/stdlib-matrix/data/ts_surface.json",
) -> PoolReport {
    let matrix = build_symbol_matrix(load_baml_surface(baml_surface_file), load_ts_surface(ts_surface_file), "inspection");
    let pools = build_pools(matrix, pending_symbols(matrix));
    let total = 0;
    let dropped = 0;
    let empty = 0;
    for (let pool in pools) {
        total += pool.ts_ids.length();
        dropped += pool.dropped;
        if (pool.ts_ids.length() == 0) {
            empty += 1;
        }
    }
    PoolReport {
        owners: pools.length(),
        total_candidates: total,
        dropped: dropped,
        empty_owners: empty,
        pools: pools,
    }
}

/// What the passes would ask, rendered, without asking. Inspection only — this
/// is how a prompt change is reviewed before it costs anything.
class PlanReport {
    propose_calls: int,
    sweep_calls: int,
    no_candidate_symbols: int,
    sample_propose: string,
    sample_sweep: string,
}

function dump_plan(
    baml_surface_file: string = "tools/stdlib-matrix/data/baml_surface.json",
    ts_surface_file: string = "tools/stdlib-matrix/data/ts_surface.json",
    owner: string = "baml.time.Instant",
    container: string = "node:child_process",
) -> PlanReport {
    let matrix = build_symbol_matrix(load_baml_surface(baml_surface_file), load_ts_surface(ts_surface_file), "inspection");
    let pools = build_pools(matrix, pending_symbols(matrix));
    let proposals = propose_requests(matrix, pools, null);
    let empty = record_empty_pools(matrix, pools);
    let sweeps = sweep_requests(matrix);
    let sample_propose = "";
    for (let request in proposals) {
        if (request.owner == owner) {
            sample_propose = render_turns(propose_turns(request));
        }
    }
    let sample_sweep = "";
    for (let request in sweeps) {
        if (request.container == container) {
            sample_sweep = render_turns(sweep_turns(request));
        }
    }
    PlanReport {
        propose_calls: proposals.length(),
        sweep_calls: sweeps.length(),
        no_candidate_symbols: empty,
        sample_propose: sample_propose,
        sample_sweep: sample_sweep,
    }
}
