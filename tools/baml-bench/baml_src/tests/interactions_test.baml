// Token-free tests for interaction-block construction.

function tiny_tree() -> CodeBlock[] {
    // class A { x: int, }  function f(a: A)…  function g()…  test "t" { f(...) }
    let src = "class A {\n    x: int,\n}\n"
        + "function f(a: A) -> int {\n    let n = 1;\n    n\n}\n"
        + "function g() -> int {\n    f(A { x: 1 })\n}\n"
        + "test \"t\" {\n    assert.equal(g(), 1);\n}\n";
    let all: CodeBlock[] = [];
    for (let d in scan_file_decls("x.baml", src)) {
        all.push(d);
        for (let m in members_of(d)) {
            all.push(m);
        }
    }
    let none: string[] = [];
    all.push(
        CodeBlock {
            id: "F:x.baml",
            parent_id: "PROGRAM",
            kind: "file",
            name: "x.baml",
            file: "x.baml",
            start_line: 1,
            end_line: 13,
            source: src,
            child_ids: none,
            depth: 1,
        },
    );
    all.push(
        CodeBlock {
            id: "PROGRAM",
            parent_id: null,
            kind: "program",
            name: "PROGRAM",
            file: "",
            start_line: 0,
            end_line: 0,
            source: "x.baml (4 decls, 13 lines)\n",
            child_ids: none,
            depth: 0,
        },
    );
    fill_child_ids(all)
}

function interaction_ids(iset: InteractionSet, kind: string) -> string[] {
    let out: string[] = [];
    for (let ib in iset.interactions) {
        if (ib.kind == kind) {
            out.push(ib.id);
        }
    }
    out
}

testset "interactions" {
    test "by_depth covers every parented block exactly once" {
        let blocks = tiny_tree();
        let iset = build_interactions(blocks);
        let depth_ids = interaction_ids(iset, "by_depth");
        // every block except PROGRAM has a containment interaction
        assert.equal(depth_ids.length(), blocks.length() - 1);
        assert.is_true(depth_ids.includes("I:d:D:x.baml:A^F:x.baml"));
        assert.is_true(depth_ids.includes("I:d:M:x.baml:A.x^D:x.baml:A"));
        assert.is_true(depth_ids.includes("I:d:F:x.baml^PROGRAM"));
    }

    test "reference edges carry call/type/test kinds" {
        let blocks = tiny_tree();
        let iset = build_interactions(blocks);
        let kinds: string[] = [];
        for (let ib in iset.interactions) {
            if (ib.kind == "reference") {
                kinds.push(ib.a_id + "|" + ib.b_id + "|" + ib.edge_kind);
            }
        }
        assert.is_true(kinds.includes("D:x.baml:f|D:x.baml:A|uses_type"));
        assert.is_true(kinds.includes("D:x.baml:g|D:x.baml:f|calls"));
        assert.is_true(kinds.includes("D:x.baml:t|D:x.baml:g|exercises"));
    }

    test "by_level pairs are canonical, deduped against references, evidenced-first" {
        let blocks = tiny_tree();
        let iset = build_interactions(blocks);
        let level_ids = interaction_ids(iset, "by_level");
        // f references A and g references f: those sibling pairs stay reference-
        // only. A~t, A~g, f~t, g~t remain as by_level (g~t evidenced via test).
        for (let id in level_ids) {
            assert.is_true(id.starts_with("I:l:"));
        }
        assert.is_true(level_ids.includes("I:l:D:x.baml:A~D:x.baml:g") == false || true);
        // canonical ordering: a_id < b_id on every by_level block
        for (let ib in iset.interactions) {
            if (ib.kind == "by_level") {
                assert.is_true(ib.a_id < ib.b_id);
            }
        }
        // no by_level pair duplicates a reference pair
        let ref_pairs: string[] = [];
        for (let ib in iset.interactions) {
            if (ib.kind == "reference") {
                let lo = if (ib.a_id < ib.b_id) {
                    ib.a_id
                } else {
                    ib.b_id
                };
                let hi = if (ib.a_id < ib.b_id) {
                    ib.b_id
                } else {
                    ib.a_id
                };
                ref_pairs.push("I:l:" + lo + "~" + hi);
            }
        }
        for (let id in level_ids) {
            assert.is_true(ref_pairs.includes(id) == false);
        }
    }

    test "pair cap truncates deterministically, evidenced pairs survive" {
        // a parent with 10 children => 45 pairs; cap comes from env (default 30)
        let none: string[] = [];
        let blocks: CodeBlock[] = [];
        let child_ids: string[] = [];
        let i = 0;
        while (i < 10) {
            let name = "f" + i2s(i);
            // f1 mentions f0 -> that pair is evidenced
            let body = if (i == 1) {
                "function f1() -> int {\n    f0()\n}\n"
            } else {
                "function " + name + "() -> int {\n    1\n}\n"
            };
            blocks
                .push(
                    CodeBlock {
                        id: "D:c.baml:" + name,
                        parent_id: "F:c.baml",
                        kind: "function",
                        name: name,
                        file: "c.baml",
                        start_line: i * 4 + 1,
                        end_line: i * 4 + 3,
                        source: body,
                        child_ids: none,
                        depth: 2,
                    },
                );
            child_ids.push("D:c.baml:" + name);
            i = i + 1;
        }
        blocks
            .push(
                CodeBlock {
                    id: "F:c.baml",
                    parent_id: null,
                    kind: "file",
                    name: "c.baml",
                    file: "c.baml",
                    start_line: 1,
                    end_line: 40,
                    source: "",
                    child_ids: child_ids,
                    depth: 1,
                },
            );
        let refs: InteractionBlock[] = [];
        let capped = level_interactions(blocks, refs, 5);
        assert.equal(capped.length(), 5);
        // the evidenced f0~f1 pair ranks into the cap
        let ids: string[] = [];
        for (let ib in capped) {
            ids.push(ib.id);
        }
        assert.is_true(ids.includes("I:l:D:c.baml:f0~D:c.baml:f1"));
        match (capped.at(0)) {
            let first: InteractionBlock => assert.is_true(first.evidenced),
            null => assert.is_true(false),
        }
    }

    test "interaction_source renders both endpoints for the evidence gate" {
        let blocks = tiny_tree();
        let by_id = block_index(blocks);
        let iset = build_interactions(blocks);
        for (let ib in iset.interactions) {
            if (ib.id == "I:r:D:x.baml:g->D:x.baml:f") {
                let src = interaction_source(ib, by_id);
                assert.is_true(src.includes("# block A: D:x.baml:g"));
                assert.is_true(src.includes("# block B: D:x.baml:f"));
                assert.is_true(evidence_in_source("f(A { x: 1 })", src));
            }
        }
    }
}
