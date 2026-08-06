// The model pass: correspondences the name heuristic cannot reach.
//
// The deterministic prepass only pairs symbols whose *names* agree after
// normalization AND whose owners correspond. That is deliberately strict — it
// would rather miss a pair than invent one — so most of the surface arrives
// here unmatched: `baml.time.Instant.now` against `Date.now`, `int.parse`
// against `Number.parseInt`, `string.iter` against `[Symbol.iterator]`.
//
// The model is a proposer, never an authority. `align_batch` asks, and
// `apply_alignments` decides what to believe: a pair naming a symbol that was
// not offered is dropped and counted, so a prompt that starts hallucinating
// shows up as a number rather than as quietly worse data. A failed call costs
// that batch's enrichment, not the run.

// No `temperature`: claude-sonnet-5 rejects it ("`temperature` is deprecated
// for this model"). Reproducibility has to come from caching verdicts.
client<llm> Aligner {
    provider: anthropic,
    options: {
        model: "claude-sonnet-5",
        api_key: env.ANTHROPIC_API_KEY,
    },
}

/// One proposed equivalence. The displays echo back lines the model was given,
/// so `apply_alignments` can reject anything it did not actually offer.
class ProposedPair {
    baml_display: string @description("the BAML name from inside [ ], nothing else"),
    ts_display: string @description("the TypeScript name from inside [ ], nothing else"),
    confidence: "high" | "medium" | "low",
    reason: string @description("under 15 words"),
}

/// A batch's answer.
class BatchVerdict {
    pairs: ProposedPair[],
}

/// One group's unmatched BAML symbols, with the TypeScript menu to match against.
class AlignBatch {
    group: string,
    baml_lines: string[],
    ts_menu: string[],
}

/// Renders a BAML symbol for the prompt: what it is, its signature, and the
/// first line of its documentation — enough to judge behaviour, short enough
/// that a whole group fits in one call.
function baml_line(symbol: BamlSymbol) -> string {
    let signature = symbol.signature?.display ?? "";
    `- [${symbol.display}] ${signature}${first_line(symbol.doc)}`
}

function ts_line(symbol: TsSymbol) -> string {
    `- [${symbol.display}] ${symbol.signature.lines().at(0) ?? ""}${first_line(symbol.doc)}`
}

function first_line(doc: string?) -> string {
    let text = doc ?? "";
    if (text.length() == 0) {
        ""
    } else {
        `  // ${text.lines().at(0) ?? ""}`
    }
}

/// Which symbols still need an answer, batched by the group they live in.
///
/// Batching by group is what makes this affordable: one call per group rather
/// than one per symbol, and a group shares enough context that the model can
/// reason about the set. Already-matched symbols are left out — the prepass has
/// spoken for them.
function align_batches(matrix: SymbolMatrix) -> AlignBatch[] {
    let judged: map<string, bool> = {};
    let claimed: map<string, bool> = {};
    for (let judgement in matrix.judgements) {
        let _ = judged.set(judgement.baml, true);
        if (judgement_pairs(judgement)) {
            if let ts: string = judgement.ts {
                let _ = claimed.set(ts, true);
            }
        }
    }

    // The TypeScript menu is the whole unclaimed surface, listed once per call.
    // It is the thing being searched, so trimming it would hide the answer.
    let menu: string[] = [];
    for (let symbol in matrix.ts) {
        if (!claimed.has(symbol.id)) {
            menu.push(ts_line(symbol));
        }
    }

    let groups: map<string, string[]> = {};
    let order: string[] = [];
    for (let symbol in matrix.baml) {
        if (!judged.has(symbol.id)) {
            let group = baml_group_of(symbol);
            if (!groups.has(group)) {
                let empty: string[] = [];
                let _ = groups.set(group, empty);
                order.push(group);
            }
            if let bucket: string[] = groups.get(group) {
                bucket.push(baml_line(symbol));
            }
        }
    }

    let batches: AlignBatch[] = [];
    for (let group in order) {
        let lines: string[] = groups.get(group) ?? [];
        if (lines.length() > 0) {
            batches.push(AlignBatch { group: group, baml_lines: lines, ts_menu: menu });
        }
    }
    batches
}

function align_batch(batch: AlignBatch) -> BatchVerdict {
    client: Aligner
    prompt: `
You are comparing two standard libraries: BAML and TypeScript.

A deterministic pass has already paired everything whose names and owners
agree, so what follows is the residue. Your job is to find pairs that are the
SAME OPERATION under different names.

Match on behaviour, not on name similarity. "substring" and "slice" are the
same operation; "push" and "pop" are not, despite both being array mutations.
Expect structural differences: BAML puts static methods on primitives where
TypeScript uses free functions, and reaches operators through interface impls
where TypeScript uses built-in syntax. Those still count as the same operation.

Use each symbol in at most one pair. Leave a symbol unpaired when nothing on
the other side does its job — an unmatched symbol is a real finding about the
two libraries, not a failure. Return an empty list rather than reaching.

Every symbol below is written as [name] followed by its signature and, where
there is one, a summary. Report the name from inside the brackets — just the
name, without the brackets, the signature, or the summary.

BAML symbols in "${batch.group}" that are still unmatched:
${batch.baml_lines.join("\n")}

Unmatched TypeScript symbols to choose from:
${batch.ts_menu.join("\n")}

${ctx.output_format}
`
}

/// One batch, as a verdict. A failed call yields an empty verdict rather than
/// aborting: alignment is enrichment, and one unlucky batch should not cost the
/// whole run.
function align_one_batch(batch: AlignBatch) -> BatchVerdict {
    //# ask the model which leftover symbols are the same operation
    let empty = BatchVerdict { pairs: [] };
    align_batch(batch) catch (_) {
        _ => empty,
    }
}

class AlignOutcome {
    matrix: SymbolMatrix,
    applied: int,
    rejected: string[],
}

/// Folds verdicts into the matrix, keeping only claims that name symbols the
/// model was actually offered.
///
/// Claims arrive as displays, which collide — 15 BAML displays do — so a claim
/// is accepted only when it resolves to exactly one symbol that has not already
/// been judged. Everything else is rejected by name and counted, so a prompt
/// that starts hallucinating shows up as a number rather than as quietly worse
/// data.
function apply_alignments(matrix: SymbolMatrix, verdicts: BatchVerdict[]) -> AlignOutcome {
    let judged: map<string, bool> = {};
    for (let judgement in matrix.judgements) {
        let _ = judged.set(judgement.baml, true);
    }
    let baml_by_display: map<string, string[]> = {};
    for (let symbol in matrix.baml) {
        if (!judged.has(symbol.id)) {
            let bucket: string[] = baml_by_display.get(symbol.display) ?? [];
            bucket.push(symbol.id);
            let _ = baml_by_display.set(symbol.display, bucket);
        }
    }
    let ts_by_display: map<string, string[]> = {};
    for (let symbol in matrix.ts) {
        let bucket: string[] = ts_by_display.get(symbol.display) ?? [];
        bucket.push(symbol.id);
        let _ = ts_by_display.set(symbol.display, bucket);
    }

    let rejected: string[] = [];
    let applied = 0;
    // A claim may arrive as the whole prompt line rather than just the name.
    // Trimming at the first space recovers the display without loosening what
    // counts as a match: the result still has to name exactly one symbol.
    // A pre-typed empty, because `?? []` in a branch yields `string[] | _[]`
    // rather than unifying with the map's value type.
    let none: string[] = [];
    let resolve = (claimed: string, table: map<string, string[]>) -> {
        let hits: string[] = table.get(claimed) ?? none;
        if (hits.length() == 0) {
            if let cut: int = claimed.index_of(" ") {
                hits = table.get(claimed.substring(0, cut)) ?? none;
            }
        }
        hits
    };
    let consumed: map<string, bool> = {};
    for (let verdict in verdicts) {
        for (let pair in verdict.pairs) {
            let baml_hits = resolve(pair.baml_display, baml_by_display);
            let ts_hits = resolve(pair.ts_display, ts_by_display);
            if (baml_hits.length() != 1) {
                rejected.push(`${pair.baml_display} is not a single unjudged BAML symbol`);
            } else if (ts_hits.length() != 1) {
                rejected.push(`${pair.ts_display} is not a single TypeScript symbol`);
            } else if (consumed.has(pair.baml_display)) {
                rejected.push(`${pair.baml_display} was already paired`);
            } else {
                let _ = consumed.set(pair.baml_display, true);
                matrix
                    .judgements
                    .push(
                        Judgement {
                            baml: baml_hits.at(0) ?? "",
                            ts: ts_hits.at(0) ?? "",
                            verdict: "match",
                            basis: "model",
                            confidence: pair.confidence,
                            reason: pair.reason,
                            divergence: null,
                            rejected: none,
                            verified: false,
                        },
                    );
                applied += 1;
            }
        }
    }

    matrix.counts = recount(matrix);
    AlignOutcome { matrix: matrix, applied: applied, rejected: rejected }
}

/// Runs every batch concurrently and folds the results in.
function align_matrix(matrix: SymbolMatrix) -> AlignOutcome {
    //# one green thread per group; the run costs the slowest batch, not the sum
    let verdicts = await baml.future.all(
        align_batches(matrix)
            .map(
                (batch) -> {
                    spawn {
                        align_one_batch(batch)
                    }
                },
            ),
    );
    apply_alignments(matrix, verdicts)
}
