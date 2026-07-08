// Token-free cache-replay tests for grading, refutation, weights, and report.

function gt_card(id: string) -> PrincipleCard {
    PrincipleCard {
        id: id,
        title: "t-" + id,
        status: "implemented",
        weight: 1.0,
        trigger: "any",
        detectability: "static",
        body: "### " + id + ": t\n",
    }
}

function gt_job(cards: PrincipleCard[], source: string) -> GradeJob {
    GradeJob {
        subject_id: "D:x.baml:f",
        subject_kind: "function",
        is_interaction: false,
        graded_source: source,
        intention_text: "goal: g\nmechanism: m",
        neighbors: "",
        span_lines: 3,
        cards: cards,
    }
}

function gt_grade_key(job: GradeJob) -> string {
    let ids_key = "";
    let bodies = "";
    for (let c in job.cards) {
        ids_key = ids_key + c.id + ",";
        bodies = bodies + c.body + "\n---\n";
    }
    cache_key(
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
    )
}

testset "grade_cache" {
    test "cached_grades reconciles: unknown dropped, missing neutral, verdict derived" {
        let dir = "/tmp/intentionbench-test-grade";
        let source = "function f() -> int {\n    let x = 1;\n    x\n}\n";
        let job = gt_job([gt_card("P-000-1"), gt_card("P-000-2")], source);
        let key = gt_grade_key(job);
        // model graded P-000-1 with a wrong verdict word + out-of-range grade,
        // answered a hallucinated card, and skipped P-000-2
        cache_put(
            dir,
            key,
            "{\"grades\": [{\"card_id\": \"P-000-1\", \"grade\": 12, \"verdict\": \"fighting\", \"evidence\": \"let x = 1;\", \"suggested_form\": null}, {\"card_id\": \"P-999-9\", \"grade\": 1, \"verdict\": \"reinventing\", \"evidence\": \"nope\", \"suggested_form\": null}]}",
        );
        let grades = cached_grades(dir, job);
        assert.equal(grades.length(), 2);
        match (grades.at(0)) {
            let g: CardGrade => {
                assert.equal(g.card_id, "P-000-1");
                assert.equal(g.grade, 10); // clamped
                assert.equal(g.verdict, "adherent"); // derived from band, not model prose
            },
            null => assert.is_true(false),
        }
        match (grades.at(1)) {
            let g: CardGrade => {
                assert.equal(g.card_id, "P-000-2");
                assert.equal(g.grade, 5);
                assert.equal(g.evidence, "");
            },
            null => assert.is_true(false),
        }
        baml.fs.remove(cache_path(dir, key)) catch (_) {
            _ => null
        };
    }

    test "run_grade_job: evidence gate neutralizes, refuted low grade goes neutral" {
        let dir = "/tmp/intentionbench-test-gradejob";
        let source = "function f() -> int {\n    let ts = epoch / 1000;\n    ts\n}\n";
        let job = gt_job([gt_card("P-021-9"), gt_card("P-022-9")], source);
        let key = gt_grade_key(job);
        // P-021-9: real quote, grade 3 -> refutation path (seeded: refuted)
        // P-022-9: fake quote, grade 2 -> evidence gate neutralizes, no refute
        cache_put(
            dir,
            key,
            "{\"grades\": [{\"card_id\": \"P-021-9\", \"grade\": 3, \"verdict\": \"fighting\", \"evidence\": \"let ts = epoch / 1000;\", \"suggested_form\": \"use time\"}, {\"card_id\": \"P-022-9\", \"grade\": 2, \"verdict\": \"reinventing\", \"evidence\": \"not in the source\", \"suggested_form\": null}]}",
        );
        let g_seed = CardGrade {
            card_id: "P-021-9",
            grade: 3,
            verdict: "fighting",
            evidence: "let ts = epoch / 1000;",
            suggested_form: "use time",
        };
        let rkey = cache_key(
            "refute@" + judge_model(),
            "P-021-9"
                + "\n"
                + job.subject_id
                + "\n"
                + job.graded_source
                + "\n"
                + i2s(g_seed.grade)
                + "\n"
                + g_seed.evidence,
        );
        cache_put(dir, rkey, "{\"refuted\": true, \"reason\": \"wire format requires epoch\"}");
        let findings = run_grade_job(dir, job);
        assert.equal(findings.length(), 2);
        match (findings.at(0)) {
            let f: Finding => {
                assert.equal(f.card_id, "P-021-9");
                assert.is_true(f.refuted);
                assert.equal(f.grade, 5); // refutation neutralized
                assert.equal(f.verdict, "neutral");
                assert.equal(f.raw_grade, 3);
            },
            null => assert.is_true(false),
        }
        match (findings.at(1)) {
            let f: Finding => {
                assert.equal(f.card_id, "P-022-9");
                assert.is_true(f.evidence_verified == false);
                assert.equal(f.grade, 5); // gate neutralized
                assert.is_true(f.refuted == false); // gate-failed grades never reach the skeptic
            },
            null => assert.is_true(false),
        }
        baml.fs.remove(cache_path(dir, key)) catch (_) {
            _ => null
        };
        baml.fs.remove(cache_path(dir, rkey)) catch (_) {
            _ => null
        };
    }
}

testset "score" {
    test "finding_weight composes card, kind and size" {
        // decl function, 50 lines: 1.0 * 1.0 * 1.6
        assert.is_true(finding_weight(1.0, "function", 50, false) == 1.6);
        // leaf var: size factor pinned to 1.0
        assert.is_true(finding_weight(1.0, "var", 50, false) == 0.4);
        // interaction reference: 0.7, size pinned
        assert.is_true(finding_weight(1.0, "reference", 0, true) == 0.7);
        // status weight scales through
        assert.is_true(finding_weight(0.5, "program", 0, false) == 0.25);
    }

    test "compute_scores: weighted mean plus commission over non-neutral" {
        let mk = (id: string, grade: int, verdict: string, weight: float) -> Finding {
            Finding {
                block_id: id,
                block_kind: "function",
                is_interaction: false,
                card_id: "P-1",
                grade: grade,
                raw_grade: grade,
                verdict: verdict,
                evidence: "e",
                evidence_verified: true,
                suggested_form: null,
                refuted: false,
                refute_reason: null,
                weight: weight,
            }
        };
        let findings = [mk("a", 10, "adherent", 1.0), mk("b", 2, "reinventing", 1.0), mk("c", 5, "neutral", 2.0)];
        let s = compute_scores(findings);
        // adherence = (10 + 2 + 10) / 4 = 5.5 ; commission = (10+2)/2 = 6
        assert.equal(f2(s.adherence), "5.50");
        assert.equal(f2(s.commission), "6.00");
    }
}

testset "report" {
    test "report builds and renders with kinds, tree, and interaction findings" {
        let none: string[] = [];
        let blocks = [
            CodeBlock {
                id: "PROGRAM",
                parent_id: null,
                kind: "program",
                name: "PROGRAM",
                file: "",
                start_line: 0,
                end_line: 0,
                source: "",
                child_ids: ["F:x.baml"],
                depth: 0,
            },
            CodeBlock {
                id: "F:x.baml",
                parent_id: "PROGRAM",
                kind: "file",
                name: "x.baml",
                file: "x.baml",
                start_line: 1,
                end_line: 3,
                source: "",
                child_ids: ["D:x.baml:f"],
                depth: 1,
            },
            CodeBlock {
                id: "D:x.baml:f",
                parent_id: "F:x.baml",
                kind: "function",
                name: "f",
                file: "x.baml",
                start_line: 1,
                end_line: 3,
                source: "function f() -> int {\n    1\n}\n",
                child_ids: none,
                depth: 2,
            },
        ];
        let iset = InteractionSet {
            interactions: [
                InteractionBlock {
                    id: "I:d:D:x.baml:f^F:x.baml",
                    kind: "by_depth",
                    a_id: "D:x.baml:f",
                    b_id: "F:x.baml",
                    parent_id: "F:x.baml",
                    edge_kind: "contains",
                    evidenced: true,
                },
            ],
            cap_hits: 0,
        };
        let findings = [
            Finding {
                block_id: "D:x.baml:f",
                block_kind: "function",
                is_interaction: false,
                card_id: "P-000-1",
                grade: 3,
                raw_grade: 3,
                verdict: "fighting",
                evidence: "function f",
                evidence_verified: true,
                suggested_form: "use g",
                refuted: false,
                refute_reason: null,
                weight: 1.0,
            },
            Finding {
                block_id: "I:d:D:x.baml:f^F:x.baml",
                block_kind: "by_depth",
                is_interaction: true,
                card_id: "P-000-1",
                grade: 9,
                raw_grade: 9,
                verdict: "adherent",
                evidence: "function f",
                evidence_verified: true,
                suggested_form: null,
                refuted: false,
                refute_reason: null,
                weight: 0.5,
            },
        ];
        let omissions: OmissionOut[] = [];
        let r = build_report("x", blocks, iset, [gt_card("P-000-1")], findings, omissions, 1, 3);
        assert.equal(r.graded_pairs, 2);
        assert.equal(r.per_kind.length(), 2);
        // weighted: (1.0*3 + 0.5*9) / 1.5 = 5.0
        assert.equal(f2(r.adherence_score), "5.00");
        let md = render_markdown(r, blocks);
        assert.is_true(md.includes("Slop report"));
        assert.is_true(md.includes("D:x.baml:f (function) × P-000-1"));
        assert.is_true(md.includes("## Per-kind"));
        assert.is_true(md.includes("## Tree"));
        assert.is_true(md.includes("by_depth"));
    }
}
