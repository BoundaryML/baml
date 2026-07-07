// Unit tests for the verify/reverify stage: verdict decoding (with and without
// the reverify-only `stale` flag) and the pure verdict -> outcome mapping.

testset "verify_verdict" {
    test "verdict without stale decodes (older agents)" {
        let v = baml.json.from_string<Verdict>(
            #"{"still_broken":true,"confidence":"high","evidence":"crash reproduced","fixed_behavior":null}"#,
        ) catch (_) {
            _ => Verdict { still_broken: false, confidence: "?", evidence: "", fixed_behavior: null, stale: null }
        };
        assert.is_true(v.still_broken);
        assert.equal(v.confidence, "high");
        assert.equal(v.stale, null);
    }

    test "verdict with stale decodes" {
        let v = baml.json.from_string<Verdict>(
            #"{"still_broken":true,"confidence":"low","evidence":"syntax no longer exists","fixed_behavior":null,"stale":true}"#,
        ) catch (_) {
            _ => Verdict { still_broken: false, confidence: "?", evidence: "", fixed_behavior: null, stale: null }
        };
        assert.equal(v.stale, true);
    }
}

testset "verify_outcome" {
    test "fixed: not broken + high confidence closes in both modes" {
        let v = Verdict { still_broken: false, confidence: "high", evidence: "", fixed_behavior: "works", stale: null };
        assert.equal(verify_outcome(v, false), "fixed");
        assert.equal(verify_outcome(v, true), "fixed");
    }

    test "not broken but low confidence stays broken" {
        let v = Verdict { still_broken: false, confidence: "low", evidence: "", fixed_behavior: null, stale: null };
        assert.equal(verify_outcome(v, false), "broken");
        assert.equal(verify_outcome(v, true), "broken");
    }

    test "stale closes ONLY on the reverify pass" {
        let v = Verdict { still_broken: true, confidence: "low", evidence: "unreproducible", fixed_behavior: null, stale: true };
        assert.equal(verify_outcome(v, false), "broken");
        assert.equal(verify_outcome(v, true), "stale");
    }

    test "still broken stays broken even with stale unset" {
        let v = Verdict { still_broken: true, confidence: "high", evidence: "", fixed_behavior: null, stale: false };
        assert.equal(verify_outcome(v, false), "broken");
        assert.equal(verify_outcome(v, true), "broken");
    }
}
