test "usage works without Git or BAML in PATH" {
    for (let args in [[], ["help", "--help"], ["check", "--help"], ["lint", "--help"], ["--help"]]) {
        let result = cli(args, env = { "PATH": "" });
        expect_exit(result, 0);
        assert.contains(result.stdout.to_string(), "usage: codeturtle");
        if (args == ["check", "--help"]) {
            assert.contains(result.stdout.to_string(), "--base");
        }
        if (args == ["lint", "--help"]) {
            assert.contains(result.stdout.to_string(), "--rev");
            assert.equal(result.stdout.to_string().includes("--head"), false);
        }
    }
}

function lint_output_case() -> bool {
    with_fixture((dir) -> {
        let text = cli(["lint", "--repo", dir]);
        expect_exit(text, 0);
        assert.contains(text.stdout.to_string(), "CodeTurtle: policy valid");
        let json = cli(["lint", "--repo", dir, "--mode=json"]);
        expect_exit(json, 0);
        let payload = baml.json.from_string<root.LintReport>(json.stdout.to_string());
        assert.is_true(payload.valid);
        assert.equal(
            payload.conditions.map((c) -> {
                c.scope
            }),
            ["file", "files", "pr"],
        );
        true
    })
}

test "lint defaults to text and also emits typed JSON" {
    assert.is_true(lint_output_case());
}

function global_options_case() -> bool {
    with_fixture((dir) -> {
        let result = cli(["--mode=json", "lint", "--policy-dir", dir + "/.github/codeturtle"]);
        expect_exit(result, 0);
        assert.is_true(baml.json.from_string<root.LintReport>(result.stdout.to_string()).valid);
        true
    })
}

test "global output option works before the subcommand" {
    assert.is_true(global_options_case());
}

function aliases_case() -> bool {
    with_fixture((dir) -> {
        let args = ["--repo", dir, "--changes-path", dir + "/changes.json", "--mode=json"];
        let checked = cli(["check"].concat(args));
        let explained = cli(["explain"].concat(args));
        let legacy = cli(args);
        expect_exit(checked, 0);
        expect_exit(explained, 0);
        expect_exit(legacy, 0);
        assert.equal(checked.stdout, explained.stdout);
        assert.equal(checked.stdout, legacy.stdout);
        true
    })
}

test "check, explain and legacy flags produce identical output" {
    assert.is_true(aliases_case());
}

function help_outside_repo_case() -> bool {
    with_temp((dir) -> {
        let result = cli(["help"], cwd = dir);
        expect_exit(result, 0);
        let text = result.stdout.to_string();
        for (
            let expected in [
                "codeturtle check --base main",
                "ns_checks/contracts.baml",
                "class codeturtle.FileChange",
                "hunks: codeturtle.Hunk[] | null",
                "typesafeai/jev-latest",
            ]
        ) {
            assert.contains(text, expected);
        }
        assert.equal(text.includes("class codeturtle.Job"), false);
        assert.equal(text.includes("class codeturtle.HelpReport"), false);
        true
    })
}

test "complete reflected help works outside a repository" {
    assert.is_true(help_outside_repo_case());
}

test "JSON help keeps guidance and qualified type references" {
    let result = cli(["help", "--mode=json"]);
    expect_exit(result, 0);
    let payload = baml.json.parse(result.stdout.to_string());
    assert.contains(baml.json.path<string>(payload, ".guidance"), "codeturtle check --base main");
    assert.contains(result.stdout.to_string(), "codeturtle.Hunk[] | null");
    assert.equal(result.stdout.to_string().includes("warning:"), false);
}

test "help is a single command without topic subcommands" {
    for (let topic in ["types", "Match", "check"]) {
        let result = cli(["help", topic]);
        expect_exit(result, 2);
        assert.contains(result.stderr.to_string(), "unrecognized arguments");
    }
}

test "missing BAML has an actionable help error" {
    let result = cli(["help"], env = { "PATH": "" });
    expect_exit(result, 2);
    assert.contains(result.stdout.to_string(), "Use codeturtle --help");
}

function output_modes_case() -> bool {
    with_fixture((dir) -> {
        let default = fixture_cli(dir);
        let json = fixture_cli(dir, mode = "json");
        let text = fixture_cli(dir, mode = "text");
        expect_exit(default, 0);
        expect_exit(text, 0);
        let payload = report(json);
        assert.equal(default.stdout, text.stdout);
        let rendered = text.stdout.to_string();
        assert.contains(rendered, "7 review requirements across 4 files");
        assert.contains(rendered, "all of @hellovai, @aaronvg");
        assert.contains(rendered, "Uncertain:");
        for (let requirement in payload.requirements) {
            assert.contains(rendered, requirement.path);
            assert.contains(rendered, requirement.evaluation.result.reason);
        }
        true
    })
}

test "text is the default and agrees with typed JSON" {
    assert.is_true(output_modes_case());
}

function failure_output_case() -> bool {
    with_fixture((dir) -> {
        write(dir, ".github/codeturtle/OWNERS", "* @baseline\n* @other # codeturtle: when=missing\n");
        let payload = report(fixture_cli(dir, mode = "json"), expected = 1);
        let text = fixture_cli(dir, mode = "text");
        expect_exit(text, 1);
        assert.equal(payload.requirements.length(), 4);
        assert.contains(text.stdout.to_string(), "any of @baseline");
        assert.contains(text.stdout.to_string(), "OWNERS:2: Condition evaluation failed: missing");
        assert.contains(text.stdout.to_string(), "Unknown condition: root.missing");
        true
    })
}

test "errors retain successful requirements in both formats" {
    assert.is_true(failure_output_case());
}

function unowned_output_case() -> bool {
    with_fixture((dir) -> {
        write(dir, ".github/codeturtle/OWNERS", "/src/timeout.rs @owner\n");
        let payload = report(fixture_cli(dir, mode = "json"), expected = 1);
        let text = fixture_cli(dir, mode = "text");
        expect_exit(text, 1);
        assert.contains(text.stdout.to_string(), "Unowned files (3):");
        for (let path in payload.unowned_files) {
            assert.contains(text.stdout.to_string(), path);
        }
        true
    })
}

test "ownership gaps appear in both formats" {
    assert.is_true(unowned_output_case());
}

function compiler_output_case() -> bool {
    with_fixture((dir) -> {
        write(dir, ".github/codeturtle/extra.baml", "class {");
        let payload = report(fixture_cli(dir, mode = "json"), expected = 1);
        let text = fixture_cli(dir, mode = "text");
        expect_exit(text, 1);
        assert.contains(payload.errors[0].message, "diagnostics:");
        assert.contains(text.stdout.to_string(), "class is missing a name");
        assert.contains(text.stdout.to_string(), "extra.baml, offsets");
        assert.equal(text.stdout.to_string().includes("DiagnosticHighlight"), false);
        true
    })
}

test "compiler errors have readable headlines and source locations" {
    assert.is_true(compiler_output_case());
}

function setup_output_case() -> bool {
    with_fixture((dir) -> {
        let payload = report(fixture_cli(dir, mode = "json", extra = ["--concurrency", "0"]), expected = 2);
        let text = fixture_cli(dir, mode = "text", extra = ["--concurrency", "0"]);
        expect_exit(text, 2);
        assert.equal(payload.evaluation_complete, false);
        assert.contains(text.stdout.to_string(), "--concurrency must be a positive integer");
        true
    })
}

test "setup errors respect the requested output format" {
    assert.is_true(setup_output_case());
}
