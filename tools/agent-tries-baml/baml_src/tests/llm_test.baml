// $parse parity for the remaining LLM functions — real output parser on
// captured-style replies, no API key/network. (Dedup has its own dedup_test.)

testset "llm" {
    test "Redraft parses revised fields" {
        let r = Redraft$parse(
            #"{"title":"new title","kind":"language","category":"bug","description":"d","suggestion":"s"}"#,
        );
        assert.equal(r.title, "new title");
        assert.equal(r.kind, "language");
    }

    test "Compare parses findings + suggestions" {
        let c = Compare$parse(
            #"{"report_md":"r","summary":"s","what_went_well":["w1","w2"],"what_failed":[],"findings":[{"kind":"skill","title":"t","description":"d","call_index":null,"suggestion":"s"}],"suggestions":[{"target":"skill","suggestion":"s","rationale":"r"}]}"#,
        );
        assert.equal(c.findings.length(), 1);
        assert.equal(c.what_went_well.length(), 2);
        for (let f in c.findings) {
            assert.equal(f.call_index, null);
        }
    }

    test "GenerateTasks parses task list" {
        let t = GenerateTasks$parse(#"{"tasks":["implement gcd","implement dijkstra","implement trie"]}"#);
        assert.equal(t.tasks.length(), 3);
    }

    test "RouteMention parses intent route" {
        let r = RouteMention$parse(
            #"{"intent":"changelog_edit","bench_prompt":"","version_ref":"0.222.0","mode":"revise","guidance":"fix title","promo_notes":"","feedback_text":"","posthog_question":"","needs_clarification":false,"clarification_question":""}"#,
        );
        assert.equal(r.intent, "changelog_edit");
        assert.equal(r.version_ref, "0.222.0");
        assert.is_true(r.needs_clarification == false);
    }

    test "WriteHogql parses query" {
        let h = WriteHogql$parse(
            #"{"hogql":"SELECT count() FROM events WHERE timestamp > now() - INTERVAL 30 DAY LIMIT 100"}"#,
        );
        assert.contains(h.hogql, "SELECT count()");
    }

    test "DraftEntry parses entry" {
        let d = DraftEntry$parse(#"{"version":"0.222.0","date":"2026-06-24","title":"t","body":"b","authors":["x","y"]}"#);
        assert.equal(d.version, "0.222.0");
        assert.equal(d.authors.length(), 2);
    }

    test "CritiqueEntry parses scores + verdict" {
        let c = CritiqueEntry$parse(
            #"{"grounding":"great","completeness":"good","specificity":"good","usefulness":"good","style":"great","runnable":"great","verdict":"approve","issues":[],"rewrite_hints":""}"#,
        );
        assert.equal(c.verdict, "approve");
        assert.equal(c.issues.length(), 0);
    }

    test "Verdict decodes via from_string (agent stage)" {
        let v = baml
            .json
            .from_string<Verdict>(
                #"{"still_broken":false,"confidence":"high","evidence":"reproduced; now fixed","fixed_behavior":"compiles cleanly"}"#,
            );
        assert.is_true(v.still_broken == false);
        assert.equal(v.confidence, "high");
    }
}
