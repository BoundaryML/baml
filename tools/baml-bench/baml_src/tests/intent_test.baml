// Token-free cache-replay tests for the intention wrappers: seed the
// content-addressed cache, run the cached_* path, assert deterministic
// reconciliation — no model is ever called.

function ib_fixture_block(
    id: string,
    kind: string,
    depth: int,
    name: string,
    source: string,
) -> CodeBlock {
    let none: string[] = [];
    CodeBlock {
        id: id,
        parent_id: null,
        kind: kind,
        name: name,
        file: "x.baml",
        start_line: 1,
        end_line: 1,
        source: source,
        child_ids: none,
        depth: depth,
    }
}

testset "intent_cache" {
    test "cached_leaf_intents replays, drops unknown ids, fills missing neutral" {
        let dir = "/tmp/intentionbench-test-cache-leaf";
        let parent = ib_fixture_block(
            "D:x.baml:f",
            "function",
            2,
            "f",
            "function f(a: int) -> int {\n    let b = a + 1;\n    b\n}\n",
        );
        let leaf_a = ib_fixture_block("M:x.baml:f.a", "param", 3, "a", "a: int\n");
        let leaf_b = ib_fixture_block("V:x.baml:f.b#1", "var", 3, "b", "let b = a + 1;\n");
        let leaves = [leaf_a, leaf_b];
        let key = cache_key("intent-leaf@" + fast_model(), parent.id + "\n" + leaf_table(leaves));
        // model answered leaf_a (confidence out of range), hallucinated an id,
        // and skipped leaf_b entirely
        cache_put(
            dir,
            key,
            "{\"intents\": [{\"block_id\": \"M:x.baml:f.a\", \"goal\": \"take the input\", \"mechanism\": \"int param\", \"confidence\": 9}, {\"block_id\": \"HALLUCINATED\", \"goal\": \"x\", \"mechanism\": \"y\", \"confidence\": 3}]}",
        );
        let pairs = cached_leaf_intents(dir, parent, leaves);
        assert.equal(pairs.length(), 2);
        match (pairs.at(0)) {
            let p: IntentPair => {
                assert.equal(p.block_id, "M:x.baml:f.a");
                assert.equal(p.intention.goal, "take the input");
                assert.equal(p.intention.confidence, 5); // clamped
            },
            null => assert.is_true(false),
        }
        match (pairs.at(1)) {
            let p: IntentPair => {
                assert.equal(p.block_id, "V:x.baml:f.b#1");
                assert.equal(p.intention.goal, ""); // filled neutral
                assert.equal(p.intention.confidence, 1);
            },
            null => assert.is_true(false),
        }
        baml.fs.remove(cache_path(dir, key)) catch (_) {
            _ => null
        };
    }

    test "cached_block_intent replays and clamps confidence" {
        let dir = "/tmp/intentionbench-test-cache-block";
        let block = ib_fixture_block("D:x.baml:f", "function", 2, "f", "function f() -> int {\n    1\n}\n");
        let child_table = "V:x.baml:f.b#1 | hold the sum | int let\n";
        let key = cache_key("intent@" + fast_model(), block.id + "\n" + block.source + "\n" + child_table + "\n" + "");
        cache_put(
            dir,
            key,
            "{\"goal\": \"produce one\", \"mechanism\": \"constant return\", \"confidence\": 0}",
        );
        let it = cached_block_intent(dir, block, child_table, "");
        assert.equal(it.goal, "produce one");
        assert.equal(it.confidence, 1); // clamped up
        baml.fs.remove(cache_path(dir, key)) catch (_) {
            _ => null
        };
    }

    test "cached_inter_intents replays with per-pair reconciliation" {
        let dir = "/tmp/intentionbench-test-cache-inter";
        let a = ib_fixture_block("D:x.baml:f", "function", 2, "f", "function f() -> int {\n    g()\n}\n");
        let b = ib_fixture_block("D:x.baml:g", "function", 2, "g", "function g() -> int {\n    1\n}\n");
        let by_id: map<string, CodeBlock> = {};
        by_id.set(a.id, a);
        by_id.set(b.id, b);
        let ib = InteractionBlock {
            id: "I:r:D:x.baml:f->D:x.baml:g",
            kind: "reference",
            a_id: a.id,
            b_id: b.id,
            parent_id: "F:x.baml",
            edge_kind: "calls",
            evidenced: true,
        };
        let pairs = [ib];
        let key = cache_key(
            "intent-inter@" + fast_model(),
            "F:x.baml" + "#" + i2s(0) + "\n" + pair_table(pairs, by_id),
        );
        cache_put(
            dir,
            key,
            "{\"intents\": [{\"interaction_id\": \"I:r:D:x.baml:f->D:x.baml:g\", \"relationship\": \"f delegates to g\", \"purpose\": \"reuse g's computation\", \"confidence\": 4}]}",
        );
        let got = cached_inter_intents(dir, "F:x.baml", 0, pairs, by_id);
        assert.equal(got.length(), 1);
        match (got.at(0)) {
            let p: InterPair => {
                assert.equal(p.intention.relationship, "f delegates to g");
                assert.equal(p.intention.confidence, 4);
            },
            null => assert.is_true(false),
        }
        baml.fs.remove(cache_path(dir, key)) catch (_) {
            _ => null
        };
    }

    test "inter_jobs batches groups of 25 deterministically" {
        let pairs: InteractionBlock[] = [];
        let i = 0;
        while (i < 60) {
            pairs
                .push(
                    InteractionBlock {
                        id: "I:l:a" + pad6(i) + "~b",
                        kind: "by_level",
                        a_id: "a" + pad6(i),
                        b_id: "b",
                        parent_id: "F:x.baml",
                        edge_kind: "sibling",
                        evidenced: false,
                    },
                );
            i = i + 1;
        }
        let jobs = inter_jobs(pairs);
        assert.equal(jobs.length(), 3); // 25 + 25 + 10
        match (jobs.at(2)) {
            let j: InterJob => {
                assert.equal(j.batch_no, 2);
                assert.equal(j.pairs.length(), 10);
            },
            null => assert.is_true(false),
        }
    }
}
