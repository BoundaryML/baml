// Token-free tests for deterministic block + interaction routing.

function mk_card(id: string) -> PrincipleCard {
    PrincipleCard {
        id: id,
        title: "t-" + id,
        status: "implemented",
        weight: 1.0,
        trigger: "",
        detectability: "",
        body: "### " + id + ": t\n",
    }
}

function mk_block(id: string, kind: string, depth: int, source: string) -> CodeBlock {
    let none: string[] = [];
    CodeBlock {
        id: id,
        parent_id: null,
        kind: kind,
        name: id,
        file: "x.baml",
        start_line: 1,
        end_line: 1,
        source: source,
        child_ids: none,
        depth: depth,
    }
}

function routed_ids(r: Routed) -> string[] {
    let out: string[] = [];
    for (let c in r.cards) {
        out.push(c.id);
    }
    out
}

testset "routing_blocks" {
    test "gate cards fire on keywords, hard-no without them" {
        let cards = [mk_card("P-047-1"), mk_card("P-999-1")];
        let by_card = rules_by_card(routing_rules());
        let plain = mk_block("D:x.baml:f", "function", 2, "function f() -> int {\n    1\n}\n");
        let r1 = routed_ids(route_block(plain, cards, by_card));
        // no sql signal: P-047-1 excluded; no-rule card routed (non-leaf)
        assert.is_true(r1.includes("P-047-1") == false);
        assert.is_true(r1.includes("P-999-1"));
        let sql = mk_block("D:x.baml:q", "function", 2, "function q() -> string {\n    \"SELECT * FROM t\"\n}\n");
        let r2 = routed_ids(route_block(sql, cards, by_card));
        assert.is_true(r2.includes("P-047-1"));
    }

    test "static kind rules route llm_functions and tests" {
        let cards = [mk_card("P-036-2"), mk_card("P-023-2")];
        let by_card = rules_by_card(routing_rules());
        let llmfn = mk_block(
            "D:x.baml:Ask",
            "llm_function",
            2,
            "function Ask() -> T {\n    client: X\n    prompt: #\"hi\"#\n}\n",
        );
        assert.is_true(routed_ids(route_block(llmfn, cards, by_card)).includes("P-036-2"));
        let t = mk_block("D:x.baml:t", "test", 2, "test \"t\" {\n    assert.equal(1, 1);\n}\n");
        assert.is_true(routed_ids(route_block(t, cards, by_card)).includes("P-023-2"));
        // and not vice versa
        assert.is_true(routed_ids(route_block(t, [mk_card("P-036-2")], by_card)).length() == 0);
    }

    test "leaves route keyword gates but never the no-rule bucket" {
        let cards = [mk_card("P-021-1"), mk_card("P-999-1")];
        let by_card = rules_by_card(routing_rules());
        let v = mk_block("V:x.baml:f.ts#1", "var", 3, "let ts = epoch_ms / 1000;\n");
        let r = routed_ids(route_block(v, cards, by_card));
        // "epoch" fires the time gate; the no-rule card stays out
        assert.is_true(r.includes("P-021-1"));
        assert.is_true(r.includes("P-999-1") == false);
    }

    test "file and program blocks route only the no-rule bucket" {
        let cards = [mk_card("P-047-1"), mk_card("P-999-1")];
        let by_card = rules_by_card(routing_rules());
        let file = mk_block("F:x.baml", "file", 1, "function q() -> string {\n    \"SELECT * FROM t\"\n}\n");
        let r = routed_ids(route_block(file, cards, by_card));
        assert.is_true(r.includes("P-047-1") == false);
        assert.is_true(r.includes("P-999-1"));
    }

    test "config blocks never route the no-rule bucket" {
        let cards = [mk_card("P-999-1")];
        let by_card = rules_by_card(routing_rules());
        let cfg = mk_block("D:x.baml:Grader", "config", 2, "client<llm> Grader {\n    provider: anthropic,\n}\n");
        assert.equal(routed_ids(route_block(cfg, cards, by_card)).length(), 0);
    }
}

testset "routing_interactions" {
    test "interaction rules fire on edge_kind plus keyword" {
        let cards = [mk_card("P-044-1"), mk_card("P-023-4"), mk_card("P-999-1")];
        let uses = InteractionBlock {
            id: "I:r:D:x.baml:f->D:x.baml:T",
            kind: "reference",
            a_id: "D:x.baml:f",
            b_id: "D:x.baml:T",
            parent_id: "F:x.baml",
            edge_kind: "uses_type",
            evidenced: true,
        };
        let src_match = "# block A\nfunction f(t: T) -> int {\n    match (t) { ... }\n}\n# block B\ninterface T {}\n";
        let r1 = routed_ids(route_interaction(uses, src_match, cards));
        assert.is_true(r1.includes("P-044-1"));
        assert.is_true(r1.includes("P-999-1") == false);
        // wrong edge kind: the same keywords route nothing
        let sib = InteractionBlock {
            id: "I:l:a~b",
            kind: "by_level",
            a_id: "a",
            b_id: "b",
            parent_id: "F:x.baml",
            edge_kind: "sibling",
            evidenced: false,
        };
        assert
            .equal(routed_ids(route_interaction(sib, src_match, [mk_card("P-044-1")])).length(), 0);
        // exercises + assert routes the test-shape card
        let ex = InteractionBlock {
            id: "I:r:D:x.baml:t->D:x.baml:f",
            kind: "reference",
            a_id: "D:x.baml:t",
            b_id: "D:x.baml:f",
            parent_id: "F:x.baml",
            edge_kind: "exercises",
            evidenced: true,
        };
        let r2 = routed_ids(
            route_interaction(
                ex,
                "# block A\ntest \"t\" {\n    assert.equal(f(), 1);\n}\n# block B\nfunction f() -> int {\n    1\n}\n",
                cards,
            ),
        );
        assert.is_true(r2.includes("P-023-4"));
    }
}
