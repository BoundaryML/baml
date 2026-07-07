// Two-way Linear⇄Convex sync invariants: the extract/render round-trip (a human
// body edit must map back to exactly the description region), the body-hash echo
// guard, dead-card detection, and the reverify webhook gates.

testset "twoway_sync" {
    test "extract_description_region inverts issue_body_md" {
        let desc = "The compiler crashes on `map<int,int>` keys.\n\n## Details\n\n- only string keys work\n- error is a runtime panic";
        let body = issue_body_md(
            desc,
            ["https://ui/runs/t1#turn-2|baml 0.13.1"],
            "baml `0.13.1` · skill `repo@abc`",
            "**What to change:** stringify keys",
            "bug",
            "$ baml run\n--- file ---",
            "https://ui/issues/i1",
            "",
        );
        assert.equal(extract_description_region(body), desc);
    }

    test "extraction survives a body with no machine sections" {
        let body = issue_body_md("just a description", [], "", "", "", "", "", "");
        assert.equal(extract_description_region(body), "just a description");
    }

    test "body_hash is stable and edit-sensitive" {
        let a = body_hash("hello world");
        let b = body_hash("hello world");
        let c = body_hash("hello world!");
        assert.equal(a, b);
        assert.is_true(a != c);
        assert.is_true(a.length() > 0);
    }

    test "is_missing_entity matches Linear's dead-card errors only" {
        assert.is_true(is_missing_entity("linear graphql errors: Entity not found: Issue"));
        assert.is_true(is_missing_entity("Could not find referenced Issue"));
        assert.is_true(is_missing_entity("rate limited") == false);
        assert.is_true(is_missing_entity("linear http 500") == false);
    }

    test "human approve/redraft wins over queued or running reverify" {
        assert.is_true(approve_from("reverify"));
        assert.is_true(approve_from("reverifying"));
        assert.is_true(redraft_from("reverify"));
        assert.is_true(redraft_from("reverifying"));
        // Terminal/mid-fix states still refuse an approve.
        assert.is_true(approve_from("tocursor") == false);
        assert.is_true(approve_from("closed") == false);
    }
}
