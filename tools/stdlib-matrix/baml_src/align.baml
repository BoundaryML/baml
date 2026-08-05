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
    let matched: map<string, bool> = {};
    for (let link in matrix.correspondences) {
        let _ = matched.set(link.baml.to_string(), true);
    }

    // The TypeScript menu is the whole unmatched surface, listed once per call.
    // It is the thing being searched, so trimming it would hide the answer.
    let menu: string[] = [];
    let claimed: map<string, bool> = {};
    for (let link in matrix.correspondences) {
        let _ = claimed.set(link.ts.to_string(), true);
    }
    let ts_index = 0;
    for (let symbol in matrix.ts) {
        if (!claimed.has(ts_index.to_string())) {
            menu.push(ts_line(symbol));
        }
        ts_index += 1;
    }

    let groups: map<string, string[]> = {};
    let order: string[] = [];
    let index = 0;
    for (let symbol in matrix.baml) {
        if (!matched.has(index.to_string())) {
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
        index += 1;
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
function apply_alignments(matrix: SymbolMatrix, verdicts: BatchVerdict[]) -> AlignOutcome {
    // Displays collide, so a display maps to every index that bears it; a claim
    // is only accepted when it resolves to exactly one unmatched symbol.
    let baml_by_display: map<string, int[]> = {};
    let matched: map<string, bool> = {};
    for (let link in matrix.correspondences) {
        let _ = matched.set(link.baml.to_string(), true);
    }
    let index = 0;
    for (let symbol in matrix.baml) {
        if (!matched.has(index.to_string())) {
            let bucket: int[] = baml_by_display.get(symbol.display) ?? [];
            bucket.push(index);
            let _ = baml_by_display.set(symbol.display, bucket);
        }
        index += 1;
    }
    let ts_by_display: map<string, int[]> = {};
    let ts_index = 0;
    for (let symbol in matrix.ts) {
        let bucket: int[] = ts_by_display.get(symbol.display) ?? [];
        bucket.push(ts_index);
        let _ = ts_by_display.set(symbol.display, bucket);
        ts_index += 1;
    }

    let rejected: string[] = [];
    let applied = 0;
    // A claim may arrive as the whole prompt line rather than just the name.
    // Trimming at the first space recovers the display without loosening what
    // counts as a match: the result still has to name exactly one symbol.
    // A pre-typed empty, because `?? []` in a branch yields `int[] | _[]`
    // rather than unifying with the map's value type.
    let none: int[] = [];
    let resolve = (claimed: string, table: map<string, int[]>) -> {
        let hits: int[] = table.get(claimed) ?? none;
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
                rejected.push(`${pair.baml_display} is not a single unmatched BAML symbol`);
            } else if (ts_hits.length() != 1) {
                rejected.push(`${pair.ts_display} is not a single TypeScript symbol`);
            } else if (consumed.has(pair.baml_display)) {
                rejected.push(`${pair.baml_display} was already paired`);
            } else {
                let _ = consumed.set(pair.baml_display, true);
                matrix
                    .correspondences
                    .push(
                        Correspondence {
                            baml: baml_hits.at(0) ?? 0,
                            ts: ts_hits.at(0) ?? 0,
                            basis: "model",
                            confidence: pair.confidence,
                            reason: pair.reason,
                        },
                    );
                applied += 1;
            }
        }
    }

    matrix.counts = recount(matrix);
    AlignOutcome { matrix: matrix, applied: applied, rejected: rejected }
}

/// Recomputes the tallies after enrichment, from the relation rather than by
/// adjusting the old numbers.
function recount(matrix: SymbolMatrix) -> MatrixCounts {
    let baml_linked: map<string, bool> = {};
    let ts_linked: map<string, bool> = {};
    for (let link in matrix.correspondences) {
        let _ = baml_linked.set(link.baml.to_string(), true);
        let _ = ts_linked.set(link.ts.to_string(), true);
    }
    let matched = 0;
    let unmatched = 0;
    let index = 0;
    for (let symbol in matrix.baml) {
        if (baml_linked.has(index.to_string())) {
            matched += 1;
        } else {
            unmatched += 1;
        }
        index += 1;
    }
    let ts_unmatched = 0;
    let scan = 0;
    while (scan < matrix.ts.length()) {
        if (!ts_linked.has(scan.to_string())) {
            ts_unmatched += 1;
        }
        scan += 1;
    }
    MatrixCounts {
        baml_symbols: matrix.baml.length(),
        ts_symbols: matrix.ts.length(),
        correspondences: matrix.correspondences.length(),
        baml_matched: matched,
        baml_unmatched: unmatched,
        ts_unmatched: ts_unmatched,
    }
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
