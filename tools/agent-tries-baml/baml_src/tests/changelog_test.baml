// Changelog worker: the revise-request fold (revise_context) that carries a
// bammy "changelog_edit" instruction into the DraftEntry prompt.

testset "changelog_revise" {
    test "revise appends the previous entry and the guidance" {
        let out = revise_context("CTX", "revise", "fix the title", "Old title", "Old body");
        assert.is_true(out.starts_with("CTX"));
        assert.is_true(out.includes("# previous entry"));
        assert.is_true(out.includes("title: Old title"));
        assert.is_true(out.includes("Old body"));
        assert.is_true(out.includes("# REVISE"));
        assert.is_true(out.ends_with("fix the title"));
    }

    test "revise without a stored entry still carries the guidance" {
        let out = revise_context("CTX", "revise", "mention the --no-cache flag", "", "");
        assert.is_true(out.includes("(previous entry unavailable)"));
        assert.is_true(out.ends_with("mention the --no-cache flag"));
    }

    test "regenerate ignores stale guidance" {
        assert.equal(revise_context("CTX", "regenerate", "leftover guidance", "t", "b"), "CTX");
    }

    test "revise with empty guidance is a no-op" {
        assert.equal(revise_context("CTX", "revise", "", "t", "b"), "CTX");
    }

    test "fresh row (no revise fields) is a no-op" {
        assert.equal(revise_context("CTX", "", "", "", ""), "CTX");
    }
}
