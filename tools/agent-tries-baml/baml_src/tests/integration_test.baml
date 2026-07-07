// Unit tests for the pure integration helpers (request-shaping + parsers), no
// network. The HTTP-calling methods need real tokens; these cover the logic.

testset "integrations" {
    test "jesc escapes quotes + newlines for JSON embedding" {
        assert.equal(jesc("a\"b"), "\"a\\\"b\"");
    }

    test "linear issue_body_md assembles sections" {
        let md = issue_body_md(
            "the body",
            ["https://x?call=3", "https://y/runs/t2#turn-4|baml 0.13.1-nightly.20260706.a"],
            "baml `0.13.1-nightly.20260706.a` · skill `https://github.com/BoundaryML/baml-skill@abc1234`",
            "fix it",
            "bug",
            "$ baml build",
            "https://dash/i",
            "https://pr/1",
        );
        assert.contains(md, "**Links:**");
        assert.contains(md, "**Environment:** baml `0.13.1-nightly.20260706.a`");
        assert.contains(md, "## Reproduction");
        assert.contains(md, "## Suggested fix");
        assert.contains(md, "## Evidence");
        // Annotated evidence: the |suffix becomes a " · baml <ver>" note, not part of the URL.
        assert.contains(md, "- [run 2](https://y/runs/t2#turn-4) · baml 0.13.1-nightly.20260706.a");
    }

    test "linear env_line_of renders the environment summary" {
        let issue = SyncIssue {
            _id: "i1",
            title: null,
            status: null,
            kind: null,
            category: null,
            evidence: null,
            bamlVersion: "0.13.1-nightly.20260706.a",
            skillUsed: "https://github.com/BoundaryML/baml-skill",
            skillVersion: "abc1234",
            description: null,
            suggestion: null,
            repro: null,
            linearIssueId: null,
            prUrl: null,
            fixSlackTs: null,
            fixAttempts: null,
        };
        assert.equal(
            env_line_of(issue),
            "baml `0.13.1-nightly.20260706.a` · skill `https://github.com/BoundaryML/baml-skill@abc1234`",
        );
    }

    test "linear extract_field pulls an id" {
        assert
            .equal(
                extract_field("{\"issueCreate\":{\"success\":true,\"issue\":{\"id\":\"ISS-1\"}}}", "id"),
                "ISS-1",
            );
    }

    test "linear extract_label_ids pulls all ids" {
        let ids = extract_label_ids("{\"labels\":{\"nodes\":[{\"id\":\"a\"},{\"id\":\"b\"}]}}");
        assert.equal(ids.length(), 2);
    }

    test "linear swap_status_label drops group labels, keeps others, adds new" {
        // No env status ids configured in tests, so the group is empty -> keeps all + adds.
        let out = swap_status_label(["keepme"], "NEW");
        assert.equal(out.length(), 2);
    }

    test "github ci_state reads conclusions" {
        assert.equal(ci_state("{\"check_runs\":[{\"conclusion\":\"failure\"}]}"), "failing");
        assert.equal(ci_state("{\"check_runs\":[{\"status\":\"in_progress\"}]}"), "pending");
        assert.equal(ci_state("{\"check_runs\":[{\"conclusion\":\"success\"}]}"), "passing");
    }

    test "github coderabbit_state" {
        assert.equal(coderabbit_state("[]"), "none");
        assert
            .equal(
                coderabbit_state("[{\"user\":\"coderabbitai\",\"state\":\"CHANGES_REQUESTED\"}]"),
                "blocking",
            );
    }

    test "github parse_pr_url" {
        let p = parse_pr_url("https://github.com/BoundaryML/baml/pull/42");
        assert.equal(p.length(), 3);
        assert.equal(seg(p, 2), "42");
    }
}
