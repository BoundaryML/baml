test "lint validates all fixture scopes without running conditions" {
    let report = root.lint_sources(fixture_owners(), fixture_sources());
    assert.is_true(report.valid);
    assert.equal(report.rule_count, 4);
    assert.equal(report.source_count, 3);
    assert.equal(
        report.conditions.map((c) -> {
            c.scope
        }),
        ["file", "files", "pr"],
    );
}

test "lint validates references even when paths match no changed files" {
    let report = lint("/never/ @owner # codeturtle: when=missing\n", "");
    assert.equal(report.valid, false);
    assert.equal(report.errors[0].rule_line, 1);
    assert.contains(report.errors[0].message, "Unknown condition: root.missing");
}

test "lint checks contracts without invoking a throwing function" {
    let source = `function explodes(change: codeturtle.FileChange) -> bool throws string { throw "Do not run during lint" }`;
    let owners = "* @baseline\n* @owner # codeturtle: when=explodes\n";
    assert.is_true(lint(owners, source).valid);
    require_error(check(owners = owners, extra = source), "Do not run during lint");
}

test "lint rejects unsupported inputs, outputs and extra required parameters" {
    for (
        let source in [
            `function wrong(input: string) -> codeturtle.Match[] { [] }`,
            `function wrong(input: codeturtle.FileChange) -> int { 1 }`,
            `function wrong(input: codeturtle.FileChange, extra: string) -> bool { true }`,
        ]
    ) {
        let report = lint("* @owner # codeturtle: when=wrong\n", source);
        assert.equal(report.valid, false);
        assert.equal(report.errors.length(), 1);
        assert.equal(report.errors[0].rule_line, 1);
    }
}

test "optional parameters keep their BAML defaults" {
    let source = `function predicate(change: codeturtle.FileChange, enabled: bool = true) -> bool { enabled }`;
    let owners = "* @owner # codeturtle: when=predicate\n";
    assert.is_true(lint(owners, source).valid);
    let report = check(owners = owners, extra = source);
    assert.is_true(report.evaluation_complete);
    assert.equal(report.requirements.length(), 4);
}

test "lint catches package compilation failure" {
    let report = lint("* @owner\n", "class {");
    assert.equal(report.valid, false);
    assert.contains(report.errors[0].message, "CompilationError");
}

test "plain ownership needs no BAML sources" {
    let report = root.lint_sources("* @owner\n", {});
    assert.is_true(report.valid);
    assert.equal(report.source_count, 0);
    assert.equal(report.conditions, []);
}

test "empty or invalid ownership is rejected" {
    for (
        let owners in [
            "# Nothing here\n",
            "* owner-without-at-sign\n",
            "* @owner # codeturtle: unknown\n",
            "/src/**/*.rs @owner\n",
        ]
    ) {
        let report = root.lint_sources(owners, {});
        assert.equal(report.valid, false);
        assert.is_true(report.errors.length() > 0);
    }
}

test "path matching distinguishes exact files and directory prefixes" {
    assert.is_true(root.matches_path("*", "any/file.rs"));
    assert.is_true(root.matches_path("/src/", "src/file.rs"));
    assert.equal(root.matches_path("/src/", "src-other/file.rs"), false);
    assert.is_true(root.matches_path("/src/file.rs", "src/file.rs"));
    assert.equal(root.matches_path("/src/file.rs", "src/file.rs/child"), false);
}

test "renames preserve ownership from the old path" {
    let pr = root.PullRequest {
        title: "Rename",
        changes: [
            root.FileChange {
                path: "new.rs",
                old_path: "protected/old.rs",
                status: "R100",
                added_code: "",
            },
        ],
    };
    let report = root.check_sources(pr, "/protected/ @owner\n", {});
    assert.is_true(report.evaluation_complete);
    assert.equal(report.requirements[0].path, "new.rs");
}
