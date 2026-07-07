// $parse parity for the Dedup LLM function — runs the REAL output parser on a
// captured-style reply, no API key/network. This is the LLM-fidelity regression
// gate for the dedup stage.

testset "dedup" {
    test "parses a new issue + an extended issue" {
        let r = Dedup$parse(
            #"
                {"issues":[
                  {"kind":"language","category":"bug","title":"crash on union>4","description":"d","suggestion":"s","repro":"$ baml build","evidence":[{"report_id":"r1","call_index":3}]},
                  {"id":"EXIST-1","kind":"skill","category":"suggestion","title":"doc map gotcha","description":"d2","suggestion":"s2","evidence":[{"report_id":"r2","call_index":null}]}
                ]}
            "#,
        );
        assert.equal(r.issues.length(), 2);
    }

    test "new issue omits id; repro + call_index present" {
        let r = Dedup$parse(
            #"{"issues":[{"kind":"language","category":"bug","title":"t","description":"d","suggestion":"s","repro":"R","evidence":[{"report_id":"r1","call_index":7}]}]}"#,
        );
        for (let it in r.issues) {
            assert.equal(it.kind, "language");
            assert.equal(it.id, null);
            assert.equal(it.repro, "R");
            for (let e in it.evidence) {
                assert.equal(e.call_index, 7);
            }
        }
    }

    test "extended issue carries id; null call_index ok" {
        let r = Dedup$parse(
            #"{"issues":[{"id":"E1","kind":"skill","category":"suggestion","title":"t","description":"d","suggestion":"s","evidence":[{"report_id":"r2","call_index":null}]}]}"#,
        );
        for (let it in r.issues) {
            assert.equal(it.id, "E1");
            assert.equal(it.repro, null);
            for (let e in it.evidence) {
                assert.equal(e.call_index, null);
            }
        }
    }

    // NB: prompt-text parity via render_prompt is deferred — baml.unstable.string
    // of a PromptAst gives a structural repr, not the rendered text. The $parse
    // tests above are the LLM-fidelity gate that matters.
}

// The cross-run-dedup open-issue scope: which statuses are matched against (so a
// re-found finding extends an existing issue) vs. excluded as terminal.
testset "dedup_scope" {
    test "open statuses include every in-flight state" {
        let s = dedup_open_statuses();
        assert.is_true(in_list(s, "open"));
        assert.is_true(in_list(s, "confirmed"));
        assert.is_true(in_list(s, "approved"));
        assert.is_true(in_list(s, "tocursor"));
        assert.is_true(in_list(s, "prprep"));
        assert.is_true(in_list(s, "pr_ready"));
        assert.is_true(in_list(s, "verifying"));
        assert.is_true(in_list(s, "reverify"));
        assert.is_true(in_list(s, "reverifying"));
        assert.is_true(in_list(s, "needs_human"));
    }

    test "terminal statuses are excluded from dedup matching" {
        let s = dedup_open_statuses();
        assert.is_true(in_list(s, "closed") == false);
        assert.is_true(in_list(s, "rejected") == false);
    }
}

// The richer match context handed to the Dedup LLM: description snippets and
// the open/closed state tag.
testset "dedup_match_context" {
    test "snippet flattens newlines and truncates" {
        let s = snippet_of("first line\nsecond line\nthird", 100);
        assert.equal(s, "first line second line third");
        let t = snippet_of("abcdefghij", 4);
        assert.equal(t, "abcd");
    }

    test "parse_match_outs tags state and reads snippet from description" {
        let outs = parse_match_outs(
            #"[{"_id":"i1","kind":"language","category":"bug","title":"T1","description":"line one\nline two"},{"_id":"","kind":"skill","title":"dropped"}]"#,
            "closed",
        );
        assert.equal(outs.length(), 1);
        for (let o in outs) {
            assert.equal(o.id, "i1");
            assert.equal(o.state, "closed");
            assert.equal(o.snippet, "line one line two");
            assert.equal(o.category, "bug");
        }
    }
}
