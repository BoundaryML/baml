// Entry points.
//
//   Bench — the CLI. Packaged with `baml pack Bench -o baml-bench` into a
//   standalone binary that takes the BAML project to inspect, grades it against
//   the embedded design-principles catalog, writes report files, and prints the
//   report in the terminal:
//
//       baml-bench --target <project> [--out <dir>] [--catalog <md>]
//
//   Dev entrypoints (baml run -f <Fn> -- <Fn> --args…):
//       Scan  — deterministic stages only (chunks + edges); no LLM, no env
//       Cards — parse + list the catalog
//       Run   — Bench minus the embedded catalog default (explicit paths)
//
// LLM work runs on a spawn worker pool: BENCH_CONCURRENCY green threads
// (default 16) pull chunks from a shared queue, and each task carries its
// chunk END-TO-END (analyze -> grade -> refute) — no wave barriers, no
// analyze/grade phase barrier, so a slow chunk never stalls its neighbors.
// The shared queue counter is safe because green threads are cooperative:
// take() has no await point, so its read-increment is atomic. The
// content-addressed cache makes ordering irrelevant to determinism: keys
// depend only on inputs, and all aggregation happens on sorted collections.

// Shared work queue / progress counter for the worker pool.
class WorkQueue {
    next: int,

    // Claim the next index below `limit`; -1 when the queue is drained.
    // No await inside -> atomic under cooperative scheduling.
    function take(self, limit: int) -> int {
        if (self.next >= limit) {
            return -1;
        }
        let i = self.next;
        self.next = self.next + 1;
        i
    }
}

// One chunk's complete result, produced inside a worker task.
class PerChunk {
    chunk_id: string,
    analysis: ChunkAnalysis,
    findings: Finding[],
}

function bench_concurrency() -> int {
    let raw = baml.env.get("BENCH_CONCURRENCY") catch (_) {
        _ => null
    };
    let n = match (raw) {
        let v: string => {
            baml.json.from_string<int>(v) catch (_) {
                _ => 16
            }
        },
        null => 16,
    };
    n.clamp(1, 64)
}

function default_catalog(catalog: string) -> string {
    if (catalog != "") {
        catalog
    } else {
        "design-principles.md"
    }
}

function Scan(target: string) -> int {
    let chunks = chunk_project(target);
    let edges = build_edges(chunks);
    baml.io.println("chunks: " + i2s(chunks.length()) + ", edges: " + i2s(edges.length()));
    for (let c in chunks) {
        baml.io
            .println(
                "  " + c.kind + "\t" + c.id + " (" + i2s(c.start_line) + "-" + i2s(c.end_line) + ")",
            );
    }
    0
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
// tests and experiments); `out` defaults to <target>/.baml-bench.
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
        target + "/.baml-bench"
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

// All the work one chunk needs after its analysis: one multi-card grading call,
// the deterministic evidence gate, and refutation of surviving low grades.
// Runs inside a spawned task; must not throw (errors -> empty findings).
function grade_chunk(
    cache_dir: string,
    c: Chunk,
    analysis: ChunkAnalysis,
    cards: PrincipleCard[],
    neighbors: string,
) -> Finding[] {
    let findings: Finding[] = [];
    if (cards.length() == 0) {
        return findings;
    }
    let grades = cached_grades(cache_dir, c, cards, analysis, neighbors) catch (_) {
        _ => {
            baml.io.eprintln("[error] grading failed for " + c.id);
            let empty: CardGrade[] = [];
            empty
        }
    };
    let card_by_id: map<string, PrincipleCard> = {};
    for (let card in cards) {
        card_by_id.set(card.id, card);
    }
    for (let g in grades) {
        match (card_by_id.get(g.card_id)) {
            let card: PrincipleCard => {
                let verified = evidence_in_source(g.evidence, c.source);
                // deterministic gates: a grade without a real quote is
                // neutralized; low grades face the skeptic.
                let effective = if (verified) {
                    g.grade
                } else {
                    5
                };
                let verdict = if (verified) {
                    g.verdict
                } else {
                    "neutral"
                };
                let refuted = false;
                let refute_reason: string? = null;
                if (verified && effective <= 4) {
                    let r = cached_refute(cache_dir, c, card, g) catch (_) {
                        _ => RefuteOut { refuted: false, reason: "" }
                    };
                    if (r.refuted) {
                        refuted = true;
                        refute_reason = r.reason;
                        effective = 5;
                        verdict = "neutral";
                    }
                }
                findings
                    .push(
                        Finding {
                            chunk_id: c.id,
                            card_id: card.id,
                            grade: effective,
                            raw_grade: g.grade,
                            verdict: verdict,
                            evidence: g.evidence,
                            evidence_verified: verified,
                            suggested_form: g.suggested_form,
                            refuted: refuted,
                            refute_reason: refute_reason,
                            weight: card.weight,
                        },
                    );
                baml.io
                    .eprintln(
                        "[grade] "
                            + c.id
                            + " × "
                            + card.id
                            + " = "
                            + i2s(effective)
                            + " ("
                            + verdict
                            + ")",
                    );
            },
            null => null,
        }
    }
    findings
}

function run_pipeline(target: string, out: string, catalog_text: string, show_report: bool) -> int {
    let cards = gradeable_cards(parse_cards(catalog_text));
    baml.fs.mkdir(out, baml.fs.MkdirOptions { recursive: true }) catch (_) {
        _ => null
    };
    let cache_dir = out + "/cache";
    let conc = bench_concurrency();

    // (a) + (b): deterministic
    let chunks = chunk_project(target)
        .filter(
            (c: Chunk) -> bool {
                c.kind != "config"
            },
        );
    if (chunks.length() == 0) {
        baml.io.eprintln("no .baml chunks found under " + target);
        return 1;
    }
    let edges = build_edges(chunks);
    baml.io
        .eprintln(
            "chunks: "
                + i2s(chunks.length())
                + ", edges: "
                + i2s(edges.length())
                + ", cards: "
                + i2s(cards.length())
                + ", concurrency: "
                + i2s(conc),
        );

    // (c)+(f): the worker pool. Each task takes a chunk end-to-end —
    // analyze (Haiku), then grade all routed cards (Sonnet), then refute low
    // grades — so there are no phase barriers and a slow chunk never stalls
    // the pool. Tasks swallow their own errors (BEP-034 fire-and-forget rule).
    let card_by_id: map<string, PrincipleCard> = {};
    for (let card in cards) {
        card_by_id.set(card.id, card);
    }
    let queue = WorkQueue { next: 0 };
    let progress = WorkQueue { next: 0 };
    let total = chunks.length();
    let workers = [];
    let w = 0;
    while (w < conc) {
        workers
            .push(
                spawn {
                    let out: PerChunk[] = [];
                    while (true) {
                        let idx = queue.take(total);
                        if (idx < 0) {
                            break;
                        }
                        let c = match (chunks.at(idx)) {
                            let v: Chunk => v,
                            null => {
                                break;
                            },
                        };
                        let neighbors = neighbor_context(c.id, edges, chunks, 8);
                        let a = cached_analyze(cache_dir, c, neighbors, prefilter_for(c, cards)) catch (_) {
                            _ => {
                                baml.io.eprintln("[error] analyze failed for " + c.id);
                                let none: string[] = [];
                                ChunkAnalysis { goal: "", mechanism: "", confidence: 1, card_ids: none }
                            }
                        };
                        let routed: PrincipleCard[] = [];
                        for (let id in a.card_ids) {
                            match (card_by_id.get(id)) {
                                let card: PrincipleCard => {
                                    routed.push(card);
                                },
                                null => null,
                            }
                        }
                        let found = grade_chunk(cache_dir, c, a, routed, neighbors);
                        out.push(PerChunk { chunk_id: c.id, analysis: a, findings: found });
                        let done = progress.take(total) + 1;
                        baml.io
                            .eprintln(
                                "[chunk "
                                    + i2s(done)
                                    + "/"
                                    + i2s(total)
                                    + "] "
                                    + c.id
                                    + " -> "
                                    + i2s(a.card_ids.length())
                                    + " cards, "
                                    + i2s(found.length())
                                    + " findings: "
                                    + a.goal,
                            );
                    }
                    out
                },
            );
        w = w + 1;
    }
    let analyses: map<string, ChunkAnalysis> = {};
    let findings: Finding[] = [];
    for (let wf in workers) {
        for (
            let pc in await wf
        ) {
            analyses.set(pc.chunk_id, pc.analysis);
            for (let finding in pc.findings) {
                findings.push(finding);
            }
        }
    }

    // intention table in chunk order (deterministic regardless of pool timing)
    let intent_table = "";
    let routed_chunks = 0;
    for (let c in chunks) {
        match (analyses.get(c.id)) {
            let a: ChunkAnalysis => {
                intent_table
                    = intent_table
                        + c.id
                        + " | "
                        + c.kind
                        + " | "
                        + a.goal
                        + " | "
                        + a.mechanism
                        + "\n";
                if (a.card_ids.length() > 0) {
                    routed_chunks = routed_chunks + 1;
                }
            },
            null => null,
        }
    }

    // (g): omission scan over the whole intention table
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
                f.chunk_id + "#" + f.card_id
            },
        );
    let report = build_report(target, chunks, edges, cards, sorted_findings, omissions, routed_chunks);
    let json_text = baml.json.to_string(report) catch (_) {
        _ => "{}"
    };
    baml.fs.write(out + "/report.json", json_text) catch (_) {
        _ => null
    };
    let md = render_markdown(report);
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
