// Ratchet: compare a fresh run against a previously generated report.
//
// Both sides are the canonical JSON, decoded into SymbolMatrix, so this
// compares structures rather than rendered text — nothing here parses markdown.
//
// The baseline is whatever report the caller supplies: reports are artifacts,
// not checked-in files, so CI passes a previous run's and a human can diff any
// two.
//
// The gate is the report's *inputs* (its provenance). Comparing output would be
// the wrong key: it stops being deterministic once model judgments land, and it
// would fire for tool edits that reword a note without changing any fact about
// either stdlib. The inputs decide whether an older report still describes
// reality.
//
// Differences under identical inputs are still reported, as an advisory: they
// mean the tool itself changed between the two reports.

function short_sha(sha: string) -> string {
    if (sha.length() <= 12) {
        sha
    } else {
        `${sha.substring(0, 12)}…`
    }
}

/// Human-readable lines for each input that moved. Empty means the baseline was
/// built from exactly these inputs.
function provenance_changes(before: MatrixProvenance, now: MatrixProvenance) -> string[] {
    let changes: string[] = [];
    if (before.baml_surface_sha256 != now.baml_surface_sha256) {
        changes.push(
            `the BAML stdlib surface changed (${short_sha(before.baml_surface_sha256)} → ${short_sha(now.baml_surface_sha256)})`,
        );
    }
    if (before.export_format_version != now.export_format_version) {
        changes.push(
            `the export format changed (v${before.export_format_version} → v${now.export_format_version})`,
        );
    }
    if (before.typescript_version != now.typescript_version) {
        changes.push(`TypeScript ${before.typescript_version} → ${now.typescript_version}`);
    }
    let before_node = before.types_node_version ?? provenance_absent();
    let now_node = now.types_node_version ?? provenance_absent();
    if (before_node != now_node) {
        changes.push(`@types/node ${before_node} → ${now_node}`);
    }
    changes
}

/// How the compared surface moved. A shrinking BAML surface or lost matches are
/// regressions; more of either is an improvement. A moving TypeScript surface is
/// neither — it reflects their release, not our work. Symbols still awaiting a
/// judgement are reported on their own: an unjudged symbol is work outstanding,
/// not a finding either way.
function count_changes(before: MatrixCounts, now: MatrixCounts) -> BaselineComparison {
    let comparison = BaselineComparison {
        up_to_date: false,
        baseline_present: true,
        counts_differ: false,
        input_changes: [],
        regressions: [],
        improvements: [],
        neutral: [],
    };
    if (now.baml_symbols < before.baml_symbols) {
        comparison.regressions.push(
            `the BAML surface shrank, ${before.baml_symbols} → ${now.baml_symbols} symbols`,
        );
    } else if (now.baml_symbols > before.baml_symbols) {
        comparison.improvements.push(
            `the BAML surface grew, ${before.baml_symbols} → ${now.baml_symbols} symbols`,
        );
    }
    if (now.matched < before.matched) {
        comparison.regressions.push(`counterparts lost, ${before.matched} → ${now.matched}`);
    } else if (now.matched > before.matched) {
        comparison.improvements.push(`counterparts gained, ${before.matched} → ${now.matched}`);
    }
    if (now.unjudged != before.unjudged) {
        comparison.neutral.push(`symbols awaiting judgement ${before.unjudged} → ${now.unjudged}`);
    }
    if (now.ts_symbols != before.ts_symbols) {
        comparison.neutral.push(
            `TypeScript surface ${before.ts_symbols} → ${now.ts_symbols} symbols`,
        );
    }
    comparison
}

function compare_to_baseline(baseline: ReportHeader?, matrix: SymbolMatrix) -> BaselineComparison {
    if let before: ReportHeader = baseline {
        let comparison = count_changes(before.counts, matrix.counts);
        comparison.input_changes = provenance_changes(before.provenance, matrix.provenance);
        comparison.up_to_date = comparison.input_changes.length() == 0;
        comparison.counts_differ = before.counts != matrix.counts;
        comparison
    } else {
        BaselineComparison {
            up_to_date: false,
            baseline_present: false,
            counts_differ: false,
            input_changes: [],
            regressions: [],
            improvements: [],
            neutral: [],
        }
    }
}
