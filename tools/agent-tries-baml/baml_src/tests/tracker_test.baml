// Unit tests for the cursor-tracker + FixDispatch pure logic. These pin the four
// bugs found during the live spawn test: Convex-float coercion (f2i), the bc-<uuid>
// launch-response id detection, PR-URL parsing, and CI/CodeRabbit state derivation.

testset "tracker" {
    // ---- f2i: Convex serializes every number as a float ----
    test "f2i truncates floats to ints" {
        assert.equal(f2i(0.0), 0);
        assert.equal(f2i(3.0), 3);
    }

    test "f2i truncates a fractional float" {
        assert.equal(f2i(2.9), 2);
    }

    // ---- PR URL parsing ----
    test "parse_pr_loc pulls owner/repo/number" {
        let loc = match (parse_pr_loc("https://github.com/BoundaryML/baml-skill/pull/31")) {
            let l: PrLoc => l,
            null => PrLoc { owner: "", repo: "", number: 0 },
        };
        assert.equal(loc.owner, "BoundaryML");
        assert.equal(loc.repo, "baml-skill");
        assert.equal(loc.number, 31);
    }

    test "parse_pr_loc rejects a non-PR url" {
        assert.is_true(parse_pr_loc("https://example.com/foo") == null);
    }

    // ---- launch-response id detection (the bc-<uuid> fix) ----
    test "a real launch response carries the agent id" {
        let resp = #"{"id":"bc-a48c1972-790b-442d-b44f-0849767c200c","name":"x","status":"ACTIVE"}"#;
        assert.equal(extract_field(resp, "id"), "bc-a48c1972-790b-442d-b44f-0849767c200c");
    }

    test "a 409 conflict body has no id but is recognizable" {
        let resp = #"{"error":{"code":"agent_id_conflict","message":"already launched"}}"#;
        assert.equal(extract_field(resp, "id"), "");
        assert.is_true(resp.includes("agent_id_conflict"));
    }

    test "an error body yields no id (real failure)" {
        assert.equal(extract_field(#"{"code":"error","message":"Invalid User API Key"}"#, "id"), "");
    }

    // ---- typed PR state parsing (GhPr) ----
    test "github PR json parses into GhPr" {
        let pr = baml
            .json
            .from_string<GhPr>(
                #"{"merged":false,"state":"open","mergeable_state":"dirty","head":{"sha":"deadbeef"},"base":{"ref":"main"}}"#,
            );
        assert.equal(pr.state, "open");
        assert.equal(pr.mergeable_state, "dirty");
        assert
            .equal(
                match (pr.head) {
                    let h: GhRef => match (h.sha) {
                        let s: string => s,
                        null => "",
                    },
                    null => "",
                },
                "deadbeef",
            );
        assert
            .equal(
                match (pr.base) {
                    let b: GhRef => match (b.ref) {
                        let r: string => r,
                        null => "",
                    },
                    null => "",
                },
                "main",
            );
    }

    // ---- cursor run -> branch/PR parsing (CursorRunState) ----
    test "cursor run json surfaces the branch + prUrl" {
        let run = baml
            .json
            .from_string<CursorRunState>(
                #"{"status":"FINISHED","git":{"branches":[{"repoUrl":"github.com/o/r","branch":"cursor/x","prUrl":"https://github.com/o/r/pull/7"}]}}"#,
            );
        assert
            .is_true(
                cursor_run_terminal(
                    match (run.status) {
                        let s: string => s,
                        null => "",
                    },
                ),
            );
        let b0 = match (run.git) {
            let g: CursorGit => match (g.branches) {
                let bs: CursorBranch[] => match (bs.at(0)) {
                    let b: CursorBranch => b,
                    null => CursorBranch { repoUrl: null, branch: null, prUrl: null },
                },
                null => CursorBranch { repoUrl: null, branch: null, prUrl: null },
            },
            null => CursorBranch { repoUrl: null, branch: null, prUrl: null },
        };
        assert
            .equal(
                match (b0.prUrl) {
                    let u: string => u,
                    null => "",
                },
                "https://github.com/o/r/pull/7",
            );
    }

    // ---- CI + CodeRabbit derivation over captured check/review JSON ----
    test "ci_state reads a failing conclusion" {
        assert
            .equal(
                ci_state(#"{"check_runs":[{"status":"completed","conclusion":"failure"}]}"#),
                "failing",
            );
    }

    test "ci_state reads an in-progress run as pending" {
        assert.equal(ci_state(#"{"check_runs":[{"status":"in_progress"}]}"#), "pending");
    }

    test "ci_state all-success is passing" {
        assert
            .equal(
                ci_state(#"{"check_runs":[{"status":"completed","conclusion":"success"}]}"#),
                "passing",
            );
    }

    test "coderabbit_state: absent / blocking / clear" {
        assert.equal(coderabbit_state(#"[]"#), "none");
        assert
            .equal(
                coderabbit_state(#"[{"user":{"login":"coderabbitai"},"state":"CHANGES_REQUESTED"}]"#),
                "blocking",
            );
        assert
            .equal(
                coderabbit_state(#"[{"user":{"login":"coderabbitai"},"state":"COMMENTED"}]"#),
                "clear",
            );
    }

    // ---- agent-id resolution (pending markers aren't dispatched yet) ----
    test "tracker_agent_id ignores a pending marker" {
        let issue = TrackIssue {
            _id: "i1",
            title: "t",
            status: "tocursor",
            cursorAgentId: "pending:bc-1",
            fixSlackTs: null,
            prUrl: null,
            prBranch: null,
            fixAttempts: null,
            lastFixedSha: null,
            linearIssueId: null,
            fixThreadTs: null,
        };
        assert.equal(tracker_agent_id(issue), "");
    }

    test "tracker_agent_id returns a real ref" {
        let issue = TrackIssue {
            _id: "i1",
            title: "t",
            status: "tocursor",
            cursorAgentId: "bc-2",
            fixSlackTs: null,
            prUrl: null,
            prBranch: null,
            fixAttempts: null,
            lastFixedSha: null,
            linearIssueId: null,
            fixThreadTs: null,
        };
        assert.equal(tracker_agent_id(issue), "bc-2");
    }

    // ---- the refix / conflict prompts carry their core instruction ----
    test "refix prompt names the PR and forbids a new PR" {
        let p = tracker_refix_prompt("My Issue", "https://github.com/o/r/pull/9", "cursor/x", "failing CI checks");
        assert.is_true(p.includes("https://github.com/o/r/pull/9"));
        assert.is_true(p.includes("do not open a new PR"));
    }

    test "conflict prompt names the base branch" {
        let p = tracker_conflict_prompt("My Issue", "https://github.com/o/r/pull/9", "cursor/x", "canary");
        assert.is_true(p.includes("canary"));
    }

    // ---- GitHub branch PR-lookup fallback ----
    test "parse_repo_url pulls owner/repo (strips .git)" {
        let p = parse_repo_url("github.com/BoundaryML/baml-skill.git");
        assert.equal(p.length(), 2);
        assert
            .equal(
                match (p.at(0)) {
                    let v: string => v,
                    null => "",
                },
                "BoundaryML",
            );
        assert
            .equal(
                match (p.at(1)) {
                    let v: string => v,
                    null => "",
                },
                "baml-skill",
            );
    }

    test "parse_repo_url rejects a non-repo url" {
        assert.equal(parse_repo_url("https://example.com/x").length(), 0);
    }

    test "open-pr lookup parses the first PR html_url" {
        let prs = baml
            .json
            .from_string<GhPrRef[]>(#"[{"html_url":"https://github.com/o/r/pull/12","number":12}]"#);
        assert
            .equal(
                match (prs.at(0)) {
                    let p: GhPrRef => match (p.html_url) {
                        let u: string => u,
                        null => "",
                    },
                    null => "",
                },
                "https://github.com/o/r/pull/12",
            );
    }

    // ---- pr_ready human-review detection (HWM + human-vs-bot filter) ----
    test "new_human_comments skips bots and old comments, keeps fresh humans" {
        let reviews = #"[{"user":{"login":"alice","type":"User"},"body":"please fix the naming","created_at":"2026-06-25T12:00:00Z"},{"user":{"login":"coderabbitai","type":"Bot"},"body":"nit","created_at":"2026-06-25T13:00:00Z"}]"#;
        let convo = #"[{"user":{"login":"bob","type":"User"},"body":"old note","created_at":"2026-06-24T09:00:00Z"}]"#;
        // High-water mark just before alice's comment: only alice (human, newer) survives.
        let fresh = new_human_comments(reviews, convo, "2026-06-25T11:00:00Z");
        assert.equal(fresh.length(), 1);
        assert.equal(newest_created_at(fresh), "2026-06-25T12:00:00Z");
    }

    test "new_human_comments: empty high-water mark takes all humans" {
        let reviews = #"[{"user":{"login":"alice","type":"User"},"body":"a","created_at":"2026-06-25T12:00:00Z"}]"#;
        assert.equal(new_human_comments(reviews, #"[]"#, "").length(), 1);
    }

    test "new_human_comments: nothing newer than the HWM" {
        let reviews = #"[{"user":{"login":"alice","type":"User"},"body":"a","created_at":"2026-06-25T12:00:00Z"}]"#;
        assert.equal(new_human_comments(reviews, #"[]"#, "2026-06-25T12:00:00Z").length(), 0);
    }

    test "human_comment_summary formats @author: body" {
        let reviews = #"[{"user":{"login":"alice","type":"User"},"body":"fix\nthe naming","created_at":"2026-06-25T12:00:00Z"}]"#;
        let fresh = new_human_comments(reviews, #"[]"#, "");
        let s = human_comment_summary(fresh);
        assert.is_true(s.includes("@alice"));
        assert.is_true(s.includes("fix the naming"));
    }
}
