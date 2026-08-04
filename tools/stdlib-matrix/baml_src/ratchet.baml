// Ratchet: compare a fresh run against a previously generated report.
//
// Both sides are the canonical JSON report, decoded into MatrixReport, so this
// compares structures rather than rendered text — nothing here parses markdown.
//
// The baseline is whatever report the caller supplies: reports are CI
// artifacts, not checked-in files, so CI passes the previous run's artifact and
// a human can diff any two reports.
//
// The gate is the report's *inputs* (its provenance). Comparing rendered output
// would be the wrong key: it is about to stop being deterministic once LLM
// alignment lands, and it would fire for tool edits that reword a note without
// changing any fact about either stdlib. The inputs are exactly what decides
// whether an older report still describes reality.
//
// Structural differences under identical inputs are still reported, as an
// advisory: they mean the tool itself changed between the two reports.

function short_sha(sha: string) -> string {
    if (sha.length() <= 12) {
        sha
    } else {
        `${sha.substring(0, 12)}…`
    }
}

/// Human-readable lines for each input that moved. Empty means the baseline
/// report was built from exactly these inputs.
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

function counts_of(report: ContainerReport) -> ContainerCounts {
    ContainerCounts {
        key: report.key,
        matched: report.matched,
        baml_only: report.baml_only,
        ts_only: report.ts_only,
    }
}

/// Classifies one container's movement, appending human-readable lines to the
/// accumulating verdict. A shrinking BAML surface or a lost match is a
/// regression; more matches or a newly compared container is an improvement;
/// movement that only reflects TypeScript's side is neutral.
function classify_delta(
    key: string,
    baseline: ContainerCounts?,
    current: ContainerCounts?,
    comparison: BaselineComparison,
) -> void {
    if let now: ContainerCounts = current {
        if let before: ContainerCounts = baseline {
            let unchanged = before.matched == now.matched
                && before.baml_only == now.baml_only
                && before.ts_only == now.ts_only;
            if (!unchanged) {
                comparison.deltas.push(ContainerDelta {
                    key: key,
                    status: "changed",
                    baseline: before,
                    current: now,
                });
                let baml_before = before.matched + before.baml_only;
                let baml_now = now.matched + now.baml_only;
                if (baml_now < baml_before) {
                    comparison.regressions.push(
                        `${key}: BAML surface shrank, ${baml_before} → ${baml_now} members`,
                    );
                } else if (now.matched < before.matched) {
                    comparison.regressions.push(
                        `${key}: matches lost, ${before.matched} → ${now.matched}`,
                    );
                } else if (now.matched > before.matched) {
                    comparison.improvements.push(
                        `${key}: matches gained, ${before.matched} → ${now.matched}`,
                    );
                } else {
                    comparison.neutral.push(
                        `${key}: BAML ${baml_before} → ${baml_now}, TS-only ${before.ts_only} → ${now.ts_only}`,
                    );
                }
            }
        } else {
            comparison.deltas.push(ContainerDelta {
                key: key,
                status: "added",
                baseline: null,
                current: now,
            });
            comparison.improvements.push(`${key}: newly compared container`);
        }
    } else {
        comparison.deltas.push(ContainerDelta {
            key: key,
            status: "removed",
            baseline: baseline,
            current: null,
        });
        comparison.regressions.push(`${key}: container no longer compared`);
    }
}

/// True when two reports differ anywhere outside their provenance — down to a
/// reworded note or a single member row. Used only for the advisory: identical
/// inputs plus a differing body means the tool changed.
function bodies_differ(before: MatrixReport, now: MatrixReport) -> bool {
    !baml.deep_equals(before.containers, now.containers)
        || !baml.deep_equals(before.uncompared_baml, now.uncompared_baml)
        || !baml.deep_equals(before.uncompared_ts, now.uncompared_ts)
        || !baml.deep_equals(before.totals, now.totals)
}

function compare_to_baseline(baseline: MatrixReport?, report: MatrixReport) -> BaselineComparison {
    let comparison = BaselineComparison {
        up_to_date: false,
        baseline_present: false,
        body_differs: false,
        input_changes: [],
        deltas: [],
        regressions: [],
        improvements: [],
        neutral: [],
    };

    if let before: MatrixReport = baseline {
        comparison.baseline_present = true;
        comparison.input_changes = provenance_changes(before.provenance, report.provenance);
        comparison.up_to_date = comparison.input_changes.length() == 0;
        comparison.body_differs = bodies_differ(before, report);

        // Narrate which containers moved only when the report is actually
        // stale; a missing baseline has nothing to compare against.
        if (!comparison.up_to_date) {
            let by_key: map<string, ContainerReport> = {};
            for (let container in before.containers) {
                let _ = by_key.set(container.key, container);
            }
            let seen: map<string, bool> = {};
            for (let container in report.containers) {
                let _ = seen.set(container.key, true);
                let baseline_counts: ContainerCounts? = null;
                if let previous: ContainerReport = by_key.get(container.key) {
                    baseline_counts = counts_of(previous);
                }
                classify_delta(
                    container.key,
                    baseline_counts,
                    counts_of(container),
                    comparison,
                );
            }
            for (let container in before.containers) {
                if (!seen.has(container.key)) {
                    classify_delta(container.key, counts_of(container), null, comparison);
                }
            }
        }
    }
    comparison
}
