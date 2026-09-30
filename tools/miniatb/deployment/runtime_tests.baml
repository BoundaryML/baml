function runtime_fixture(id: string) -> Feedback {
    Feedback {
        id: id,
        source: "Slack",
        description: "original",
        reported_version: null,
        files: {},
        source_url: null,
        channel: null,
        thread: null,
    }
}

function test_folder() -> string {
    safe_exec("mktemp", ["-d", "/tmp/miniatb-test.XXXXXX"])
}

test "queue retries preserve original feedback and completed status" {
    let folder = test_folder();
    defer { baml.fs.remove_dir_all(folder); }
    let feedback = runtime_fixture("event-one");
    enqueue(feedback, directory = folder);
    let path = folder + "/queue/" + feedback_id(feedback) + ".json";
    let job = read_json<Job>(path);
    job.status = "completed";
    write_json(path, job.to_json());
    feedback.description = "retry must not overwrite";
    enqueue(feedback, directory = folder);
    let saved = read_json<Job>(path);
    assert.equal(saved.status, "completed");
    assert.equal(saved.feedback.description, "original");
}

test "concurrent enqueue publishes one complete job" {
    let folder = test_folder();
    defer { baml.fs.remove_dir_all(folder); }
    let feedback = runtime_fixture("concurrent-event");
    let a = spawn { enqueue(feedback, directory = folder) };
    let b = spawn { enqueue(feedback, directory = folder) };
    await a;
    await b;
    assert.equal(baml.fs.read_dir(folder + "/queue").length(), 1);
    assert.equal(
        read_json<Job>(folder + "/queue/" + feedback_id(feedback) + ".json").status,
        "queued",
    );
}

test "PostHog cursor survives pages and rejects stale checkpoints" {
    let folder = test_folder();
    defer { baml.fs.remove_dir_all(folder); }
    let first = posthog_state("42", "https://us.posthog.com", directory = folder);
    posthog_checkpoint(
        {
            "after": "2026-09-19 12:00:00.000001",
            "uuid": "event-1",
            "until": first.until,
            "complete": false,
        }
            .to_json(),
        directory = folder,
    );
    let second = posthog_state("42", "https://us.posthog.com", directory = folder);
    assert.equal(second.until, first.until);
    assert.equal(second.uuid, "event-1");
    let before = baml.fs.read(folder + "/posthog.json");
    let rejected = false;
    posthog_checkpoint({ "until": "wrong", "complete": true }.to_json(), directory = folder) catch_all (_) {
        _ => {
            rejected = true;
        }
    };
    assert.is_true(rejected);
    assert.equal(baml.fs.read(folder + "/posthog.json"), before);
    posthog_checkpoint({ "until": second.until, "complete": true }.to_json(), directory = folder);
    let saved = read_json<PosthogState>(folder + "/posthog.json");
    assert.equal(saved.until, null);
    assert.equal(saved.uuid, "");
    assert.equal(
        saved.after,
        (baml.time.Instant.parse(first.until) - baml.time.Duration.from_seconds(300)).to_string(),
    );
    posthog_state("43", "https://us.posthog.com", directory = folder);
    assert.equal(read_json<PosthogState>(folder + "/posthog.json").project, "43");
}

test "repro paths cannot escape or install settings" {
    for (
        let path in [
            "../escape.baml",
            "/tmp/escape.baml",
            ".git/config",
            "foo/.claude/settings.toml",
            "foo\\bar.baml",
            "test.py",
        ]
    ) {
        let rejected = false;
        let files: map<string, string> = { "baml.toml": "[package]\nname=\"repro\"" };
        files.set(path, "bad");
        validate_repro(files) catch_all (_) {
            _ => {
                rejected = true;
            }
        };
        assert.is_true(rejected);
    }
    validate_repro(
        {
            "baml.toml": "[package]\nname=\"repro\"",
            "src/main.baml": "function example() -> int { 1 }",
        },
    );
    let rejected = false;
    validate_repro({ "main.baml": "missing project" }) catch_all (_) {
        _ => {
            rejected = true;
        }
    };
    assert.is_true(rejected);
}

test "run IDs cannot select another directory" {
    assert.is_true(valid_run_id("0123456789abcdef01234567"));
    for (let id in ["../elsewhere", "", "0123456789abcdef0123456g"]) {
        assert.is_true(!valid_run_id(id));
    }
}

test "publishing requires explicit opt in" {
    if (baml.env.get("MINIATB_ALLOW_PUSH") != "1") {
        let rejected = false;
        require_publish_enabled() catch_all (_) {
            _ => {
                rejected = true;
            }
        };
        assert.is_true(rejected);
    }
}

test "publishing rejects protected paths and symlinks" {
    for (
        let name in [
            ".github/workflows/ci.yaml",
            ".git/config",
            "x/.env",
            "x/.env.production",
            "x/.codex/config.toml",
            "pnpm-lock.yaml",
            ".infisical-scan.toml",
            "nested/.infisical-scan.toml",
            ".infisical-scan-ignore",
            "nested/.gitleaks.toml",
            "nested/.gitleaksignore",
            "../source.rs",
            "keys.pem",
            "node_modules/a.js",
        ]
    ) {
        assert.is_true(!safe_change(name));
    }
    let folder = test_folder();
    defer { baml.fs.remove_dir_all(folder); }
    baml.fs.write(folder + "/safe.rs", "fn main() {}\n");
    validate_change(folder, "safe.rs");
    baml.fs.symlink("safe.rs", folder + "/link.rs");
    let rejected = false;
    validate_change(folder, "link.rs") catch_all (_) {
        _ => {
            rejected = true;
        }
    };
    assert.is_true(rejected);
    baml.fs.write(folder + "/safe.rs", "<<<<<<< conflict\n");
    rejected = false;
    validate_change(folder, "safe.rs") catch_all (_) {
        _ => {
            rejected = true;
        }
    };
    assert.is_true(rejected);
}

test "streamed process output is bounded and both pipes drain" {
    let child = baml.sys.start_process(
        "/bin/sh",
        ["-c", "yes x | head -c 100000; yes y | head -c 100000 >&2"],
        baml.sys.ProcessOptions { stderr: baml.sys.StderrMode.Pipe },
    );
    defer { child.close(); }
    child.stdin.close();
    let out = spawn { bounded_output(child.stdout) };
    let err = spawn { bounded_output(child.stderr ?? baml.sys.panic("Missing stderr")) };
    assert.equal(child.wait().exit_code, 0);
    assert.equal((await out).length(), 65536 + "\n[output truncated]".length());
    assert.equal((await err).length(), 65536 + "\n[output truncated]".length());
}

/// Explicit container-only integration test; no model or external service calls.
function test_sandbox() -> void {
    let folder = test_folder();
    defer { baml.fs.remove_dir_all(folder); }
    mkdir(folder + "/worktree");
    mkdir(folder + "/scratch");
    baml.fs.chmod(folder, 511);
    baml.fs.chmod(folder + "/scratch", 511);
    baml.fs.write(folder + "/secret", "must not be visible");
    baml.fs.write(folder + "/worktree/.git", "gitdir: /private/git");
    baml.fs.write(folder + "/worktree/source.baml", "original");
    mkdir(toolchain_dir());
    let command = "test ! -e "
        + folder
        + "/secret && test ! -s .git && test -z \"$ATB2_GITHUB_TOKEN\" && test -z \"$FEEDBACK_SUPABASE_KEY\" && ! touch source.baml && touch /scratch/ok && test ! -e /app && test \"$(ls /sys/class/net 2>/dev/null | wc -l)\" = 0";
    let result = sandbox(command, folder + "/worktree", folder + "/scratch");
    assert.equal(result.exit_code, 0);
    assert.equal(sandbox("exit 1", folder + "/worktree", folder + "/scratch").exit_code, 1);
    for (let command in ["exit 125", "kill -KILL $$", "sleep 2"]) {
        let rejected = false;
        sandbox(command, folder + "/worktree", folder + "/scratch", seconds = 1) catch_all (_) {
            _ => {
                rejected = true;
            }
        };
        assert.is_true(rejected);
    }
    assert.equal(baml.fs.read(folder + "/worktree/source.baml"), "original");
    assert.is_true(baml.fs.exists(folder + "/scratch/ok"));
    baml.fs.chmod(folder + "/worktree", 511);
    baml.fs.chmod(folder + "/worktree/source.baml", 438);
    let edited = sandbox(
        "printf fixed > source.baml; printf saved > /scratch/result; cargo --version; rustc --version",
        folder + "/worktree",
        folder + "/scratch",
        writable = true,
    );
    assert.equal(edited.exit_code, 0);
    assert.equal(baml.fs.read(folder + "/worktree/source.baml"), "fixed");
    assert.equal(baml.fs.read(folder + "/scratch/result"), "saved");
    assert.equal(baml.fs.read(folder + "/worktree/.git"), "gitdir: /private/git");
    let temporary = sandbox(
        "printf generated > source.baml; printf artifact > /scratch/artifact",
        folder + "/worktree",
        folder + "/scratch",
        writable = true,
        persist_source = false,
    );
    assert.equal(temporary.exit_code, 0);
    assert.equal(baml.fs.read(folder + "/worktree/source.baml"), "fixed");
    assert.equal(baml.fs.read(folder + "/scratch/artifact"), "artifact");
}

/// Runs actual passing and failing BAML tests through the sandbox, with no LLMs.
function test_replay() -> void {
    mkdir(run_dir() + "/scratch");
    baml.fs.chmod(run_dir() + "/scratch", 511);
    mkdir(toolchain_dir() + "/bin");
    safe_exec("cp", ["--", controller_cli(), toolchain_dir() + "/bin/baml"]);
    let repro = Repro {
        kind: Kind.Bug,
        files: {
            "baml.toml": "[package]\nname = \"repro\"\n",
            "main.baml": "test \"example\" { assert.equal(1, 1) }\n",
        },
        command: "test",
        explanation: "offline fixture",
        blocker: null,
    };
    let passing = replay(repro);
    assert.equal(passing.exit_code, 0);
    repro.files.set("main.baml", "test \"example\" { assert.equal(1, 2) }\n");
    assert.is_true(replay(repro).exit_code != 0);
    mkdir(run_dir() + "/verified/bin");
    safe_exec("cp", ["--", controller_cli(), run_dir() + "/verified/bin/baml"]);
    baml.fs.write(toolchain_dir() + "/bin/baml", "#!/bin/sh\nexit 0\n");
    assert.equal(replay(repro).exit_code, 0);
    assert.is_true(replay(repro, cli = run_dir() + "/verified").exit_code != 0);

    assert.equal(
        baml.fs.read_dir(run_dir())
            .filter((entry) -> {
                entry.name.starts_with("replay.")
            })
            .length(),
        0,
    );
}
