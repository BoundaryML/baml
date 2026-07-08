// Stage (c) — intention inference on every block and interaction, all on the
// fast tier (GraderFast/Haiku), scheduled bottom-up so each parent's call is
// grounded in its children's already-inferred intentions:
//
//   InferLeafIntents        ONE call per parent with leaf children (fields,
//                           variants, params, vars batched in one prompt)
//   InferBlockIntent        one call per non-leaf block (method/decl/file/
//                           program), given its children's intentions
//   InferInteractionIntents ONE call per parent group of interactions
//                           (sub-batched), given both endpoints' heads
//
// Every call has a cached_* wrapper: content-addressed hit -> decode, miss ->
// call the model; either way the output is deterministically reconciled
// against the request (unknown ids dropped, missing ids filled neutral,
// confidence clamped). Bump prompt_version() in model.baml on prompt edits.

// A block is leaf-treated when it is a member-level node with no children —
// it rides its parent's batched call instead of getting its own.
function is_leaf_block(b: CodeBlock) -> bool {
    (b.depth >= 3) && (b.child_ids.length() == 0)
}

function store_json(cache_dir: string, key: string, value: json) -> null {
    let text = baml.json.to_string(value) catch (_) {
        _ => ""
    };
    if (text != "") {
        cache_put(cache_dir, key, text);
    }
    null
}

// ---- result carriers -----------------------------------------------------------

class IntentPair {
    block_id: string,
    intention: Intention,
}

class InterPair {
    interaction_id: string,
    intention: InteractionIntention,
}

class IntentResult {
    blocks: IntentPair[], // one per CodeBlock, sorted by block_id
    interactions: InterPair[], // one per InteractionBlock, sorted by interaction_id
}

// ---- prompt tables ----------------------------------------------------------------

function leaf_table(leaves: CodeBlock[]) -> string {
    let out = "";
    for (let l in leaves) {
        out
            = out
                + l.id
                + " | "
                + l.kind
                + " | "
                + l.name
                + " | "
                + squash_ws(truncate(l.source, 300))
                + "\n";
    }
    out
}

function child_intent_table(parent: CodeBlock, intents: map<string, Intention>) -> string {
    let out = "";
    for (let cid in parent.child_ids) {
        match (intents.get(cid)) {
            let it: Intention => {
                out
                    = out
                        + cid
                        + " | "
                        + squash_ws(it.goal)
                        + " | "
                        + squash_ws(it.mechanism)
                        + "\n";
            },
            null => null,
        }
    }
    out
}

function pair_table(pairs: InteractionBlock[], by_id: map<string, CodeBlock>) -> string {
    let out = "";
    for (let p in pairs) {
        let a_desc = match (by_id.get(p.a_id)) {
            let b: CodeBlock => b.kind + " " + squash_ws(truncate(block_head(b), 160)),
            null => "",
        };
        let b_desc = match (by_id.get(p.b_id)) {
            let b: CodeBlock => b.kind + " " + squash_ws(truncate(block_head(b), 160)),
            null => "",
        };
        out
            = out
                + p.id
                + " | "
                + p.edge_kind
                + " | A: "
                + p.a_id
                + " ("
                + a_desc
                + ") | B: "
                + p.b_id
                + " ("
                + b_desc
                + ")\n";
    }
    out
}

// ---- LLM functions ------------------------------------------------------------------

function InferLeafIntents(
    parent_kind: string,
    parent_source: string,
    leaves: string,
) -> LeafBatchOut {
    client: GraderFast
    prompt: #"
        You are annotating the smallest blocks of a BAML codebase (BAML is a
        typed language for LLM functions). Below: a parent block, then a table
        of its leaf members (fields, enum variants, parameters, let-bound
        variables), one per line as `block_id | kind | name | source`.

        For EACH row, infer the author's intent: `goal` is what this leaf is
        for (one short sentence), `mechanism` is how it does it — the type,
        expression, or primitive actually used (one short sentence). The gap
        between a legitimate goal and a poor mechanism is what a later reviewer
        grades. `confidence` is 1-5. Copy `block_id` exactly from the table;
        return exactly one entry per row. Never invent ids.

        # parent block ({{ parent_kind }})
        {{ parent_source }}

        # leaf blocks
        {{ leaves }}

        {{ ctx.output_format }}
    "#
}

function InferBlockIntent(
    block_kind: string,
    block_source: string,
    child_intents: string,
    neighbors: string,
) -> Intention {
    client: GraderFast
    prompt: #"
        You are inferring the intention behind one block of a BAML codebase
        (BAML is a typed language for LLM functions). State what the author is
        trying to achieve (`goal`) and how the block actually does it
        (`mechanism` — the approach and primitives used). One sentence each,
        cleanly separated — the gap between a legitimate goal and a poor
        mechanism is what a later reviewer grades. `confidence` is 1-5.

        Ground the summary in the children's already-inferred intentions when
        present: the block's purpose is what its parts accomplish together,
        not a restatement of its syntax.

        # block kind
        {{ block_kind }}

        # block source
        {{ block_source }}

        # children (id | goal | mechanism)
        {{ child_intents }}

        # reference neighborhood (how this block is wired to the project)
        {{ neighbors }}

        {{ ctx.output_format }}
    "#
}

function InferInteractionIntents(pairs: string) -> InterBatchOut {
    client: GraderFast
    prompt: #"
        You are annotating RELATIONS between blocks of a BAML codebase. Below:
        a table of interactions, one per line as
        `interaction_id | edge_kind | A: <block> | B: <block>`.

        Edge kinds: `contains` (A is declared inside B), `sibling` (A and B sit
        side by side under the same parent), `calls` / `uses_type` /
        `exercises` (A statically references B).

        For EACH row: `relationship` is how A and B relate structurally (one
        short sentence), `purpose` is WHY the relation exists — what the pair
        achieves together, or what one provides the other (one short sentence).
        `confidence` is 1-5. Copy `interaction_id` exactly; one entry per row;
        never invent ids.

        # interactions
        {{ pairs }}

        {{ ctx.output_format }}
    "#
}

// ---- cached wrappers -----------------------------------------------------------------

// Batched leaf intents for one parent, reconciled: one Intention per requested
// leaf, unknown ids dropped, missing filled neutral.
function cached_leaf_intents(
    cache_dir: string,
    parent: CodeBlock,
    leaves: CodeBlock[],
) -> IntentPair[] {
    let table = leaf_table(leaves);
    let key = cache_key("intent-leaf@" + fast_model(), parent.id + "\n" + table);
    let raw = match (cache_get(cache_dir, key)) {
        let hit: string => {
            baml.json.from_string<LeafBatchOut>(hit) catch (_) {
                _ => fresh_leaf_intents(cache_dir, key, parent, table)
            }
        },
        null => fresh_leaf_intents(cache_dir, key, parent, table),
    };
    let by_id: map<string, LeafIntent> = {};
    for (let li in raw.intents) {
        if (!by_id.has(li.block_id)) {
            by_id.set(li.block_id, li);
        }
    }
    let out: IntentPair[] = [];
    for (let l in leaves) {
        let it = match (by_id.get(l.id)) {
            let li: LeafIntent => {
                Intention {
                    goal: li.goal,
                    mechanism: li.mechanism,
                    confidence: li.confidence.clamp(1, 5),
                }
            },
            null => Intention { goal: "", mechanism: "", confidence: 1 },
        };
        out.push(IntentPair { block_id: l.id, intention: it });
    }
    out
}

function fresh_leaf_intents(
    cache_dir: string,
    key: string,
    parent: CodeBlock,
    table: string,
) -> LeafBatchOut {
    let out = InferLeafIntents(parent.kind, truncate(parent.source, 4000), table);
    store_json(cache_dir, key, out.to_json());
    out
}

// One non-leaf block's intention. The child-intent table participates in the
// key, so keys are transitively content-addressed: editing a variable
// re-judges exactly its containment chain.
function cached_block_intent(
    cache_dir: string,
    block: CodeBlock,
    child_intents: string,
    neighbors: string,
) -> Intention {
    let key = cache_key(
        "intent@" + fast_model(),
        block.id + "\n" + block.source + "\n" + child_intents + "\n" + neighbors,
    );
    let raw = match (cache_get(cache_dir, key)) {
        let hit: string => {
            baml.json.from_string<Intention>(hit) catch (_) {
                _ => fresh_block_intent(cache_dir, key, block, child_intents, neighbors)
            }
        },
        null => fresh_block_intent(cache_dir, key, block, child_intents, neighbors),
    };
    return Intention { goal: raw.goal, mechanism: raw.mechanism, confidence: raw.confidence.clamp(1, 5) };
}

function fresh_block_intent(
    cache_dir: string,
    key: string,
    block: CodeBlock,
    child_intents: string,
    neighbors: string,
) -> Intention {
    let out = InferBlockIntent(block.kind, truncate(block.source, 6000), child_intents, neighbors);
    store_json(cache_dir, key, out.to_json());
    out
}

// Batched interaction intents for one (parent group, sub-batch), reconciled.
function cached_inter_intents(
    cache_dir: string,
    parent_id: string,
    batch_no: int,
    pairs: InteractionBlock[],
    by_id: map<string, CodeBlock>,
) -> InterPair[] {
    let table = pair_table(pairs, by_id);
    let key = cache_key("intent-inter@" + fast_model(), parent_id + "#" + i2s(batch_no) + "\n" + table);
    let raw = match (cache_get(cache_dir, key)) {
        let hit: string => {
            baml.json.from_string<InterBatchOut>(hit) catch (_) {
                _ => fresh_inter_intents(cache_dir, key, table)
            }
        },
        null => fresh_inter_intents(cache_dir, key, table),
    };
    let got: map<string, InterIntent> = {};
    for (let ii in raw.intents) {
        if (!got.has(ii.interaction_id)) {
            got.set(ii.interaction_id, ii);
        }
    }
    let out: InterPair[] = [];
    for (let p in pairs) {
        let it = match (got.get(p.id)) {
            let ii: InterIntent => {
                InteractionIntention {
                    relationship: ii.relationship,
                    purpose: ii.purpose,
                    confidence: ii.confidence.clamp(1, 5),
                }
            },
            null => InteractionIntention { relationship: "", purpose: "", confidence: 1 },
        };
        out.push(InterPair { interaction_id: p.id, intention: it });
    }
    out
}

function fresh_inter_intents(cache_dir: string, key: string, table: string) -> InterBatchOut {
    let out = InferInteractionIntents(table);
    store_json(cache_dir, key, out.to_json());
    out
}

// ---- orchestration --------------------------------------------------------------------

function intent_row(cid: string, v: Intention) -> string {
    cid + " | " + squash_ws(v.goal) + " | " + squash_ws(v.mechanism) + "\n"
}

// One wave job: a non-leaf block whose (non-leaf) children already have
// intents. The job batches its leaf children first, then infers its own
// intention grounded in the full child table. Must not throw.
function intent_job(
    cache_dir: string,
    b: CodeBlock,
    by_id: map<string, CodeBlock>,
    interactions: InteractionBlock[],
    intents: map<string, Intention>,
) -> IntentPair[] {
    let out: IntentPair[] = [];
    // (1) batched leaf children
    let leaves: CodeBlock[] = [];
    for (let cid in b.child_ids) {
        match (by_id.get(cid)) {
            let c: CodeBlock => {
                if (is_leaf_block(c)) {
                    leaves.push(c);
                }
            },
            null => null,
        }
    }
    let local: map<string, Intention> = {};
    if (leaves.length() > 0) {
        let leaf_pairs = cached_leaf_intents(cache_dir, b, leaves) catch (_) {
            _ => {
                baml.io.eprintln("[error] leaf intents failed for " + b.id);
                let empty: IntentPair[] = [];
                empty
            }
        };
        for (let lp in leaf_pairs) {
            out.push(lp);
            local.set(lp.block_id, lp.intention);
        }
    }
    // (2) own intention over the full child table
    let table = "";
    for (let cid in b.child_ids) {
        match (local.get(cid)) {
            let v: Intention => {
                table = table + intent_row(cid, v);
            },
            null => {
                match (intents.get(cid)) {
                    let v: Intention => {
                        table = table + intent_row(cid, v);
                    },
                    null => null,
                }
            },
        }
    }
    let neighbors = if (b.depth == 2) {
        neighbor_context(b.id, interactions, by_id, 8)
    } else {
        ""
    };
    let own = cached_block_intent(cache_dir, b, table, neighbors) catch (_) {
        _ => {
            baml.io.eprintln("[error] intent failed for " + b.id);
            Intention { goal: "", mechanism: "", confidence: 1 }
        }
    };
    out.push(IntentPair { block_id: b.id, intention: own });
    out
}

// A sub-batch of one parent group's interactions.
class InterJob {
    parent_id: string,
    batch_no: int,
    pairs: InteractionBlock[],
}

function inter_jobs(interactions: InteractionBlock[]) -> InterJob[] {
    // group by parent_id (interactions arrive sorted by id)
    let parents: string[] = [];
    let grouped: map<string, InteractionBlock[]> = {};
    for (let ib in interactions) {
        let g: InteractionBlock[] = match (grouped.get(ib.parent_id)) {
            let v: InteractionBlock[] => v,
            null => {
                let empty: InteractionBlock[] = [];
                parents.push(ib.parent_id);
                empty
            },
        };
        g.push(ib);
        grouped.set(ib.parent_id, g);
    }
    let jobs: InterJob[] = [];
    for (
        let p in parents
            .sort_by_key(
                (s: string) -> string {
                    s
                },
            )
    ) {
        match (grouped.get(p)) {
            let g: InteractionBlock[] => {
                let batch: InteractionBlock[] = [];
                let batch_no = 0;
                for (let ib in g) {
                    batch.push(ib);
                    if (batch.length() == 25) {
                        jobs.push(InterJob { parent_id: p, batch_no: batch_no, pairs: batch });
                        batch = [];
                        batch_no = batch_no + 1;
                    }
                }
                if (batch.length() > 0) {
                    jobs.push(InterJob { parent_id: p, batch_no: batch_no, pairs: batch });
                }
            },
            null => null,
        }
    }
    jobs
}

// Infer every intention: interaction batches first, then block waves bottom-up
// by depth (3: methods, 2: decls, 1: files, 0: program). Each stage runs on
// the fast pool; results are sorted before returning so downstream stages see
// a deterministic order regardless of scheduling.
function infer_all_intents(
    cache_dir: string,
    blocks: CodeBlock[],
    interactions: InteractionBlock[],
    by_id: map<string, CodeBlock>,
) -> IntentResult {
    let conc = fast_concurrency();

    // ---- interaction batches (independent of block intents) ----
    let ijobs = inter_jobs(interactions);
    let iqueue = WorkQueue { next: 0 };
    let itotal = ijobs.length();
    let iworkers = [];
    let w = 0;
    while (w < conc) {
        iworkers
            .push(
                spawn {
                    let acc: InterPair[] = [];
                    while (true) {
                        let idx = iqueue.take(itotal);
                        if (idx < 0) {
                            break;
                        }
                        match (ijobs.at(idx)) {
                            let job: InterJob => {
                                let pairs = cached_inter_intents(cache_dir, job.parent_id, job.batch_no, job.pairs, by_id) catch (_) {
                                    _ => {
                                        baml.io.eprintln("[error] interaction intents failed for " + job.parent_id);
                                        let empty: InterPair[] = [];
                                        empty
                                    }
                                };
                                for (let p in pairs) {
                                    acc.push(p);
                                }
                            },
                            null => {
                                break;
                            },
                        }
                    }
                    acc
                },
            );
        w = w + 1;
    }
    let inter_pairs: InterPair[] = [];
    for (let wf in iworkers) {
        for (
            let p in await wf
        ) {
            inter_pairs.push(p);
        }
    }
    baml.io.eprintln("[intents] interactions: " + i2s(inter_pairs.length()));

    // ---- block waves, bottom-up ----
    let intents: map<string, Intention> = {};
    let block_pairs: IntentPair[] = [];
    let d = 3;
    while (d >= 0) {
        let wave: CodeBlock[] = blocks
            .filter(
                (b: CodeBlock) -> bool {
                    b.depth == d && !is_leaf_block(b)
                },
            );
        if (wave.length() > 0) {
            let queue = WorkQueue { next: 0 };
            let total = wave.length();
            let workers = [];
            let ww = 0;
            while (ww < conc) {
                workers
                    .push(
                        spawn {
                            let acc: IntentPair[] = [];
                            while (true) {
                                let idx = queue.take(total);
                                if (idx < 0) {
                                    break;
                                }
                                match (wave.at(idx)) {
                                    let b: CodeBlock => {
                                        for (let p in intent_job(cache_dir, b, by_id, interactions, intents)) {
                                            acc.push(p);
                                        }
                                    },
                                    null => {
                                        break;
                                    },
                                }
                            }
                            acc
                        },
                    );
                ww = ww + 1;
            }
            for (let wf in workers) {
                for (
                    let p in await wf
                ) {
                    block_pairs.push(p);
                    intents.set(p.block_id, p.intention);
                }
            }
            baml.io.eprintln("[intents] depth " + i2s(d) + ": " + i2s(wave.length()) + " blocks");
        }
        d = d - 1;
    }

    return IntentResult {
        blocks: block_pairs
            .sort_by_key(
                (p: IntentPair) -> string {
                    p.block_id
                },
            ),
        interactions: inter_pairs
            .sort_by_key(
                (p: InterPair) -> string {
                    p.interaction_id
                },
            ),
    };
}
