function safe_change(name: string) -> bool {
    !name.starts_with("/")
        && !name.includes("\\")
        && ![".github/", ".git", ".agents/", ".claude/"].some((prefix) -> {
            name.starts_with(prefix)
        })
        && !name.split("/").some((part) -> {
            part.starts_with(".env")
                || [
                    "..",
                    ".git",
                    ".github",
                    ".agents",
                    ".codex",
                    ".claude",
                    ".ssh",
                    ".aws",
                    ".infisical-scan.toml",
                    ".infisical-scan-ignore",
                    ".gitleaks.toml",
                    ".gitleaksignore",
                    "__pycache__",
                    "node_modules",
                    "target",
                    ".baml",
                    "pnpm-lock.yaml",
                    "package-lock.json",
                    "bun.lockb",
                ]
                    .includes(part)
        })
        && ![".sql", ".pem", ".key", ".lock"].some((suffix) -> {
            name.ends_with(suffix)
        })
}

function validate_change(tree: string, name: string) -> void {
    require(safe_change(name), "Protected source path");
    let parts = name.split("/");
    let parent = tree;
    for (let part in parts) {
        let entry = baml.fs.read_dir(parent).find((entry) -> {
            entry.name == part
        });
        if (entry == null) {
            return;
        } // Deleted file.
        require(!entry.is_symlink, "Symlinked change");
        parent = parent + "/" + part;
    }
    require(baml.fs.size(parent) <= 1000000, "Source artifact too large");
    let file = baml.fs.open(parent, "r");
    defer { file.close(); }
    let raw = file.bytes();
    let text = raw.to_string();
    require(
        !raw.includes(0)
            && !text.includes("/Users/")
            && !text.includes("/home/")
            && !(text.includes("-----BEGIN ") && text.includes("PRIVATE KEY"))
            && !text.split("\n").some((line) -> {
                line.starts_with("<<<<<<< ")
                    || line.trim() == "======="
                    || line.starts_with(">>>>>>> ")
            }),
        "Unsafe source artifact",
    );
}

function require_publish_enabled() -> void {
    require(
        baml.env.get("MINIATB_ALLOW_PUSH") == "1",
        "Push disabled; set MINIATB_ALLOW_PUSH=1 for an authorized fix run",
    );
}

/// Scanner policy belongs to the controller, never to the agent worktree.
function scan_worktree(tree: string) -> void {
    safe_exec(
        "infisical",
        [
            "scan",
            "--no-git",
            "--source",
            tree,
            "--config",
            app_dir() + "/deployment/secret-scan.toml",
            "--redact",
            "--telemetry=false",
        ],
    );
}

function publish(issue: Issue, fix: Fix) -> string {
    require_publish_enabled();
    let tree = run_dir() + "/worktree";
    let saved = read_json<RunInput>(run_dir() + "/input.json");
    let branch = saved.workspace.branch;
    require(
        issue.workspace.id == saved.workspace.id
            && branch.starts_with("bammy/")
            && safe_exec("git", ["-C", tree, "branch", "--show-current"]) == branch,
        "Unexpected branch",
    );
    let changed = safe_exec("git", ["-C", tree, "ls-files", "--modified", "--others", "--exclude-standard", "-z"])
        .split("\0");
    for (let name in changed) {
        if (name != "") {
            validate_change(tree, name);
        }
    }
    scan_worktree(tree);
    safe_exec("git", ["-C", tree, "add", "-A"]);
    safe_exec("git", ["-C", tree, "diff", "--cached", "--check"]);
    let base = safe_exec("git", ["-C", tree, "rev-parse", "HEAD"]);
    let tree_sha = safe_exec("git", ["-C", tree, "write-tree"]);
    require(
        tree_sha != safe_exec("git", ["-C", tree, "rev-parse", "HEAD^{tree}"]),
        "No changes to publish",
    );
    let sha = safe_exec(
        "git",
        [
            "-c",
            "user.name=atx24",
            "-c",
            "user.email=atx.dhilan@gmail.com",
            "-C",
            tree,
            "commit-tree",
            tree_sha,
            "-p",
            base,
            "-m",
            issue.ticket.title,
        ],
    );
    write_json(
        run_dir() + "/publication.json",
        {
            "branch": branch,
            "sha": sha,
            "base": base,
            "phase": "prepared",
            "pr": null,
            "title": issue.ticket.title,
            "body": [
                issue.ticket.description.join(" "),
                issue.ticket.investigation.join(" "),
                fix.summary,
                "Verification: rebuilt the modified CLI and independently replayed the original reproduction.",
                "Additional agent tests: `" + fix.test_command + "`",
                "Issue: " + issue_url(issue.id),
            ]
                .join("\n\n"),
        }
            .to_json(),
    );
    resume_publication(issue)
}

function published_pr() -> string? {
    let path = run_dir() + "/publication.json";
    if (!baml.fs.exists(path)) {
        return null;
    };
    baml.json.path_or<string?>(read_json<json>(path), ".pr", null)
}

function resume_publication(issue: Issue) -> string {
    require_publish_enabled();
    require(!issue_cancelled(issue.id), "Issue cancelled");
    let token = github_token("publish");
    checked(
        "node",
        [app_dir() + "/deployment/publish-pr.mjs", run_dir(), issue.workspace.branch],
        options = baml.sys.ProcessOptions {
            env: {
                "PATH": "/usr/local/bin:/usr/bin:/bin",
                "HOME": "/tmp",
                "GH_TOKEN": token,
                "GIT_ASKPASS": app_dir() + "/deployment/askpass",
                "MINIATB_PUSH_TOKEN": token,
                "GIT_CONFIG_NOSYSTEM": "1",
                "GIT_CONFIG_GLOBAL": "/dev/null",
                "GIT_TERMINAL_PROMPT": "0",
            },
            timeout_ms: 600000,
        },
    )
}

/// Called under the same lock as pipeline jobs; a toolchain cannot change mid-fix.
function fix_saved(id: string) -> string? {
    require(
        run_dir() == run_path(id) && baml.env.get("MINIATB_FIX") == "1",
        "Fix environment not configured",
    );
    let issue = read_json<Issue>(run_dir() + "/issue.json");
    require(
        baml.fs.read(toolchain_dir() + "/version") == issue.workspace.version,
        "Toolchain changed; reproduce again before fixing",
    );
    // Source was read-only during investigation; only this explicit fix run writes it.
    safe_exec("chmod", ["-R", "a+rwX", run_dir() + "/worktree"]);
    fix_issue(issue)
}

function fix(id: string) -> void {
    run_path(id);
    let environment = controller_env(id);
    environment.set("MINIATB_FIX", "1");
    checked(
        "timeout",
        [
            "--kill-after=10s",
            "3600s",
            "flock",
            "--nonblock",
            "--close",
            data_dir() + "/worker.lock",
            controller_cli(),
            "run",
            "--directory",
            app_dir(),
            "--agent-skill-check",
            "off",
            "fix_saved",
            "--",
            "--id",
            id,
        ],
        options = baml.sys.ProcessOptions { env: environment },
    );
}
