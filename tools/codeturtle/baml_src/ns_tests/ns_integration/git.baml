function real_diff_case() -> bool {
    with_git((dir, base) -> {
        change_code(dir);
        let head = commit(dir, "Add timeouts");
        let payload = git_check(dir, ["--base", base, "--head", head]);
        let context = payload.context ?? throw "Missing Git context";
        assert.equal(context.base_revision, base);
        assert.equal(context.head_revision, head);
        assert.equal(context.policy_revision, base);
        assert.equal(context.policy_source, "base_revision");
        assert.is_true(root.tests.requirements(payload, "@hellovai").length() > 0);
        assert.is_true(root.tests.requirements(payload, "@sdk-team").length() > 0);
        let change = snapshot(payload, "src/timeout.rs");
        assert.equal(change.status, "M");
        assert.equal(change.before, "let greeting = \"hello\";\n");
        assert.contains(change.after ?? "", "BAML_REQUEST_TIMEOUT");
        assert.contains(change.removed_code ?? "", "let greeting = \"hello\";");
        let hunks = change.hunks ?? throw "Missing hunks";
        assert.equal(hunks[0].new_start, 1);
        true
    })
}

test "real Git diffs supply contents, hunks and trusted revision context" {
    assert.is_true(real_diff_case());
}

function docs_case() -> bool {
    with_git((dir, base) -> {
        change_code(dir);
        write(dir, "docs/api.md", "Documents with_timeout.\n");
        let head = commit(dir, "Document API");
        let payload = git_check(dir, ["--base", base, "--head", head]);
        assert.equal(root.tests.requirements(payload, "@sdk-team"), []);
        assert.equal(snapshot(payload, "src/api/client.rs").path, "src/api/client.rs");
        true
    })
}

test "Git changes are available together to multi-file predicates" {
    assert.is_true(docs_case());
}

function policy_tamper_case() -> bool {
    with_git((dir, base) -> {
        change_code(dir);
        write(dir, ".github/codeturtle/OWNERS", "* @attacker\n");
        write(dir, ".github/codeturtle/environment.baml", "class { invalid");
        let head = commit(dir, "Try to weaken review");
        let payload = git_check(dir, ["--base", base, "--head", head]);
        assert.equal(root.tests.requirements(payload, "@attacker"), []);
        assert.is_true(root.tests.requirements(payload, "@hellovai").length() > 0);
        assert.equal(payload.errors, []);
        true
    })
}

test "head cannot replace trusted ownership or predicates" {
    assert.is_true(policy_tamper_case());
}

function merge_base_case() -> bool {
    with_git((dir, base) -> {
        git(dir, ["checkout", "-b", "feature"]);
        change_code(dir);
        let feature = commit(dir, "Feature");
        git(dir, ["checkout", "main"]);
        write(dir, "target-only.txt", "Target advanced\n");
        let owners = baml.fs.read(dir + "/.github/codeturtle/OWNERS");
        write(
            dir,
            ".github/codeturtle/OWNERS",
            owners.replace_all("@repo-maintainers", "@target-maintainers"),
        );
        let target = commit(dir, "Advance target policy");
        let payload = git_check(dir, ["--base", target, "--head", feature]);
        let context = payload.context ?? throw "Missing context";
        assert.equal(context.merge_base, base);
        assert.equal(context.policy_revision, target);
        assert.is_true(root.tests.requirements(payload, "@target-maintainers").length() > 0);
        assert.equal(
            payload.requirements
                .map((r) -> {
                    r.path
                })
                .includes("target-only.txt"),
            false,
        );
        true
    })
}

test "diff uses merge-base while policy uses the target tip" {
    assert.is_true(merge_base_case());
}

function file_kinds_case() -> bool {
    with_git((dir, base) -> {
        git(dir, ["mv", "protected/old.rs", "renamed.rs"]);
        baml.fs.remove(dir + "/src/deleted.rs");
        baml.fs.write_bytes(dir + "/assets/data.bin", b"\x00new\x02");
        let head = commit(dir, "Rename, delete and change binary");
        let payload = git_check(dir, ["--base", base, "--head", head]);
        let renamed = snapshot(payload, "renamed.rs");
        assert.equal(renamed.old_path, "protected/old.rs");
        assert.is_true((renamed.status ?? "").starts_with("R"));
        let deleted = snapshot(payload, "src/deleted.rs");
        assert.equal(deleted.status, "D");
        assert.equal(deleted.after, null);
        let binary = snapshot(payload, "assets/data.bin");
        assert.equal(binary.is_binary, true);
        assert.equal(binary.hunks, []);
        assert.equal(root.tests.requirements(payload, "@original-owner")[0].path, "renamed.rs");
        true
    })
}

test "renames, deletions and binary blobs preserve their metadata" {
    assert.is_true(file_kinds_case());
}

function dirty_tree_case() -> bool {
    with_git((dir, base) -> {
        write(dir, "src/timeout.rs", "std::env::var(\"UNCOMMITTED\");\n");
        assert.equal(git_check(dir, ["--base", base, "--head", base]).requirements, []);
        true
    })
}

test "dirty working tree does not alter committed input" {
    assert.is_true(dirty_tree_case());
}

function missing_policy_case() -> bool {
    with_git((dir, base) -> {
        git(dir, ["rm", "-r", ".github/codeturtle"]);
        let head = commit(dir, "Remove policy");
        let failed = git_check(dir, ["--base", head], expected = 2);
        assert.contains(failed.errors[0].message, "--policy-dir");
        let preview = git_check(dir, ["--base", head, "--policy-dir", project_dir() + "/fixture/.github/codeturtle"]);
        let context = preview.context ?? throw "Missing preview context";
        assert.equal(context.policy_source, "local_preview");
        assert.equal(context.policy_revision, null);
        true
    })
}

test "missing committed policy requires an explicit local preview" {
    assert.is_true(missing_policy_case());
}

function invalid_ref_case() -> bool {
    with_git((dir, base) -> {
        let payload = git_check(dir, ["--base", "missing-revision"], expected = 2);
        assert.equal(payload.evaluation_complete, false);
        assert.equal(payload.errors.length(), 1);
        true
    })
}

test "invalid Git revision is a typed setup error" {
    assert.is_true(invalid_ref_case());
}

function literal_paths_case() -> bool {
    with_git((dir, original) -> {
        let names = ["src/name[1] space.rs", "src/name1 space.rs", "src/tab\tname.rs", "src/new\nline.rs"];
        for (let name in names) {
            write(dir, name, "old\n");
        }
        let base = commit(dir, "Unusual filenames");
        let i = 0;
        for (let name in names) {
            write(dir, name, "new_" + i.to_string() + "\n");
            i += 1;
        }
        let head = commit(dir, "Change unusual filenames");
        let payload = git_check(dir, ["--base", base, "--head", head]);
        i = 0;
        for (let name in names) {
            assert.equal(snapshot(payload, name).added_code, "new_" + i.to_string());
            i += 1;
        }
        assert.equal(root.tests.requirements(payload, "@snapshot").length(), 4);
        true
    })
}

test "spaces, tabs, brackets and newlines are literal Git paths" {
    assert.is_true(literal_paths_case());
}

function symlink_case() -> bool {
    with_git((dir, original) -> {
        baml.fs.remove(dir + "/.github/codeturtle/environment.baml");
        baml.fs.symlink("OWNERS", dir + "/.github/codeturtle/environment.baml");
        let base = commit(dir, "Invalid symlink policy");
        let payload = git_check(dir, ["--base", base], expected = 2);
        assert.contains(payload.errors[0].message, "regular files");
        true
    })
}

test "policy symlinks are rejected before execution" {
    assert.is_true(symlink_case());
}

function compilation_context_case() -> bool {
    with_git((dir, original) -> {
        write(dir, ".github/codeturtle/environment.baml", "class {");
        let base = commit(dir, "Invalid source");
        let payload = git_check(dir, ["--base", base], expected = 1);
        let context = payload.context ?? throw "Missing error context";
        assert.equal(context.policy_revision, base);
        assert.equal(payload.errors.length(), 1);
        assert.equal(payload.evaluation_complete, false);
        true
    })
}

test "compilation failure preserves Git revision context" {
    assert.is_true(compilation_context_case());
}

function coverage_exit_case() -> bool {
    with_git((dir, original) -> {
        write(dir, ".github/codeturtle/OWNERS", "/src/timeout.rs @owner\n");
        let base = commit(dir, "Narrow ownership");
        write(dir, "other.rs", "new\n");
        let head = commit(dir, "Unowned change");
        let payload = git_check(dir, ["--base", base, "--head", head], expected = 1);
        assert.equal(payload.unowned_files, ["other.rs"]);
        assert.equal(payload.errors, []);
        true
    })
}

test "unowned Git changes produce nonzero CLI exit" {
    assert.is_true(coverage_exit_case());
}

function committed_lint_case() -> bool {
    with_git((dir, base) -> {
        write(dir, ".github/codeturtle/OWNERS", "* @owner # codeturtle: unknown\n");
        let committed = cli(["lint", "--repo", dir, "--rev", base, "--mode=json"]);
        let local = cli(["lint", "--repo", dir, "--mode=json"]);
        expect_exit(committed, 0);
        expect_exit(local, 1);
        assert.is_true(baml.json.from_string<root.LintReport>(committed.stdout.to_string()).valid);
        assert.equal(baml.json.from_string<root.LintReport>(local.stdout.to_string()).valid, false);
        true
    })
}

test "lint can use committed policy instead of a dirty local policy" {
    assert.is_true(committed_lint_case());
}

function shell_quote(value: string) -> string {
    "'" + value.replace_all("'", "'\"'\"'") + "'"
}

function git_parallel_case() -> bool {
    with_git((dir, base) -> {
        change_code(dir);
        write(dir, "src/new.rs", "new file\n");
        let head = commit(dir, "Several changed files");
        let sequential = git_check(dir, ["--base", base, "--head", head, "--concurrency", "1"]);
        let real_git = success(exec("which", ["git"]));
        let trace = dir + "/trace";
        let bin = dir + "/bin";
        mkdir(trace + "/active");
        mkdir(bin);
        let shim = [
            "#!/bin/sh",
            "active=" + shell_quote(trace + "/active"),
            "mkdir \"$active/$$\"",
            "trap 'rmdir \"$active/$$\"' EXIT",
            "count=0",
            "for marker in \"$active\"/*; do",
            "  [ -d \"$marker\" ] && count=$((count+1))",
            "done",
            "printf '%s\\n' \"$count\" >> " + shell_quote(trace + "/counts"),
            "sleep 0.05",
            shell_quote(real_git) + " \"$@\"",
        ]
            .join("\n")
            + "\n";
        baml.fs.write(bin + "/git", shim);
        baml.fs.chmod(bin + "/git", 493);
        let parallel = git_check(
            dir,
            ["--base", base, "--head", head, "--concurrency", "3"],
            env = { "PATH": bin + ":" + (baml.env.get("PATH") ?? throw "Missing PATH") },
        );
        assert.equal(baml.json.to_string(parallel), baml.json.to_string(sequential));
        let peak = 0;
        for (let line in baml.fs.read(trace + "/counts").lines()) {
            let count = baml.json.from_string<int>(line);
            if (count > peak) {
                peak = count;
            }
        }
        assert.is_true(peak >= 2 && peak <= 3);
        assert.equal(baml.fs.read_dir(trace + "/active").length(), 0);
        true
    })
}

test "parallel Git subprocesses are bounded and match sequential results" {
    assert.is_true(git_parallel_case());
}
