// SeedSmoke — the offline end-to-end harness and determinism proof.
//
// Mirrors every cache-key derivation in the pipeline (leaf intents, block
// intents, interaction intents, grades, refutes, omissions) and seeds canned
// judgments, so a subsequent `Run` on the same target/out/catalog executes the
// ENTIRE pipeline — waves, both pools, gates, refutation, report — with zero
// LLM calls. Two such Runs must produce byte-identical reports.
//
//   baml run -f SeedSmoke -- SeedSmoke --target fixtures/smoke --out <out> --catalog fixtures/mini-catalog.md
//   baml run -f Run       -- Run       --target fixtures/smoke --out <out> --catalog fixtures/mini-catalog.md

function canned_intent(id: string) -> Intention {
    Intention { goal: "canned goal " + id, mechanism: "canned mechanism " + id, confidence: 3 }
}

function canned_inter_intent(id: string) -> InteractionIntention {
    InteractionIntention {
        relationship: "canned rel " + id,
        purpose: "canned purpose " + id,
        confidence: 3,
    }
}

// Deterministic canned grade: some subjects get a low grade so the evidence
// gate + refutation paths run; some low grades get refuted so both branches
// render in the report.
function canned_grade_for(subject_id: string, card_id: string) -> int {
    if (((subject_id + card_id).length()) % 4 == 0) {
        3
    } else {
        6
    }
}

function canned_refuted_for(subject_id: string, card_id: string) -> bool {
    ((subject_id + card_id).length()) % 8 == 0
}

// First non-empty line of the graded source — a verbatim quote that passes
// the evidence gate.
function first_line_of(source: string) -> string {
    for (let line in source.lines()) {
        if (line.trim() != "") {
            return line.trim();
        }
    }
    ""
}

function seed_put(cache_dir: string, key: string, value: json) -> null {
    let text = baml.json.to_string(value) catch (_) {
        _ => ""
    };
    if (text != "") {
        cache_put(cache_dir, key, text);
    }
    null
}

function SeedSmoke(target: string, out: string, catalog: string) -> int {
    let catalog_text = baml.fs.read(default_catalog(catalog)) catch (_) {
        _ => ""
    };
    if (catalog_text == "") {
        baml.io.eprintln("cannot read catalog: " + default_catalog(catalog));
        return 1;
    }
    let cards = gradeable_cards(parse_cards(catalog_text));
    let cache_dir = out + "/cache";
    baml.fs.mkdir(cache_dir, baml.fs.MkdirOptions { recursive: true }) catch (_) {
        _ => null
    };

    // deterministic stages, identical to run_pipeline
    let blocks = build_tree(target);
    let by_id = block_index(blocks);
    let iset = build_interactions(blocks);
    let seeded = 0;

    // canned intents for every block and interaction
    let intent_of: map<string, Intention> = {};
    for (let b in blocks) {
        intent_of.set(b.id, canned_intent(b.id));
    }
    let inter_intent_of: map<string, InteractionIntention> = {};
    for (let ib in iset.interactions) {
        inter_intent_of.set(ib.id, canned_inter_intent(ib.id));
    }

    // ---- (c1) leaf-intent batches: one per non-leaf block with leaf children ----
    for (let b in blocks) {
        if (!is_leaf_block(b)) {
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
            if (leaves.length() > 0) {
                let intents: LeafIntent[] = [];
                for (let l in leaves) {
                    let it = canned_intent(l.id);
                    intents
                        .push(
                            LeafIntent {
                                block_id: l.id,
                                goal: it.goal,
                                mechanism: it.mechanism,
                                confidence: it.confidence,
                            },
                        );
                }
                let key = cache_key("intent-leaf@" + fast_model(), b.id + "\n" + leaf_table(leaves));
                seed_put(cache_dir, key, LeafBatchOut { intents: intents }.to_json());
                seeded = seeded + 1;
            }

            // ---- (c2) the block's own intent, over the canned child table ----
            let table = "";
            for (let cid in b.child_ids) {
                match (intent_of.get(cid)) {
                    let v: Intention => {
                        table = table + intent_row(cid, v);
                    },
                    null => null,
                }
            }
            let neighbors = if (b.depth == 2) {
                neighbor_context(b.id, iset.interactions, by_id, 8)
            } else {
                ""
            };
            let bkey = cache_key("intent@" + fast_model(), b.id + "\n" + b.source + "\n" + table + "\n" + neighbors);
            seed_put(cache_dir, bkey, canned_intent(b.id).to_json());
            seeded = seeded + 1;
        }
    }

    // ---- (c3) interaction-intent batches, same grouping as the pipeline ----
    for (let job in inter_jobs(iset.interactions)) {
        let intents: InterIntent[] = [];
        for (let p in job.pairs) {
            let it = canned_inter_intent(p.id);
            intents
                .push(
                    InterIntent {
                        interaction_id: p.id,
                        relationship: it.relationship,
                        purpose: it.purpose,
                        confidence: it.confidence,
                    },
                );
        }
        let key = cache_key(
            "intent-inter@" + fast_model(),
            job.parent_id + "#" + i2s(job.batch_no) + "\n" + pair_table(job.pairs, by_id),
        );
        seed_put(cache_dir, key, InterBatchOut { intents: intents }.to_json());
        seeded = seeded + 1;
    }

    // ---- (e)+(f) grade jobs, constructed exactly like run_pipeline ----
    let by_card = rules_by_card(routing_rules());
    let jobs: GradeJob[] = [];
    for (let b in blocks) {
        let routed = route_block(b, cards, by_card).cards;
        if (routed.length() > 0) {
            let neighbors = if (b.depth == 2) {
                neighbor_context(b.id, iset.interactions, by_id, 8)
            } else {
                ""
            };
            push_grade_jobs(
                jobs,
                b.id,
                b.kind,
                false,
                truncate(b.source, 6000),
                block_intent_text(intent_of, b.id),
                neighbors,
                b.end_line - b.start_line + 1,
                routed,
            );
        }
    }
    for (let ib in iset.interactions) {
        let src = interaction_source(ib, by_id);
        let routed = route_interaction(ib, src, cards).cards;
        if (routed.length() > 0) {
            push_grade_jobs(
                jobs,
                ib.id,
                ib.kind,
                true,
                src,
                inter_intent_text(inter_intent_of, ib.id),
                "",
                0,
                routed,
            );
        }
    }
    for (let job in jobs) {
        let ids_key = "";
        let bodies = "";
        let grades: CardGrade[] = [];
        for (let c in job.cards) {
            ids_key = ids_key + c.id + ",";
            bodies = bodies + c.body + "\n---\n";
            let grade = canned_grade_for(job.subject_id, c.id);
            let evidence = if (grade <= 4) {
                first_line_of(job.graded_source)
            } else {
                ""
            };
            grades
                .push(
                    CardGrade {
                        card_id: c.id,
                        grade: grade,
                        verdict: verdict_of(grade),
                        evidence: evidence,
                        suggested_form: if (grade <= 4) {
                            "canned intended form"
                        } else {
                            null
                        },
                    },
                );
        }
        let gkey = cache_key(
            "grade@" + judge_model(),
            ids_key
                + "\n"
                + bodies
                + "\n"
                + job.subject_id
                + "\n"
                + job.graded_source
                + "\n"
                + job.intention_text,
        );
        seed_put(cache_dir, gkey, MultiGradeOut { grades: grades }.to_json());
        seeded = seeded + 1;
        // refutes for the low grades (evidence is verbatim, so the gate passes
        // and the pipeline WILL consult the skeptic)
        for (let g in grades) {
            if (g.grade <= 4) {
                let rkey = cache_key(
                    "refute@" + judge_model(),
                    g.card_id
                        + "\n"
                        + job.subject_id
                        + "\n"
                        + job.graded_source
                        + "\n"
                        + i2s(g.grade)
                        + "\n"
                        + g.evidence,
                );
                seed_put(
                    cache_dir,
                    rkey,
                    RefuteOut {
                        refuted: canned_refuted_for(job.subject_id, g.card_id),
                        reason: "canned refutation reason",
                    }
                    .to_json(),
                );
                seeded = seeded + 1;
            }
        }
    }

    // ---- (g) omission scan over the canned decl+file intention table ----
    let intent_table = "";
    for (let b in blocks) {
        if (b.depth <= 2) {
            match (intent_of.get(b.id)) {
                let it: Intention => {
                    intent_table
                        = intent_table
                            + b.id
                            + " | "
                            + b.kind
                            + " | "
                            + squash_ws(it.goal)
                            + " | "
                            + squash_ws(it.mechanism)
                            + "\n";
                },
                null => null,
            }
        }
    }
    let okey = cache_key("omissions@" + judge_model(), intent_table + "\n" + card_index(cards));
    let first_card_id = match (cards.at(0)) {
        let c: PrincipleCard => c.id,
        null => "",
    };
    seed_put(
        cache_dir,
        okey,
        OmissionScanOut {
            omissions: [
                OmissionOut {
                    card_id: first_card_id,
                    description: "canned omission",
                    evidence: "PROGRAM",
                },
            ],
        }
        .to_json(),
    );
    seeded = seeded + 1;

    baml.io
        .println(
            "seeded "
                + i2s(seeded)
                + " judgments ("
                + i2s(jobs.length())
                + " grade jobs) into "
                + cache_dir,
        );
    0
}
