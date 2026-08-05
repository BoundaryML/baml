// LLM alignment: the second pass over what deterministic name matching leaves
// behind.
//
// The structure-only pass matches `to_upper_case` ↔ `toUpperCase` by
// normalizing case and underscores. What it cannot see is that BAML's
// `String.substring` and TypeScript's `String.slice` are the same operation
// under different names, or that `Instant.now` answers `Date.now`. That
// residue — every BAML-only and TS-only member — is what goes to the model.
//
// The shape here keeps the model at arm's length: `align_candidates` decides
// what to ask and `apply_verdicts` decides what to believe, both pure
// functions over the report, both unit-tested without a key. Only
// `align_container` needs credentials. A wrong or unavailable model degrades
// the matrix to the structure-only pass rather than corrupting it.

// No `temperature`: claude-sonnet-5 rejects it ("`temperature` is deprecated
// for this model"). Run-to-run stability therefore comes from caching verdicts
// rather than from pinning sampling.
client<llm> Aligner {
    provider: anthropic,
    options: {
        model: "claude-sonnet-5",
        api_key: env.ANTHROPIC_API_KEY,
    },
}

/// One container pair's residue, rendered for the model.
class AlignmentTask {
    container_key: string,
    baml_members: string[],
    ts_members: string[],
}

/// A claimed equivalence between one BAML member and one TS member. The
/// member strings echo back the lines the model was given, so `apply_verdicts`
/// can reject anything it did not actually offer.
class AlignmentPair {
    baml_member: string @description("the BAML member's display name, exactly as given"),
    ts_member: string @description("the TypeScript member's display name, exactly as given"),
    confidence: "high" | "medium" | "low",
    reason: string @description("under 15 words"),
}

class AlignmentVerdict {
    container_key: string,
    pairs: AlignmentPair[],
}

/// Renders a pooled member for the prompt: display name plus signature, which
/// is what makes `slice` recognizable as `substring`.
function member_line(member: PooledMember) -> string {
    let signature = member.signature ?? "";
    if (signature.length() == 0) {
        `- ${member.display}`
    } else {
        `- ${member.display} — ${signature}`
    }
}

/// The containers worth asking about, and what to ask. A container with
/// residue on only one side has nothing to align: an unmatched member needs a
/// candidate on the other side to be matched *to*.
function align_candidates(report: MatrixReport) -> AlignmentTask[] {
    let tasks: AlignmentTask[] = [];
    for (let container in report.containers) {
        let baml_members: string[] = [];
        let ts_members: string[] = [];
        for (let row in container.rows) {
            if (row.status == "baml_only") {
                if let member: PooledMember = row.baml {
                    baml_members.push(member_line(member));
                }
            } else if (row.status == "ts_only") {
                if let member: PooledMember = row.ts {
                    ts_members.push(member_line(member));
                }
            }
        }
        if (baml_members.length() > 0 && ts_members.length() > 0) {
            tasks.push(AlignmentTask {
                container_key: container.key,
                baml_members: baml_members,
                ts_members: ts_members,
            });
        }
    }
    tasks
}

function align_container(task: AlignmentTask) -> AlignmentPair[] {
    client: Aligner
    prompt: `
You are comparing two standard libraries: BAML and TypeScript. A deterministic
name matcher has already paired members whose names agree after case and
underscore normalization. What remains below is the residue on each side of
the "${task.container_key}" container.

Identify pairs that are the SAME OPERATION under different names.

Match on behavior, not on name similarity. "substring" and "slice" are the
same operation; "push" and "pop" are not, despite both being array mutations.
Use each member in at most one pair. Leave a member unpaired when nothing on
the other side does its job — an unmatched member is a real finding about the
two libraries, not a failure. Return an empty list if nothing aligns.

Copy member names back exactly as given.

BAML members:
${task.baml_members.join("\n")}

TypeScript members:
${task.ts_members.join("\n")}

${ctx.output_format}
`
}

/// Smoke entry: align a single container from a generated report, so the LLM
/// seam can be exercised without running the whole matrix.
function align_one(
    container: string = "string",
    json_file: string = "tools/stdlib-matrix/report/matrix.json",
) -> AlignmentPair[] {
    let report = load_report(json_file);
    let matching = align_candidates(report)
        .filter(
            (task) -> {
                task.container_key == container
            },
        );
    if let task: AlignmentTask = matching.at(0) {
        align_container(task)
    } else {
        throw baml.errors.InvalidArgument {
            message: `no alignable container ${container}; it needs residue on both sides`,
        };
    }
}

/// Merges model verdicts into a report, returning a new report.
///
/// Every claim is checked against the rows actually sent: a pair naming a
/// member that was not offered, or naming one already consumed by an earlier
/// pair, is dropped. The model is a proposer, not an authority — it can only
/// join two members this run already classified as unmatched, never invent a
/// member or overwrite a deterministic match.
///
/// Dropped claims are counted rather than silently ignored, so a prompt that
/// starts hallucinating shows up as a number instead of as quietly worse data.
class MergeOutcome {
    report: MatrixReport,
    applied: int,
    rejected: string[],
}

function apply_verdicts(report: MatrixReport, verdicts: AlignmentVerdict[]) -> MergeOutcome {
    let by_container: map<string, AlignmentVerdict> = {};
    for (let verdict in verdicts) {
        let _ = by_container.set(verdict.container_key, verdict);
    }

    let rejected: string[] = [];
    let applied = 0;
    let containers: ContainerReport[] = [];
    for (let container in report.containers) {
        let verdict = by_container.get(container.key);
        if let found: AlignmentVerdict = verdict {
            containers.push(
                merge_container(container, found, rejected),
            );
        } else {
            containers.push(container);
        }
    }
    for (let container in containers) {
        applied += container.aligned;
    }

    let matched = 0;
    let aligned = 0;
    let baml_only = 0;
    let ts_only = 0;
    let synthetic_hidden = 0;
    for (let container in containers) {
        matched += container.matched;
        aligned += container.aligned;
        baml_only += container.baml_only;
        ts_only += container.ts_only;
        synthetic_hidden += container.synthetic_hidden;
    }

    MergeOutcome {
        report: MatrixReport {
            format_version: report.format_version,
            provenance: report.provenance,
            uncompared_baml: report.uncompared_baml,
            uncompared_ts: report.uncompared_ts,
            containers: containers,
            totals: MatrixTotals {
                containers: containers.length(),
                matched: matched,
                aligned: aligned,
                baml_only: baml_only,
                ts_only: ts_only,
                synthetic_hidden: synthetic_hidden,
            },
        },
        applied: applied,
        rejected: rejected,
    }
}

/// Folds one container's accepted pairs into merged rows. Unmatched rows keep
/// their identity; a joined pair becomes a single "aligned" row carrying both
/// members and the model's reasoning.
function merge_container(
    container: ContainerReport,
    verdict: AlignmentVerdict,
    rejected: string[],
) -> ContainerReport {
    let baml_rows: map<string, MemberRow> = {};
    let ts_rows: map<string, MemberRow> = {};
    for (let row in container.rows) {
        if (row.status == "baml_only") {
            if let member: PooledMember = row.baml {
                let _ = baml_rows.set(member.display, row);
            }
        } else if (row.status == "ts_only") {
            if let member: PooledMember = row.ts {
                let _ = ts_rows.set(member.display, row);
            }
        }
    }

    let consumed: map<string, bool> = {};
    let merged: MemberRow[] = [];
    for (let pair in verdict.pairs) {
        let baml_row = baml_rows.get(pair.baml_member);
        let ts_row = ts_rows.get(pair.ts_member);
        if let b: MemberRow = baml_row {
            if let t: MemberRow = ts_row {
                if (consumed.has(pair.baml_member) || consumed.has(pair.ts_member)) {
                    rejected.push(
                        `${container.key}: ${pair.baml_member} ↔ ${pair.ts_member} (member already paired)`,
                    );
                } else {
                    let _ = consumed.set(pair.baml_member, true);
                    let _ = consumed.set(pair.ts_member, true);
                    merged.push(MemberRow {
                        key: b.key,
                        status: "aligned",
                        baml: b.baml,
                        ts: t.ts,
                        baml_alternates: b.baml_alternates,
                        ts_alternates: t.ts_alternates,
                        alignment: AlignmentInfo {
                            confidence: pair.confidence,
                            reason: pair.reason,
                        },
                    });
                }
            } else {
                rejected.push(
                    `${container.key}: ${pair.ts_member} is not an unmatched TypeScript member`,
                );
            }
        } else {
            rejected.push(
                `${container.key}: ${pair.baml_member} is not an unmatched BAML member`,
            );
        }
    }

    let rows: MemberRow[] = [];
    for (let row in container.rows) {
        let display = "";
        if (row.status == "baml_only") {
            display = row.baml?.display ?? "";
        } else if (row.status == "ts_only") {
            display = row.ts?.display ?? "";
        }
        if (!consumed.has(display) || display.length() == 0) {
            rows.push(row);
        }
    }
    for (let row in merged) {
        rows.push(row);
    }
    rows.sort_by_key(
        (r) -> {
            `${status_rank(r.status)}:${r.key}`
        },
    );

    let matched = 0;
    let aligned = 0;
    let baml_only = 0;
    let ts_only = 0;
    for (let row in rows) {
        match (row.status) {
            "matched" => { matched += 1; },
            "aligned" => { aligned += 1; },
            "baml_only" => { baml_only += 1; },
            _ => { ts_only += 1; },
        }
    }

    ContainerReport {
        key: container.key,
        note: container.note,
        baml_origins: container.baml_origins,
        ts_origins: container.ts_origins,
        matched: matched,
        aligned: aligned,
        baml_only: baml_only,
        ts_only: ts_only,
        synthetic_hidden: container.synthetic_hidden,
        rows: rows,
    }
}

/// One container's alignment, as a verdict. A failed call yields an empty
/// verdict rather than aborting the run: alignment is an enrichment pass, and
/// one unlucky container should not cost the whole matrix.
function align_task(task: AlignmentTask) -> AlignmentVerdict {
    //# ask the model which leftover members are the same operation
    let empty: AlignmentPair[] = [];
    let pairs = align_container(task) catch (_) {
        _ => empty,
    };
    AlignmentVerdict { container_key: task.container_key, pairs: pairs }
}

/// Runs every alignable container concurrently and merges the results.
function align_report(report: MatrixReport) -> MergeOutcome {
    //# one green thread per container pair; ~20 calls in the time of the slowest
    let verdicts = await baml.future.all(
        align_candidates(report)
            .map(
                (task) -> {
                    spawn {
                        align_task(task)
                    }
                },
            ),
    );
    apply_verdicts(report, verdicts)
}
