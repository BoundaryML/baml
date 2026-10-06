test "file, nested multi-file and additive ownership" {
    let report = check();
    assert.is_true(report.evaluation_complete);
    assert.equal(report.errors, []);
    assert.equal(report.requirements.length(), 7);
    let env = requirements(report, "@hellovai");
    assert.equal(
        env.map((r) -> {
            r.path
        }),
        ["src/timeout.rs", "src/config.rs"],
    );
    assert.equal(env[0].approval, root.Approval.All);
    assert.equal(env[1].evaluation.result.verdict, root.Verdict.Uncertain);
    let api = requirements(report, "@sdk-team");
    assert.equal(api.length(), 1);
    assert.equal(api[0].path, "src/api/client.rs");
    assert.equal(
        api[0].evaluation.condition,
        "checks.api.compatibility.undocumented_public_api_change",
    );
}

test "documentation removes only the conditional API requirement" {
    let pr = fixture_pr();
    pr.changes.push(root.FileChange { path: "docs/api.md", added_code: "Document with_timeout" });
    let report = check(pr = pr);
    assert.is_true(report.evaluation_complete);
    assert.equal(requirements(report, "@sdk-team"), []);
    assert.equal(requirements(report, "@repo-maintainers").length(), 5);
}

test "PR context sees the title and all changes" {
    let pr = fixture_pr();
    pr.title = "Docs: update timeout instructions";
    let report = check(pr = pr);
    assert.is_true(report.evaluation_complete);
    assert.equal(requirements(report, "@release-team").length(), 4);
}

test "missing function blocks completion and retains baseline" {
    let report = check(owners = "* @baseline\n* @other # codeturtle: when=missing\n");
    require_error(report, "Unknown condition: root.missing");
    assert.equal(report.errors[0].rule_line, 2);
    assert.equal(requirements(report, "@baseline").length(), 4);
}

test "unsupported input type is a blocking error" {
    let report = check(
        owners = "* @owner # codeturtle: when=wrong\n",
        extra = `
            function wrong(input: string) -> codeturtle.Match[] { [] }
        `,
    );
    require_error(report, "Unsupported condition input type");
}

test "unsupported return type is a blocking error" {
    let report = check(
        owners = "* @owner # codeturtle: when=wrong\n",
        extra = `
            function wrong(input: codeturtle.FileChange) -> int { 1 }
        `,
    );
    require_error(report, "CompilationError");
}

test "runtime exception is not a negative match" {
    let report = check(
        owners = "* @baseline\n* @owner # codeturtle: when=broken\n",
        extra = `
            function broken(input: codeturtle.FileChange) -> bool throws string { throw "broken" }
        `,
    );
    require_error(report, "broken");
    assert.equal(requirements(report, "@baseline").length(), 4);
    assert.equal(requirements(report, "@owner"), []);
}

test "out-of-scope result invalidates the whole rule" {
    let report = check(
        owners = "/src/ @owner # codeturtle: when=wrong\n",
        extra = `
            function wrong(input: codeturtle.FileChange[]) -> codeturtle.Match[] {
                [codeturtle.Match { path: "unrelated.rs", verdict: codeturtle.Verdict.Applies, reason: "bad", evidence: [] }]
            }
        `,
    );
    require_error(report, "outside the ownership rule");
    assert.equal(report.requirements, []);
}

test "single-file result cannot point at a different matching file" {
    let report = check(
        owners = "* @owner # codeturtle: when=wrong\n",
        extra = `
            function wrong(input: codeturtle.FileChange) -> codeturtle.Match {
                codeturtle.Match { path: "src/greeting.rs", verdict: codeturtle.Verdict.Applies, reason: "bad", evidence: [] }
            }
        `,
    );
    require_error(report, "different file");
    assert.equal(report.requirements, []);
}

test "unowned files block completion without an evaluation error" {
    let report = check(owners = "/src/timeout.rs @owner\n");
    assert.equal(report.evaluation_complete, false);
    assert.equal(report.errors, []);
    assert.equal(report.unowned_files, ["src/greeting.rs", "src/config.rs", "src/api/client.rs"]);
}

test "compilation failure returns a typed error report" {
    let report = check(extra = "class {");
    require_error(report, "CompilationError");
    assert.equal(report.requirements, []);
}

test "quoted natural-language conditions are explicitly unsupported" {
    require_error(
        check(owners = "* @owner # codeturtle: when=\"adds an env variable\"\n"),
        "function conditions only",
    );
}

test "boolean file result preserves negative trace and report shape" {
    let report = check(
        owners = "* @baseline\n* @reviewer # codeturtle: all, when=predicate\n",
        extra = `
            function predicate(change: codeturtle.FileChange) -> bool {
                change.added_code.includes("std::env::var(")
            }
        `,
    );
    assert.is_true(report.evaluation_complete);
    let positive = requirements(report, "@reviewer");
    assert.equal(positive.length(), 1);
    assert.equal(positive[0].path, "src/timeout.rs");
    assert.equal(positive[0].approval, root.Approval.All);
    assert.equal(positive[0].evaluation.result.evidence, []);
    assert.equal(positive[0].evaluation.result.reason, "Condition predicate returned true");
    let results = condition_results(report, "predicate");
    assert.equal(
        results.map((r) -> {
            r.verdict
        }),
        [
            root.Verdict.Applies,
            root.Verdict.DoesNotApply,
            root.Verdict.DoesNotApply,
            root.Verdict.DoesNotApply,
        ],
    );
}

test "Match-or-bool union keeps detailed reasons and evidence" {
    let report = check(
        owners = "* @baseline\n* @reviewer # codeturtle: when=predicate\n",
        extra = `
            function predicate(change: codeturtle.FileChange) -> codeturtle.Match | bool {
                if (change.path == "src/timeout.rs") {
                    codeturtle.Match { path: change.path, verdict: codeturtle.Verdict.Uncertain,
                        reason: "Inspect timeout behavior", evidence: [change.added_code] }
                } else { false }
            }
        `,
    );
    let result = requirements(report, "@reviewer")[0].evaluation.result;
    assert.equal(result.verdict, root.Verdict.Uncertain);
    assert.equal(result.reason, "Inspect timeout behavior");
    assert.is_true(result.evidence.length() > 0);
}

test "multi-file boolean applies to all rule-matching files" {
    let report = check(
        owners = "* @baseline\n/src/ @reviewer # codeturtle: when=predicate\n",
        extra = `
            function predicate(changes: codeturtle.FileChange[]) -> bool { changes.length() > 1 }
        `,
    );
    assert.equal(requirements(report, "@reviewer").length(), 4);
}

test "PR boolean sees all files but cannot expand the rule's scope" {
    let report = check(
        owners = "* @baseline\n/src/timeout.rs @reviewer # codeturtle: when=predicate\n",
        extra = `
            function predicate(pr: codeturtle.PullRequest) -> bool { pr.changes.length() == 4 }
        `,
    );
    assert.equal(
        requirements(report, "@reviewer").map((r) -> {
            r.path
        }),
        ["src/timeout.rs"],
    );
}

test "multi-file array-or-bool union can select files or return false" {
    let owners = "* @baseline\n* @reviewer # codeturtle: when=predicate\n";
    let source = `
        function predicate(changes: codeturtle.FileChange[]) -> codeturtle.Match[] | bool {
            for (let change in changes) { if (change.path == "docs/api.md") { return false; } }
            [codeturtle.Match { path: "src/api/client.rs", verdict: codeturtle.Verdict.Applies,
                reason: "API without documentation", evidence: [] }]
        }
    `;
    assert.equal(
        requirements(check(owners = owners, extra = source), "@reviewer").map((r) -> {
            r.path
        }),
        ["src/api/client.rs"],
    );
    let pr = fixture_pr();
    pr.changes.push(root.FileChange { path: "docs/api.md", added_code: "Updated docs" });
    let after = check(owners = owners, extra = source, pr = pr);
    assert.equal(requirements(after, "@reviewer"), []);
    assert.equal(condition_results(after, "predicate").length(), 5);
}

test "PR array-or-bool union can select a file or return false" {
    let owners = "* @baseline\n* @reviewer # codeturtle: when=predicate\n";
    let source = `
        function predicate(pr: codeturtle.PullRequest) -> codeturtle.Match[] | bool {
            if (pr.title == "No API changes") { false } else {
                [codeturtle.Match { path: "src/api/client.rs", verdict: codeturtle.Verdict.Applies,
                    reason: "Inspect the API", evidence: [] }]
            }
        }
    `;
    assert.equal(requirements(check(owners = owners, extra = source), "@reviewer").length(), 1);
    let pr = fixture_pr();
    pr.title = "No API changes";
    assert.equal(requirements(check(owners = owners, extra = source, pr = pr), "@reviewer"), []);
}

test "false never waives ownership coverage" {
    let report = check(
        owners = "* @reviewer # codeturtle: when=predicate\n",
        extra = `
            function predicate(change: codeturtle.FileChange) -> bool { false }
        `,
    );
    assert.equal(report.requirements, []);
    assert.equal(report.errors, []);
    assert.equal(report.unowned_files.length(), 4);
    assert.equal(report.evaluation_complete, false);
}

test "non-positive concurrency returns a blocking error" {
    require_error(check(concurrency = 0), "positive integer");
}
