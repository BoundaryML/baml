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
