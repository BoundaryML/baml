// test helpers: unwrap Array.at() (returns T?) with loud sentinels
function nth_chunk(chunks: Chunk[], i: int) -> Chunk {
    match (chunks.at(i)) {
        let c: Chunk => c,
        null => {
            Chunk {
                id: "MISSING",
                file: "",
                name: "MISSING",
                kind: "",
                start_line: -1,
                end_line: -1,
                source: "",
            }
        },
    }
}

function nth_card(cards: PrincipleCard[], i: int) -> PrincipleCard {
    match (cards.at(i)) {
        let c: PrincipleCard => c,
        null => {
            PrincipleCard {
                id: "MISSING",
                title: "",
                status: "",
                weight: -1.0,
                trigger: "",
                detectability: "",
                body: "",
            }
        },
    }
}

// Token-free tests: the deterministic stages directly, and the cached LLM
// paths by pre-seeding the content-addressed cache so no model is ever called.

testset "chunker" {
    test "chunks top-level decls with correct kinds and spans" {
        let src = "// header comment\n"
            + "class User {\n    name: string,\n}\n"
            + "\n"
            + "function greet(u: User) -> string {\n    \"hi \" + u.name\n}\n"
            + "\n"
            + "function Ask(q: string) -> string {\n    client: Grader\n    prompt: #\"\n        {{ q }} has a } brace and a { too\n    \"#\n}\n"
            + "\n"
            + "test \"greet works\" {\n    assert.equal(greet(User { name: \"a\" }), \"hi a\");\n}\n";
        let chunks = chunk_file("x.baml", src);
        assert.equal(chunks.length(), 4);
        let c0 = nth_chunk(chunks, 0);
        assert.equal(c0.name, "User");
        assert.equal(c0.kind, "type");
        assert.equal(c0.start_line, 2);
        assert.equal(c0.end_line, 4);
        let c1 = nth_chunk(chunks, 1);
        assert.equal(c1.name, "greet");
        assert.equal(c1.kind, "function");
        let c2 = nth_chunk(chunks, 2);
        assert.equal(c2.name, "Ask");
        assert.equal(c2.kind, "llm_function");
        let c3 = nth_chunk(chunks, 3);
        assert.equal(c3.name, "greet works");
        assert.equal(c3.kind, "test");
    }

    test "braces inside strings, raw strings and comments don't split chunks" {
        let src = "function f() -> string {\n"
            + "    // a comment with a stray {\n"
            + "    let s = \"literal } brace\";\n"
            + "    s\n"
            + "}\n";
        let chunks = chunk_file("y.baml", src);
        assert.equal(chunks.length(), 1);
        assert.equal(nth_chunk(chunks, 0).end_line, 5);
    }

    test "braceless type alias is a single-line chunk" {
        let chunks = chunk_file("z.baml", "type Reply = \"yes\" | \"no\"\n");
        assert.equal(chunks.length(), 1);
        assert.equal(nth_chunk(chunks, 0).name, "Reply");
    }
}

testset "interactions" {
    test "edges connect callers, types, and tests" {
        let src = "class User {\n    name: string,\n}\n"
            + "function greet(u: User) -> string {\n    u.name\n}\n"
            + "test \"t\" {\n    greet(User { name: \"a\" });\n}\n";
        let chunks = chunk_file("x.baml", src);
        let edges = build_edges(chunks);
        // greet -> User (uses_type), test -> greet + test -> User (exercises)
        let kinds: string[] = [];
        for (let e in edges) {
            kinds.push(e.from_id + "|" + e.to_id + "|" + e.kind);
        }
        assert.is_true(kinds.includes("x.baml:greet|x.baml:User|uses_type"));
        assert.is_true(kinds.includes("x.baml:t|x.baml:greet|exercises"));
    }
}

testset "cards" {
    test "parses cards with status, weight, trigger" {
        let md = "# Catalog\n\n## Batch\n\n"
            + "### P-001-1: First principle\n"
            + "- **Source**: BEP-001 (Thing, status: implemented)\n"
            + "- **Principle**: Do the thing.\n"
            + "- **Applicability trigger**: any chunk that things.\n"
            + "- **Detectability**: static.\n"
            + "\n"
            + "### P-002-1: Draft principle\n"
            + "- **Source**: BEP-002 (Other, status: **draft**)\n"
            + "- **Applicability trigger**: never yet.\n"
            + "\n"
            + "## Next section\n";
        let cards = parse_cards(md);
        assert.equal(cards.length(), 2);
        let c0 = nth_card(cards, 0);
        assert.equal(c0.id, "P-001-1");
        assert.equal(c0.status, "implemented");
        assert.is_true(c0.weight == 1.0);
        assert.equal(c0.trigger, "any chunk that things.");
        let c1 = nth_card(cards, 1);
        assert.equal(c1.status, "draft");
        assert.is_true(c1.weight == 0.0);
        assert.equal(gradeable_cards(cards).length(), 1);
    }

    test "the real catalog parses to 144 cards" {
        let text = baml.fs.read("design-principles.md") catch (_) {
            _ => ""
        };
        // skipped silently when run from another cwd
        if (text != "") {
            assert.equal(parse_cards(text).length(), 144);
        }
    }
}

testset "evidence_gate" {
    test "whitespace-normalized quote matching" {
        let source = "function f() {\n    let x =   compute( a,\n        b );\n}\n";
        assert.is_true(evidence_in_source("let x = compute( a, b );", source));
        assert.is_true(evidence_in_source("compute( a,\n        b )", source));
        assert.is_true(evidence_in_source("let y = other()", source) == false);
        assert.is_true(evidence_in_source("", source) == false);
    }
}

testset "cache" {
    test "sha256 key round-trips through put/get" {
        let dir = "/tmp/adherence-test-cache";
        let key = cache_key("test", "payload");
        assert.is_true(key.length() == 64);
        cache_put(dir, key, "{\"a\":1}");
        match (cache_get(dir, key)) {
            let hit: string => assert.equal(hit, "{\"a\":1}"),
            null => assert.is_true(false),
        }
        baml.fs.remove(cache_path(dir, key)) catch (_) {
            _ => null
        };
    }

    test "cached_grades replays from a seeded cache without any LLM call" {
        let dir = "/tmp/adherence-test-cache2";
        let chunk = Chunk {
            id: "x.baml:f",
            file: "x.baml",
            name: "f",
            kind: "function",
            start_line: 1,
            end_line: 3,
            source: "function f() -> int {\n    1\n}\n",
        };
        let card = PrincipleCard {
            id: "P-000-0",
            title: "t",
            status: "implemented",
            weight: 1.0,
            trigger: "any",
            detectability: "static",
            body: "### P-000-0: t\n",
        };
        let analysis = ChunkAnalysis { goal: "g", mechanism: "m", confidence: 3, card_ids: ["P-000-0"] };
        let ids_key = card.id + ",";
        let bodies = card.body + "\n---\n";
        let key = cache_key("grade@" + judge_model(), ids_key + "\n" + bodies + "\n" + chunk.id + "\n" + chunk.source);
        cache_put(
            dir,
            key,
            "{\"grades\": [{\"card_id\": \"P-000-0\", \"grade\": 3, \"verdict\": \"fighting\", \"evidence\": \"function f() -> int {\", \"suggested_form\": null}]}",
        );
        let grades = cached_grades(dir, chunk, [card], analysis, "");
        assert.equal(grades.length(), 1);
        let g = match (grades.at(0)) {
            let v: CardGrade => v,
            null => {
                CardGrade {
                    card_id: "",
                    grade: -1,
                    verdict: "",
                    evidence: "",
                    suggested_form: null,
                }
            },
        };
        assert.equal(g.grade, 3);
        assert.equal(g.verdict, "fighting");
        assert.is_true(evidence_in_source(g.evidence, chunk.source));
        baml.fs.remove(cache_path(dir, key)) catch (_) {
            _ => null
        };
    }
}

testset "render" {
    test "f2 formats floats deterministically" {
        assert.equal(f2(7.125), "7.13");
        assert.equal(f2(7.0), "7.00");
        assert.equal(f2(0.5), "0.50");
    }

    test "report builds and renders" {
        let chunks = chunk_file("x.baml", "function f() -> int {\n    1\n}\n");
        let cards: PrincipleCard[] = [
            PrincipleCard {
                id: "P-000-0",
                title: "t",
                status: "implemented",
                weight: 1.0,
                trigger: "any",
                detectability: "static",
                body: "b",
            },
        ];
        let findings: Finding[] = [
            Finding {
                chunk_id: "x.baml:f",
                card_id: "P-000-0",
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
        ];
        let edges: Edge[] = [];
        let omissions: OmissionOut[] = [];
        let r = build_report("x", chunks, edges, cards, findings, omissions, 1);
        assert.is_true(r.adherence_score == 3.0);
        assert.equal(r.graded_pairs, 1);
        let md = render_markdown(r);
        assert.is_true(md.includes("Slop report"));
        assert.is_true(md.includes("x.baml:f × P-000-0"));
    }
}

testset "prefilter" {
    test "gate cards are excluded without their signal and included with it" {
        let cards: PrincipleCard[] = [
            PrincipleCard {
                id: "P-047-1",
                title: "sql params",
                status: "implemented",
                weight: 1.0,
                trigger: "sql",
                detectability: "static",
                body: "b",
            },
            PrincipleCard {
                id: "P-036-2",
                title: "output format",
                status: "implemented",
                weight: 1.0,
                trigger: "llm fn",
                detectability: "static",
                body: "b",
            },
            PrincipleCard {
                id: "P-999-1",
                title: "no rules card",
                status: "implemented",
                weight: 1.0,
                trigger: "anything",
                detectability: "semantic",
                body: "b",
            },
        ];
        // plain function, no sql anywhere: P-047-1 hard-no'd, P-036-2 not static
        // (wrong kind), unknown card always a candidate
        let plain = Chunk {
            id: "a.baml:f",
            file: "a.baml",
            name: "f",
            kind: "function",
            start_line: 1,
            end_line: 1,
            source: "function f() -> int {\n    1\n}\n",
        };
        let pre1 = prefilter_for(plain, cards);
        assert.equal(pre1.static_ids.length(), 0);
        assert.equal(pre1.candidates.length(), 1);
        // sql-shaped chunk: P-047-1 gate fires -> candidate (LLM confirms)
        let sql = Chunk {
            id: "a.baml:q",
            file: "a.baml",
            name: "q",
            kind: "function",
            start_line: 1,
            end_line: 1,
            source: "function q() -> string {\n    \"SELECT * FROM users WHERE id = \" + id\n}\n",
        };
        let pre2 = prefilter_for(sql, cards);
        assert.equal(pre2.candidates.length(), 2);
        // llm_function: P-036-2 static rule fires -> routed with no LLM opinion
        let llmfn = Chunk {
            id: "a.baml:g",
            file: "a.baml",
            name: "g",
            kind: "llm_function",
            start_line: 1,
            end_line: 1,
            source: "function g() -> T {\n    client: X\n    prompt: #\"hi\"#\n}\n",
        };
        let pre3 = prefilter_for(llmfn, cards);
        assert.is_true(pre3.static_ids.includes("P-036-2"));
    }

    test "merge_routed unions static ids with candidate-validated llm ids" {
        let cards: PrincipleCard[] = [
            PrincipleCard {
                id: "P-111-1",
                title: "c",
                status: "implemented",
                weight: 1.0,
                trigger: "",
                detectability: "",
                body: "b",
            },
        ];
        let pre = Prefilter { static_ids: ["P-222-2"], candidates: cards };
        // llm proposed one real candidate and one hallucinated id
        let merged = merge_routed(pre, ["P-111-1", "P-666-6"]);
        assert.equal(merged.length(), 2);
        assert.is_true(merged.includes("P-111-1"));
        assert.is_true(merged.includes("P-222-2"));
        assert.is_true(merged.includes("P-666-6") == false);
    }
}
