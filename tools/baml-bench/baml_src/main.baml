// intentionbench entry points.
//
//   Bench — the CLI. Packaged with `baml pack Bench -o intentionbench` into a
//   standalone binary that takes the BAML project to inspect, grades it
//   against the embedded design-principles catalog, writes report files, and
//   prints the report in the terminal:
//
//       intentionbench --target <project> [--out <dir>] [--catalog <md>]
//
//   Dev entrypoints (baml run -f <Fn> -- <Fn> --args…):
//       Scan  — deterministic stages only (block tree + interactions); no LLM
//       Cards — parse + list the catalog
//       Run   — Bench minus the embedded catalog default (explicit paths)

function default_catalog(catalog: string) -> string {
    if (catalog != "") {
        catalog
    } else {
        "design-principles.md"
    }
}

// Print the block tree (indented) and interaction summary. No LLM, no env.
function Scan(target: string) -> int {
    let blocks = build_tree(target);
    let by_id = block_index(blocks);
    baml.io.println("blocks: " + i2s(blocks.length()));
    print_subtree("PROGRAM", by_id, "");
    let iset = build_interactions(blocks);
    let by_depth = 0;
    let by_level = 0;
    let refs = 0;
    for (let ib in iset.interactions) {
        if (ib.kind == "by_depth") {
            by_depth = by_depth + 1;
        }
        if (ib.kind == "by_level") {
            by_level = by_level + 1;
        }
        if (ib.kind == "reference") {
            refs = refs + 1;
        }
    }
    baml.io
        .println(
            "interactions: "
                + i2s(iset.interactions.length())
                + " (by_depth "
                + i2s(by_depth)
                + ", by_level "
                + i2s(by_level)
                + ", reference "
                + i2s(refs)
                + "), cap hits: "
                + i2s(iset.cap_hits),
        );
    0
}

function print_subtree(id: string, by_id: map<string, CodeBlock>, indent: string) -> null {
    match (by_id.get(id)) {
        let b: CodeBlock => {
            let span = if (b.depth == 0) {
                ""
            } else {
                " (" + i2s(b.start_line) + "-" + i2s(b.end_line) + ")"
            };
            baml.io.println(indent + b.kind + "\t" + b.id + span);
            for (let c in b.child_ids) {
                print_subtree(c, by_id, indent + "  ");
            }
        },
        null => null,
    }
    null
}

function Cards(catalog: string) -> int {
    let text = baml.fs.read(default_catalog(catalog)) catch (_) {
        _ => ""
    };
    if (text == "") {
        baml.io.eprintln("cannot read catalog: " + default_catalog(catalog));
        return 1;
    }
    let cards = parse_cards(text);
    let gradeable = gradeable_cards(cards);
    baml.io.println("cards: " + i2s(cards.length()) + ", gradeable: " + i2s(gradeable.length()));
    for (let c in cards) {
        baml.io.println("  " + c.id + " [" + c.status + " w=" + f2(c.weight) + "] " + c.title);
    }
    0
}

// The CLI entry point. `catalog` overrides the embedded catalog (useful for
// tests and experiments); `out` defaults to <target>/.intentionbench.
function Bench(target: string, out: string = "", catalog: string = "") -> int {
    let catalog_text = if (catalog != "") {
        baml.fs.read(catalog) catch (_) {
            _ => ""
        }
    } else {
        embedded_catalog()
    };
    if (catalog_text == "") {
        baml.io.eprintln("cannot read catalog: " + catalog);
        return 1;
    }
    let out_dir = if (out != "") {
        out
    } else {
        target + "/.intentionbench"
    };
    run_pipeline(target, out_dir, catalog_text, true)
}

// Dev variant: explicit catalog path, report goes to files only.
function Run(target: string, out: string, catalog: string) -> int {
    let catalog_path = default_catalog(catalog);
    let catalog_text = baml.fs.read(catalog_path) catch (_) {
        _ => ""
    };
    if (catalog_text == "") {
        baml.io.eprintln("cannot read catalog: " + catalog_path);
        return 1;
    }
    run_pipeline(target, out, catalog_text, false)
}

// Render one block's intention lines for grading prompts + cache keys.
function block_intent_text(intents: map<string, Intention>, id: string) -> string {
    match (intents.get(id)) {
        let it: Intention => {
            "goal: " + squash_ws(it.goal) + "\nmechanism: " + squash_ws(it.mechanism)
        },
        null => "",
    }
}

function inter_intent_text(intents: map<string, InteractionIntention>, id: string) -> string {
    match (intents.get(id)) {
        let it: InteractionIntention => {
            "relationship: " + squash_ws(it.relationship) + "\npurpose: " + squash_ws(it.purpose)
        },
        null => "",
    }
}

// Split one subject's routed cards into GradeJobs of <= grade_batch_size().
function push_grade_jobs(
    jobs: GradeJob[],
    subject_id: string,
    subject_kind: string,
    is_interaction: bool,
    graded_source: string,
    intention_text: string,
    neighbors: string,
    span_lines: int,
    routed: PrincipleCard[],
) -> null {
    let batch: PrincipleCard[] = [];
    for (let c in routed) {
        batch.push(c);
        if (batch.length() == grade_batch_size()) {
            jobs.push(
                GradeJob {
                    subject_id: subject_id,
                    subject_kind: subject_kind,
                    is_interaction: is_interaction,
                    graded_source: graded_source,
                    intention_text: intention_text,
                    neighbors: neighbors,
                    span_lines: span_lines,
                    cards: batch,
                },
            );
            batch = [];
        }
    }
    if (batch.length() > 0) {
        jobs.push(
            GradeJob {
                subject_id: subject_id,
                subject_kind: subject_kind,
                is_interaction: is_interaction,
                graded_source: graded_source,
                intention_text: intention_text,
                neighbors: neighbors,
                span_lines: span_lines,
                cards: batch,
            },
        );
    }
    null
}

function run_pipeline(target: string, out: string, catalog_text: string, show_report: bool) -> int {
    let cards = gradeable_cards(parse_cards(catalog_text));
    baml.fs.mkdir(out, baml.fs.MkdirOptions { recursive: true }) catch (_) {
        _ => null
    };
    let cache_dir = out + "/cache";

    // (a) + (b): deterministic tree + interactions
    let blocks = build_tree(target);
    if (blocks.length() <= 1) {
        baml.io.eprintln("no .baml blocks found under " + target);
        return 1;
    }
    let by_id = block_index(blocks);
    let iset = build_interactions(blocks);
    baml.io
        .eprintln(
            "blocks: "
                + i2s(blocks.length())
                + ", interactions: "
                + i2s(iset.interactions.length())
                + ", cards: "
                + i2s(cards.length())
                + ", fast/judge concurrency: "
                + i2s(fast_concurrency())
                + "/"
                + i2s(judge_concurrency()),
        );

    // (c): every intention, bottom-up on the fast pool
    let intents = infer_all_intents(cache_dir, blocks, iset.interactions, by_id);
    let intent_of: map<string, Intention> = {};
    for (let p in intents.blocks) {
        intent_of.set(p.block_id, p.intention);
    }
    let inter_intent_of: map<string, InteractionIntention> = {};
    for (let p in intents.interactions) {
        inter_intent_of.set(p.interaction_id, p.intention);
    }

    // (d): deterministic routing -> grade jobs (blocks, then interactions)
    let by_card = rules_by_card(routing_rules());
    let jobs: GradeJob[] = [];
    let routed_blocks = 0;
    for (let b in blocks) {
        let routed = route_block(b, cards, by_card).cards;
        if (routed.length() > 0) {
            routed_blocks = routed_blocks + 1;
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
    let routed_inters = 0;
    for (let ib in iset.interactions) {
        let src = interaction_source(ib, by_id);
        let routed = route_interaction(ib, src, cards).cards;
        if (routed.length() > 0) {
            routed_inters = routed_inters + 1;
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
    baml.io
        .eprintln(
            "[route] "
                + i2s(routed_blocks)
                + "/"
                + i2s(blocks.length())
                + " blocks, "
                + i2s(routed_inters)
                + "/"
                + i2s(iset.interactions.length())
                + " interactions -> "
                + i2s(jobs.length())
                + " grade jobs",
        );

    // (e)+(f): the judge pool. Each job grades one card batch and refutes its
    // own low grades in parallel. Tasks swallow their own errors.
    let conc = judge_concurrency();
    let queue = WorkQueue { next: 0 };
    let progress = WorkQueue { next: 0 };
    let total = jobs.length();
    let workers = [];
    let w = 0;
    while (w < conc) {
        workers
            .push(
                spawn {
                    let acc: Finding[] = [];
                    while (true) {
                        let idx = queue.take(total);
                        if (idx < 0) {
                            break;
                        }
                        match (jobs.at(idx)) {
                            let job: GradeJob => {
                                let found = run_grade_job(cache_dir, job);
                                for (let f in found) {
                                    acc.push(f);
                                }
                                let done = progress.take(total) + 1;
                                baml.io
                                    .eprintln(
                                        "[grade "
                                            + i2s(done)
                                            + "/"
                                            + i2s(total)
                                            + "] "
                                            + job.subject_id
                                            + " ("
                                            + i2s(job.cards.length())
                                            + " cards, "
                                            + i2s(found.length())
                                            + " findings)",
                                    );
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
    let findings: Finding[] = [];
    for (let wf in workers) {
        for (
            let f in await wf
        ) {
            findings.push(f);
        }
    }

    // (g): omission scan over the decl+file intention table (tree order)
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
    let omissions = cached_omissions(cache_dir, intent_table, cards) catch (_) {
        _ => {
            baml.io.eprintln("[error] omission scan failed");
            let none: OmissionOut[] = [];
            none
        }
    };
    baml.io.eprintln("[omissions] " + i2s(omissions.length()));

    // (h): aggregate + render
    let sorted_findings = findings
        .sort_by_key(
            (f: Finding) -> string {
                f.block_id + "#" + f.card_id
            },
        );
    let report = build_report(target, blocks, iset, cards, sorted_findings, omissions, routed_blocks, blocks.length());
    let json_text = baml.json.to_string(report) catch (_) {
        _ => "{}"
    };
    baml.fs.write(out + "/report.json", json_text) catch (_) {
        _ => null
    };
    let md = render_markdown(report, blocks);
    baml.fs.write(out + "/report.md", md) catch (_) {
        _ => null
    };
    if (show_report) {
        baml.io.println(md);
    }
    baml.io
        .println(
            "adherence: "
                + f2(report.adherence_score)
                + "  commission: "
                + f2(report.commission_score)
                + "  omission: "
                + f2(report.omission_score),
        );
    baml.io.println("wrote " + out + "/report.md and " + out + "/report.json");
    0
}
