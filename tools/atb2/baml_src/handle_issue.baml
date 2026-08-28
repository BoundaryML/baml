// issue -> fix (or design doc).
//
//   Easy / Medium: an agent in a git worktree on a fresh branch implements
//                  the fix, encodes the repro(s) as tests, and runs the
//                  repo's gate; BAML re-runs the gate, pushes and opens a
//                  DRAFT PR (Live) or stops before pushing (DryRun).
//   Hard:          the agent writes a suggested fix + design doc for the
//                  shepherd to hand to their own agent. No code change.
//
// DELIBERATE DEPARTURE from create_issue's "no subprocess" rule: this stage
// exists to run git, cargo and the Claude Code CLI. Every child process gets
// an explicit environment (sandbox_env — it REPLACES the environment, so the
// agent never sees this process's secrets), a working directory inside a
// per-issue worktree, and a timeout. The worktree is removed in a `defer`,
// so nothing the agent wrote survives outside the branch.
//
// Layout (outside the repo):
//   ~/.atb2/repo/                 cached clone of BoundaryML/baml (fetched each run)
//   ~/.atb2/target/               shared CARGO_TARGET_DIR (warm builds)
//   ~/.atb2/worktrees/<branch>/   the agent's checkout, deleted at the end
//   ~/.atb2/runs/<branch>/        audit trail: transcript.jsonl, pr_body.md, outcome.json

enum HandleMode {
    /// Push the branch and open a draft PR when the gate passes.
    Live,
    /// Everything up to (not including) the push — what the evals run.
    DryRun,
}

// ==================== Pipeline ====================

function handle_issue(
    issue: Issue,
    mode: HandleMode = HandleMode.Live,
    update: bool = false,
) -> Issue {
    let skip = handle_ready(issue, update);
    if (skip != null) {
        log.info({ "issue": issue.id, "skipped": skip });
        return issue;
    };
    let difficulty = issue.difficulty ?? baml.sys.panic("handle_issue: gauge_issue first (" + issue.id + ")");

    // 0. Is it still a bug on this toolchain? (in-process repro check, no sandbox)
    if (!still_broken(issue)) {
        log.info({ "issue": issue.id, "kind": "already_fixed", "toolchain": toolchain_version() });
        return issue;
    };

    ensure_cache();
    let sb = open_sandbox(issue, update);
    let keep_branch = false;
    defer {
        close_sandbox(sb, keep_branch);
    }
    let budget = time_budget_s(difficulty);
    let started = baml.time.Instant.now();

    // Hard: suggested fix + design doc, read-only.
    if (difficulty == Difficulty.Hard) {
        let d = run_agent_design(sb, issue, budget);
        if (d == null) {
            write_outcome(sb, stopped_outcome(sb, started));
            return issue;
        };
        let doc = render_design_doc(d ?? baml.sys.panic("unreachable"));
        write_outcome(
            sb,
            HandleOutcome {
                kind: "hard",
                branch: sb.branch,
                pr: null,
                turns: read_turns(sb),
                seconds: elapsed_s(started),
                timed_out: false,
                gate: null,
                report: null,
                design_doc: doc,
            },
        );
        return with_design_doc(issue, doc);
    };

    // Easy / Medium: fix + tests + gate + PR.
    warm_build(sb);
    let report = run_agent_fix(sb, issue, budget);
    if (report == null) {
        keep_branch = mode == HandleMode.Live;
        write_outcome(sb, stopped_outcome(sb, started));
        return issue;
    };
    let fix = report ?? baml.sys.panic("unreachable");
    checkpoint_uncommitted(sb);
    let gate = run_gate(sb.worktree);
    let turns = read_turns(sb);
    let seconds = elapsed_s(started);
    if (!gate.ok) {
        keep_branch = mode == HandleMode.Live;
        write_outcome(
            sb,
            HandleOutcome {
                kind: "gate_failed",
                branch: sb.branch,
                pr: null,
                turns: turns,
                seconds: seconds,
                timed_out: false,
                gate: gate,
                report: fix,
                design_doc: null,
            },
        );
        return issue;
    };

    let draft = PrDraft { title: pr_title(issue), body: pr_body(issue, fix, gate, turns, seconds) };
    baml.fs.write(sb.run_dir + "/pr_body.md", draft.body);
    if (mode == HandleMode.DryRun) {
        write_outcome(
            sb,
            HandleOutcome {
                kind: "fixed",
                branch: sb.branch,
                pr: null,
                turns: turns,
                seconds: seconds,
                timed_out: false,
                gate: gate,
                report: fix,
                design_doc: null,
            },
        );
        return issue;
    };

    let pr = push_and_open_pr(sb, draft);
    write_outcome(
        sb,
        HandleOutcome {
            kind: "fixed",
            branch: sb.branch,
            pr: pr,
            turns: turns,
            seconds: seconds,
            timed_out: false,
            gate: gate,
            report: fix,
            design_doc: null,
        },
    );
    log.info({ "issue": issue.id, "pr": pr, "turns": turns, "seconds": seconds });
    with_status(issue, InProgress { state: "in_progress", pr: pr })
}

/// Why the stage would not run on this issue; null means go.
function handle_ready(issue: Issue, update: bool) -> string? {
    if (issue.difficulty == null) {
        return "not gauged";
    };
    if (update) {
        return null;
    };
    if (issue.design_doc != null) {
        return "already has a design doc";
    };
    match (issue.status) {
        let s: Open => null,
        _ => "not open",
    }
}

/// True unless EVERY repro comes back Fixed on this toolchain. Inconclusive
/// (needs a human, or a command the in-process check cannot verify) counts
/// as still broken — the agent gets to look.
function still_broken(issue: Issue) -> bool {
    if (issue.repros.length() == 0) {
        return true;
    };
    for (let r in issue.repros) {
        if (run_repro(r).verdict != Verdict.Fixed) {
            return true;
        };
    }
    false
}

function with_status(issue: Issue, status: IssueStatus) -> Issue {
    Issue {
        id: issue.id,
        title: issue.title,
        description: issue.description,
        shepherd: issue.shepherd,
        subsystem: issue.subsystem,
        repros: issue.repros,
        version: issue.version,
        feedback_ids: issue.feedback_ids,
        status: status,
        comments: issue.comments,
        resolution_plan: issue.resolution_plan,
        difficulty: issue.difficulty,
        design_doc: issue.design_doc,
    }
}

function with_design_doc(issue: Issue, doc: string) -> Issue {
    Issue {
        id: issue.id,
        title: issue.title,
        description: issue.description,
        shepherd: issue.shepherd,
        subsystem: issue.subsystem,
        repros: issue.repros,
        version: issue.version,
        feedback_ids: issue.feedback_ids,
        status: issue.status,
        comments: issue.comments,
        resolution_plan: issue.resolution_plan,
        difficulty: issue.difficulty,
        design_doc: doc,
    }
}

// ==================== Config ====================

function atb2_home() -> string {
    baml.env.get("ATB2_HOME") ?? ((baml.env.get("HOME") ?? "/tmp") + "/.atb2")
}

function cache_repo() -> string {
    atb2_home() + "/repo"
}

function target_dir() -> string {
    atb2_home() + "/target"
}

function repo_url() -> string {
    baml.env.get("ATB2_REPO_URL") ?? "https://github.com/BoundaryML/baml.git"
}

function repo_slug() -> string {
    baml.env.get("ATB2_REPO") ?? "BoundaryML/baml"
}

/// Seconds the agent gets. Also the process timeout: the CLI is killed at
/// the budget, which is what "did not finish in budget" means in the eval.
function time_budget_s(d: Difficulty) -> int {
    match (d) {
        Difficulty.Trivial => 1800,
        Difficulty.Easy => 1800,
        Difficulty.Medium => 3600,
        Difficulty.Hard => 1800,
    }
}

// ==================== Naming (pure) ====================

/// "GH-4587" -> 4587; null for pipeline ids ("ISSUE-<uuid>").
function github_number(issue: Issue) -> int? {
    if (!issue.id.starts_with("GH-")) {
        return null;
    };
    int.parse(issue.id.slice(3, issue.id.length())) catch (_) {
        _ => null,
    }
}

/// Lowercase, non-alphanumerics collapsed to "-", trimmed, at most `max`.
function slug(title: string, max: int = 40) -> string {
    let out = "";
    let last_dash = true;
    for (let c in title.to_lower_case().chars()) {
        let keep = (c >= "a" && c <= "z") || (c >= "0" && c <= "9");
        if (keep) {
            out = out + c;
            last_dash = false;
        } else if (!last_dash) {
            out = out + "-";
            last_dash = true;
        };
    }
    let trimmed = if (out.ends_with("-")) {
        out.slice(0, out.length() - 1)
    } else {
        out
    };
    let cut = if (trimmed.length() > max) {
        trimmed.slice(0, max)
    } else {
        trimmed
    };
    if (cut.ends_with("-")) {
        cut.slice(0, cut.length() - 1)
    } else {
        cut
    }
}

function branch_for(issue: Issue) -> string {
    let n = github_number(issue);
    let head = if (n != null) {
        "gh-" + (n ?? 0).to_string()
    } else {
        "issue-" + slug(issue.id.replace("ISSUE-", ""), max = 8)
    };
    "agent/" + head + "-" + slug(issue.title)
}

function branch_dir_name(branch: string) -> string {
    branch.replace_all("/", "__")
}

function worktree_for(branch: string) -> string {
    atb2_home() + "/worktrees/" + branch_dir_name(branch)
}

function run_dir_for(branch: string) -> string {
    atb2_home() + "/runs/" + branch_dir_name(branch)
}

// ==================== Processes ====================

class Cmd {
    program: string,
    args: string[],
    cwd: string,
    timeout_ms: int,
}

class CmdResult {
    cmd: string,
    exit_code: int,
    stdout: string,
    stderr: string,
    ok: bool,
}

/// The environment every child gets. It REPLACES the environment, so this
/// is an allowlist: nothing from this process leaks unless named here.
/// The agent runs with `with_gh = false` and never sees GH_TOKEN,
/// FEEDBACK_SUPABASE_KEY, ANTHROPIC_API_KEY or anything else.
function sandbox_env(with_gh: bool) -> map<string, string> {
    let env: map<string, string> = {
        "PATH": baml.env.get("PATH") ?? "/usr/local/bin:/usr/bin:/bin",
        "HOME": baml.env.get("HOME") ?? "/tmp",
        "TERM": "dumb",
        "LANG": baml.env.get("LANG") ?? "C.UTF-8",
        "GIT_TERMINAL_PROMPT": "0",
        "CARGO_TARGET_DIR": target_dir(),
        "CARGO_PROFILE_DEV_OPT_LEVEL": "1",
        "CARGO_PROFILE_TEST_OPT_LEVEL": "1",
        "CARGO_INCREMENTAL": "0",
    };
    for (let k in ["CARGO_HOME", "RUSTUP_HOME", "RUSTUP_TOOLCHAIN", "SSH_AUTH_SOCK"]) {
        let v = baml.env.get(k);
        if (v != null) {
            env.set(k, v ?? "");
        };
    }
    if (with_gh) {
        let tok = baml.env.get("GH_TOKEN") ?? baml.env.get("GITHUB_TOKEN");
        if (tok != null) {
            env.set("GH_TOKEN", tok ?? "");
        };
    }
    env
}

function run_with(c: Cmd, env: map<string, string>) -> CmdResult {
    let label = c.program + " " + c.args.join(" ");
    let out = baml.sys.exec(
        c.program,
        c.args,
        baml.sys.ProcessOptions {
            cwd: c.cwd,
            env: env,
            timeout_ms: c.timeout_ms,
            stdin: null,
            keep_stdin_open: null,
        },
    ) catch (e) {
        baml.errors.Timeout => {
            return CmdResult {
                cmd: label,
                exit_code: 124,
                stdout: "",
                stderr: "timed out after " + c.timeout_ms.to_string() + "ms",
                ok: false,
            };
        },
        _ => {
            return CmdResult {
                cmd: label,
                exit_code: 127,
                stdout: "",
                stderr: "could not start: " + label,
                ok: false,
            };
        },
    };
    CmdResult {
        cmd: label,
        exit_code: out.exit_code,
        stdout: out.stdout.to_string(),
        stderr: out.stderr.to_string(),
        ok: out.exit_code == 0,
    }
}

function run(c: Cmd) -> CmdResult {
    run_with(c, sandbox_env(false))
}

function git(cwd: string, args: string[]) -> CmdResult {
    run(Cmd { program: "git", args: args, cwd: cwd, timeout_ms: 600000 })
}

/// Last `n` lines of a string — what goes into logs and PR gates.
function tail(s: string, n: int) -> string {
    let ls = s.lines();
    let start = if (ls.length() > n) {
        ls.length() - n
    } else {
        0
    };
    let out: string[] = [];
    let i = start;
    while (i < ls.length()) {
        out.push(ls.at(i) ?? "");
        i += 1;
    }
    out.join("\n")
}

// ==================== Sandbox lifecycle ====================

class Sandbox {
    branch: string,
    worktree: string,
    run_dir: string,
    reused_branch: bool,
}

/// One cached clone, fetched each run. Safe to call from parallel tests:
/// the fetch is retried when another child holds the index lock.
function ensure_cache() -> null {
    let repo = cache_repo();
    baml.fs.mkdir(atb2_home(), baml.fs.MkdirOptions { recursive: true });
    baml.fs.mkdir(target_dir(), baml.fs.MkdirOptions { recursive: true });
    if (!baml.fs.exists(repo + "/.git")) {
        let clone = run(
            Cmd {
                program: "git",
                args: ["clone", "--origin", "origin", "--branch", "canary", repo_url(), repo],
                cwd: atb2_home(),
                timeout_ms: 1800000,
            },
        );
        if (!clone.ok) {
            baml.sys.panic("clone failed: " + tail(clone.stderr, 5));
        };
        git(repo, ["config", "extensions.worktreeConfig", "true"]);
    };
    let tries = 0;
    while (tries < 3) {
        tries += 1;
        let f = git(repo, ["fetch", "--prune", "origin", "canary"]);
        if (f.ok) {
            return null;
        };
        baml.sys.sleep(baml.time.Duration.from_seconds(5)) catch (_) {
            _ => null,
        };
    }
    baml.sys.panic("git fetch failed three times")
}

function branch_exists_locally(branch: string) -> bool {
    git(cache_repo(), ["rev-parse", "--verify", "--quiet", "refs/heads/" + branch]).ok
}

function branch_exists_on_origin(branch: string) -> bool {
    git(cache_repo(), ["ls-remote", "--exit-code", "--heads", "origin", branch]).ok
}

function open_sandbox(issue: Issue, update: bool) -> Sandbox {
    let repo = cache_repo();
    let branch = branch_for(issue);
    let wt = worktree_for(branch);
    let rd = run_dir_for(branch);

    // a crashed earlier run may have left the worktree registered
    if (baml.fs.exists(wt)) {
        git(repo, ["worktree", "remove", "--force", wt]);
        if (baml.fs.exists(wt)) {
            baml.fs.remove_dir_all(wt);
        };
    };
    git(repo, ["worktree", "prune"]);
    baml.fs.remove_dir_all(rd);
    baml.fs.mkdir(rd, baml.fs.MkdirOptions { recursive: true });
    baml.fs.mkdir(atb2_home() + "/worktrees", baml.fs.MkdirOptions { recursive: true });

    let reused = false;
    let add = if (branch_exists_locally(branch) && update) {
        reused = true;
        git(repo, ["worktree", "add", wt, branch])
    } else {
        if (branch_exists_locally(branch)) {
            git(repo, ["branch", "-D", branch]);
        };
        if (branch_exists_on_origin(branch)) {
            reused = true;
            git(repo, ["fetch", "origin", branch]);
            git(repo, ["worktree", "add", "--track", "-b", branch, wt, "origin/" + branch])
        } else {
            git(repo, ["worktree", "add", "-b", branch, wt, "origin/canary"])
        }
    };
    if (!add.ok) {
        baml.sys.panic("worktree add failed: " + tail(add.stderr, 5));
    };

    // the agent can run git, but it cannot push: only push_and_open_pr
    // overrides this, and only in Live mode after the gate passed
    git(wt, ["config", "--worktree", "remote.origin.pushurl", "no_push://atb2"]);
    git(wt, ["config", "--worktree", "user.name", "atb2"]);
    git(wt, ["config", "--worktree", "user.email", "atb2@boundaryml.com"]);

    log.info({ "sandbox": wt, "branch": branch, "reused_branch": reused });
    Sandbox { branch: branch, worktree: wt, run_dir: rd, reused_branch: reused }
}

/// Runs from `defer`: never throws. Removes the worktree (and the local
/// branch unless kept for a human), then checks the cache is untouched.
function close_sandbox(sb: Sandbox, keep_branch: bool) -> null {
    let repo = cache_repo();
    git(repo, ["worktree", "remove", "--force", sb.worktree]);
    git(repo, ["worktree", "prune"]);
    if (baml.fs.exists(sb.worktree)) {
        baml.fs.remove_dir_all(sb.worktree) catch (_) {
            _ => null,
        };
    };
    if (!keep_branch) {
        git(repo, ["branch", "-D", sb.branch]);
    };
    if ((baml.env.get("ATB2_KEEP_RUNS") ?? "1") == "0") {
        baml.fs.remove_dir_all(sb.run_dir) catch (_) {
            _ => null,
        };
    };
    if (!cache_is_clean(sb)) {
        log.warn(
            { "sandbox": sb.worktree, "warning": "cache repo is not clean after close_sandbox" },
        );
    };
    null
}

function cache_is_clean(sb: Sandbox) -> bool {
    let status = git(cache_repo(), ["status", "--porcelain"]);
    let list = git(cache_repo(), ["worktree", "list", "--porcelain"]);
    status.ok && status.stdout.trim().length() == 0 && !list.stdout.includes(sb.worktree)
}

/// A first `cargo build` on a cold target dir costs 10-30 minutes; do it
/// before the agent's clock starts.
function warm_build(sb: Sandbox) -> null {
    let b = run(
        Cmd {
            program: "cargo",
            args: ["build", "-p", "baml_cli", "--bin", "baml-cli"],
            cwd: sb.worktree + "/baml_language",
            timeout_ms: 2400000,
        },
    );
    log.info({ "warm_build": b.ok, "tail": tail(b.stderr, 2) });
    null
}

/// Whatever the agent left uncommitted is committed before the worktree is
/// removed, so nothing is lost and the gate sees the real tree.
function checkpoint_uncommitted(sb: Sandbox) -> null {
    let status = git(sb.worktree, ["status", "--porcelain"]);
    if (status.stdout.trim().length() > 0) {
        git(sb.worktree, ["add", "-A"]);
        git(sb.worktree, ["commit", "-q", "-m", "chore(atb2): checkpoint uncommitted work"]);
    };
    null
}

// ==================== The agent ====================

// Only so implement_fix / write_design_doc are LLM functions: their
// $render_prompt / $parse companions render the prompt and parse the reply.
// The CLI itself is run by run_claude, not through this client.
client Handle = claude_code.ClaudeCodeClient.new(model = "opus");

function agent_model() -> string {
    baml.env.get("ATB2_MODEL") ?? "opus"
}

class AgentRun {
    text: string,
    num_turns: int,
    exit_code: int,
    timed_out: bool,
}

/// The `claude -p` argv, mirroring what claude_code.ClaudeCodeClient builds
/// minus --json-schema (the prompt carries the schema via ctx.output_format).
function claude_args(prompt: string, tools: string[]) -> string[] {
    [
        "-p",
        prompt,
        "--output-format",
        "stream-json",
        "--verbose",
        "--model",
        agent_model(),
        "--permission-mode",
        "acceptEdits",
        "--no-session-persistence",
        "--tools",
        tools.join(","),
    ]
}

/// Run one Claude Code session in `cwd` with Claude Code's own tools, killed
/// at `timeout_s`. The stream-json goes to `transcript_path`; the reply text
/// and turn count come from its `result` event.
function run_claude(
    cwd: string,
    prompt: string,
    tools: string[],
    timeout_s: int,
    transcript_path: string,
) -> AgentRun {
    let r = run_with(
        Cmd {
            program: "claude",
            args: claude_args(prompt, tools),
            cwd: cwd,
            timeout_ms: timeout_s * 1000,
        },
        sandbox_env(false),
    );
    baml.fs.write(transcript_path, r.stdout);
    let parsed = parse_stream(r.stdout);
    AgentRun {
        text: parsed.text,
        num_turns: parsed.num_turns,
        exit_code: r.exit_code,
        timed_out: r.exit_code == 124,
    }
}

/// stream-json -> the `result` event's text and num_turns. Without a result
/// event (killed mid-run) the turns are the assistant messages seen and the
/// text is empty. Non-JSON lines are skipped.
function parse_stream(transcript: string) -> AgentRun {
    let text = "";
    let turns = 0;
    let assistants = 0;
    let saw_result = false;
    for (let line in transcript.lines()) {
        if (line.trim().length() == 0) {
            continue;
        };
        let j = baml.json.parse(line) catch (_) {
            _ => null,
        };
        if (j == null) {
            continue;
        };
        let ev = j ?? baml.sys.panic("unreachable");
        let kind = baml.json.path_or<string>(ev, ".type", "");
        if (kind == "assistant") {
            assistants += 1;
        } else if (kind == "result") {
            saw_result = true;
            turns += baml.json.path_or<int>(ev, ".num_turns", 0);
            text = baml.json.path_or<string>(ev, ".result", text);
        };
    }
    AgentRun {
        text: text,
        num_turns: if (saw_result) {
            turns
        } else {
            assistants
        },
        exit_code: 0,
        timed_out: false,
    }
}

function read_turns(sb: Sandbox) -> int {
    let path = sb.run_dir + "/transcript.jsonl";
    if (!baml.fs.exists(path)) {
        return 0;
    };
    parse_stream(baml.fs.read(path)).num_turns
}

function elapsed_s(started: baml.time.Instant) -> int {
    started.elapsed().to_seconds().to_int() catch (_) {
        _ => 0,
    }
}

// The tools the CLI is given: Claude Code's own. The fix run may edit; the
// design run is told not to (and its branch is discarded regardless).
function fix_tools() -> string[] {
    ["Read", "Edit", "Write", "Glob", "Grep", "Bash"]
}

function design_tools() -> string[] {
    ["Read", "Glob", "Grep", "Bash"]
}

/// What the agent must know about where it is and what it may not do. The
/// guarantees that matter (no push, no secrets, cleanup) are enforced
/// outside the prompt too.
function sandbox_rules(branch: string) -> string {
    "You are working in a git worktree of BoundaryML/baml on branch `"
        + branch
        + "`, checked out from `canary`. "
        + "The Rust workspace is `baml_language/` (run cargo from there); `CARGO_TARGET_DIR` is shared and already warm, so "
        + "the built CLI is at `$CARGO_TARGET_DIR/debug/baml-cli`, not `target/`.\n"
        + "Rules:\n"
        + "- Stay on this branch. Never `git push`, never checkout or create another branch, never touch `.git` directly.\n"
        + "- Never create or modify files outside this worktree.\n"
        + "- Commit as you go with conventional-commit messages (`fix(scope): ...`, `test(scope): ...`). Leave the tree clean.\n"
        + "- Never edit `.snap` files by hand; regenerate with `cargo insta test --test-runner=nextest --accept -p baml_tests`.\n"
        + "- Do not change CI, `mise.toml`, or `Cargo.lock` unless the fix genuinely requires it.\n"
        + "- If the gate cannot be made to pass honestly, stop and say so in `blockers`. Never weaken or delete tests to pass.\n"
        + "- Your final message must be exactly the JSON the output format below asks for, nothing else."
}

class FixReport {
    /// 2-4 sentences: the root cause and the change made.
    summary: string,
    /// Markdown for the PR's "## Design" section: what changed and why, the
    /// files touched, and how the repro is now covered by tests.
    design: string,
    /// Paths of the tests that encode the issue's repro(s).
    tests_added: string[],
    files_changed: string[],
    /// What the agent believes about the gate; BAML re-runs it regardless.
    gate_passed: bool,
    /// Why it stopped short, if it did.
    blockers: string?,
}

function implement_fix(issue: Issue, branch: string, test_route: string) -> FixReport {
    client: Handle
    prompt: `
        ${sandbox_rules(branch)}

        Fix this issue in the BAML toolchain.

        Title: ${issue.title}
        Subsystem: ${issue.subsystem.to_string()}

        ${issue.description}

        Resolution plan from triage (a starting point, not gospel):
        ${issue.resolution_plan ?? "(none)"}

        Repros (${issue.repros.length().to_string()}):
        ${issue.repros.map((r: Repro) -> string { format_repro(r) }).join("\n\n")}

        How to work:
        1. Reproduce FIRST. Turn every repro above into a test before touching the fix, placed
           per baml_language/TEST_INSTRUCTIONS.md. For this issue: ${test_route}
           Confirm the new test fails before the fix and passes after it.
        2. Make the smallest correct fix. Cite the files you changed.
        3. Run the gate, in this order, from baml_language/, and make every step pass:
             cargo fmt --all -- --config imports_granularity=Crate --config group_imports=StdExternalCrate
             cargo clippy --workspace --all-targets --all-features -- -D warnings
             cargo test --lib -p <each crate you changed>
             cargo nextest run -p baml_tests
             cargo insta test --test-runner=nextest --accept -p baml_tests   (then commit any .snap changes)
             cargo build -p baml_cli --bin baml-cli && $CARGO_TARGET_DIR/debug/baml-cli test --from crates/baml_tests/baml_src
        4. Commit everything. \`git status --porcelain\` must be empty. Do not push.

        ${ctx.output_format}
    `
}

/// Where the repro's test belongs, per TEST_INSTRUCTIONS.md, from the
/// repros' expectations.
function test_route(issue: Issue) -> string {
    let compile_error = false;
    let behavior = false;
    let inspection = false;
    for (let r in issue.repros) {
        match (r.expectation) {
            let e: ShouldNotCompile => {
                compile_error = true;
            },
            let e: ShouldCompile => {
                behavior = true;
            },
            let e: ShouldEvaluateTo => {
                behavior = true;
            },
            let e: RequiresInspection => {
                inspection = true;
            },
        }
    }
    let routes: string[] = [];
    if (compile_error) {
        routes.push(
            "a compile-error fixture: one .baml file under crates/baml_tests/projects/diagnostic_errors/<name>/ (snapshots are generated; commit them)",
        );
    };
    if (behavior) {
        routes.push(
            "a BAML `test` block under crates/baml_tests/baml_src/ns_<topic>/ that runs the code and asserts the correct value or a catchable throw",
        );
    };
    if (inspection || routes.length() == 0) {
        routes.push(
            "a Rust test in the owning crate (use baml_tests::stdlib_prefix::{check_user_files, setup_test_db}, not baml_project::testing) — with a comment on why a BAML test cannot observe it",
        );
    };
    routes.join("; and ")
}

function run_agent_fix(sb: Sandbox, issue: Issue, timeout_s: int) -> FixReport? {
    let prompt = implement_fix$render_prompt(issue, sb.branch, test_route(issue)).text();
    let session = run_claude(sb.worktree, prompt, fix_tools(), timeout_s, sb.run_dir + "/transcript.jsonl");
    if (session.timed_out || session.exit_code != 0 || session.text.length() == 0) {
        log.warn(
            {
                "issue": issue.id,
                "agent": "stopped",
                "exit_code": session.exit_code,
                "timed_out": session.timed_out,
                "turns": session.num_turns,
            },
        );
        return null;
    };
    implement_fix$parse(session.text) catch (_) {
        _ => {
            log.warn(
                { "issue": issue.id, "agent": "unparseable report", "tail": tail(session.text, 5) },
            );
            null
        },
    }
}

// ---------- Hard path ----------

class DesignDoc {
    /// Concrete enough to hand to an agent as its task.
    suggested_fix: string,
    /// Markdown with: Problem, Root-cause hypothesis (with evidence),
    /// Proposed design, Alternatives considered, Test plan, Risks and
    /// rollout, Open questions for the shepherd.
    doc: string,
}

function write_design_doc(issue: Issue, branch: string) -> DesignDoc {
    client: Handle
    prompt: `
        ${sandbox_rules(branch)}
        This is a READ-ONLY investigation: do not change any file.

        This issue was rated Hard — design work, a cross-subsystem change, or an unknown root cause.
        Investigate the repository and write the suggested fix and design doc a maintainer would
        hand to an engineer (or their agent). Cite file paths and line ranges you actually read.

        Title: ${issue.title}
        Subsystem: ${issue.subsystem.to_string()}

        ${issue.description}

        Resolution plan from triage:
        ${issue.resolution_plan ?? "(none)"}

        Repros:
        ${issue.repros.map((r: Repro) -> string { format_repro(r) }).join("\n\n")}

        The doc must have these sections: Problem; Root-cause hypothesis (with the evidence);
        Proposed design; Alternatives considered; Test plan (which route per
        baml_language/TEST_INSTRUCTIONS.md, which repros); Risks and rollout; Open questions for the shepherd.

        ${ctx.output_format}
    `
}

function render_design_doc(d: DesignDoc) -> string {
    "## Suggested fix\n\n" + d.suggested_fix + "\n\n" + d.doc
}

function run_agent_design(sb: Sandbox, issue: Issue, timeout_s: int) -> DesignDoc? {
    let prompt = write_design_doc$render_prompt(issue, sb.branch).text();
    let session = run_claude(sb.worktree, prompt, design_tools(), timeout_s, sb.run_dir + "/transcript.jsonl");
    if (session.timed_out || session.exit_code != 0 || session.text.length() == 0) {
        log.warn(
            {
                "issue": issue.id,
                "agent": "stopped",
                "exit_code": session.exit_code,
                "timed_out": session.timed_out,
                "turns": session.num_turns,
            },
        );
        return null;
    };
    write_design_doc$parse(session.text) catch (_) {
        _ => {
            log.warn(
                {
                    "issue": issue.id,
                    "agent": "unparseable design doc",
                    "tail": tail(session.text, 5),
                },
            );
            null
        },
    }
}

// ==================== The gate ====================
// BAML runs it itself after the agent finishes; the agent's own claim is
// never trusted. Order and commands follow baml_language/TEST_INSTRUCTIONS.md.

class GateStep {
    name: string,
    ok: bool,
    seconds: int,
    tail: string,
}

class GateResult {
    steps: GateStep[],
    ok: bool,
    changed_crates: string[],
}

/// Crates touched vs origin/canary, from `crates/<name>/...` paths.
function changed_crates_from(diff_names: string) -> string[] {
    let out: string[] = [];
    for (let line in diff_names.lines()) {
        let l = line.trim();
        let marker = "baml_language/crates/";
        let i = l.index_of(marker);
        if (i != null) {
            let rest = l.slice((i ?? 0) + marker.length(), l.length());
            let name = rest.split("/").at(0) ?? "";
            if (name.length() > 0 && !out.includes(name)) {
                out.push(name);
            };
        };
    }
    out
}

function changed_crates(worktree: string) -> string[] {
    changed_crates_from(git(worktree, ["diff", "--name-only", "origin/canary...HEAD"]).stdout)
}

function gate_step(name: string, c: Cmd) -> GateStep {
    let started = baml.time.Instant.now();
    let r = run(c);
    let step = GateStep {
        name: name,
        ok: r.ok,
        seconds: elapsed_s(started),
        tail: tail(
            if (r.ok) {
                r.stdout
            } else {
                r.stderr + "\n" + r.stdout
            },
            15,
        ),
    };
    log.info({ "gate": name, "ok": r.ok, "seconds": step.seconds });
    step
}

function run_gate(worktree: string) -> GateResult {
    let wd = worktree + "/baml_language";
    let crates = changed_crates(worktree);
    let steps: GateStep[] = [];
    let cargo = (name: string, args: string[], timeout_ms: int) -> bool {
        let s = gate_step(name, Cmd { program: "cargo", args: args, cwd: wd, timeout_ms: timeout_ms });
        steps.push(s);
        s.ok
    };
    let ok = cargo(
        "fmt",
        [
            "fmt",
            "--all",
            "--check",
            "--",
            "--config",
            "imports_granularity=Crate",
            "--config",
            "group_imports=StdExternalCrate",
        ],
        300000,
    );
    if (ok) {
        ok
            = cargo(
                "clippy",
                ["clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"],
                2400000,
            );
    };
    for (let crate in crates) {
        if (ok && crate != "baml_tests") {
            ok = cargo("cargo test --lib -p " + crate, ["test", "--lib", "-p", crate], 1800000);
        };
    }
    if (ok) {
        ok = cargo("nextest baml_tests", ["nextest", "run", "-p", "baml_tests"], 1800000);
    };
    if (ok) {
        ok
            = cargo(
                "insta baml_tests",
                ["insta", "test", "--test-runner=nextest", "--accept", "-p", "baml_tests"],
                1800000,
            );
    };
    if (ok) {
        // --accept must not have produced unreviewed snapshot churn
        let status = git(worktree, ["status", "--porcelain"]);
        let clean = status.stdout.trim().length() == 0;
        steps.push(
            GateStep { name: "insta clean", ok: clean, seconds: 0, tail: tail(status.stdout, 15) },
        );
        ok = clean;
    };
    if (ok) {
        ok = cargo("build baml-cli", ["build", "-p", "baml_cli", "--bin", "baml-cli"], 1200000);
    };
    if (ok) {
        let s = gate_step(
            "baml-cli test baml_src",
            Cmd {
                program: target_dir() + "/debug/baml-cli",
                args: ["test", "--from", "crates/baml_tests/baml_src"],
                cwd: wd,
                timeout_ms: 1200000,
            },
        );
        steps.push(s);
        ok = s.ok;
    };
    GateResult { steps: steps, ok: ok, changed_crates: crates }
}

function gate_summary(g: GateResult) -> string {
    g.steps
        .map((s: GateStep) -> string {
            "`"
                + s.name
                + "` "
                + (
                    if (s.ok) {
                        "green"
                    } else {
                        "FAILED"
                    }
                )
                + " ("
                + s.seconds.to_string()
                + "s)"
        })
        .join(" · ")
}

// ==================== The PR ====================

class PrDraft {
    title: string,
    body: string,
}

function pr_title(issue: Issue) -> string {
    let scope = issue.subsystem.to_string().to_lower_case();
    let t = "fix(" + scope + "): " + issue.title;
    if (t.length() > 72) {
        t.slice(0, 72)
    } else {
        t
    }
}

function pr_body(issue: Issue, r: FixReport, g: GateResult, turns: int, seconds: int) -> string {
    let n = github_number(issue);
    let fixes = if (n != null) {
        "Fixes #" + (n ?? 0).to_string() + "\n\n"
    } else {
        ""
    };
    let budget = time_budget_s(issue.difficulty ?? Difficulty.Medium);
    fixes
        + r.summary
        + "\n\n## Design\n\n"
        + r.design
        + "\n\n## Metrics\n\n"
        + "| | |\n|---|---|\n"
        + "| difficulty | "
        + (issue.difficulty ?? Difficulty.Medium).to_string()
        + " |\n"
        + "| agent time | "
        + seconds.to_string()
        + "s of "
        + budget.to_string()
        + "s budget |\n"
        + "| agent turns | "
        + turns.to_string()
        + " |\n"
        + "| tests added | "
        + (
            if (r.tests_added.length() == 0) {
                "none"
            } else {
                r.tests_added.join(", ")
            }
        )
        + " |\n"
        + "| files changed | "
        + r.files_changed.length().to_string()
        + " |\n"
        + "\n## Gates\n\n"
        + gate_summary(g)
        + "\n\n_Opened by atb2 `handle_issue`; the gate above was re-run by the pipeline, not the agent._\n"
}

/// Live only. Pushes with the real push URL (the worktree's is disabled)
/// and opens a draft PR, or updates the existing PR for the branch.
function push_and_open_pr(sb: Sandbox, draft: PrDraft) -> string {
    let env = sandbox_env(true);
    let push = run_with(
        Cmd {
            program: "git",
            args: [
                "-c",
                "remote.origin.pushurl=" + repo_url(),
                "push",
                "--force-with-lease",
                "-u",
                "origin",
                sb.branch,
            ],
            cwd: sb.worktree,
            timeout_ms: 600000,
        },
        env,
    );
    if (!push.ok) {
        baml.sys.panic("push failed: " + tail(push.stderr, 5));
    };
    let body_path = sb.run_dir + "/pr_body.md";
    let existing = run_with(
        Cmd {
            program: "gh",
            args: ["pr", "view", sb.branch, "--repo", repo_slug(), "--json", "url", "--jq", ".url"],
            cwd: sb.worktree,
            timeout_ms: 120000,
        },
        env,
    );
    if (existing.ok && existing.stdout.trim().length() > 0) {
        run_with(
            Cmd {
                program: "gh",
                args: [
                    "pr",
                    "edit",
                    sb.branch,
                    "--repo",
                    repo_slug(),
                    "--title",
                    draft.title,
                    "--body-file",
                    body_path,
                ],
                cwd: sb.worktree,
                timeout_ms: 120000,
            },
            env,
        );
        return existing.stdout.trim();
    };
    let created = run_with(
        Cmd {
            program: "gh",
            args: [
                "pr",
                "create",
                "--repo",
                repo_slug(),
                "--draft",
                "--base",
                "canary",
                "--head",
                sb.branch,
                "--title",
                draft.title,
                "--body-file",
                body_path,
            ],
            cwd: sb.worktree,
            timeout_ms: 120000,
        },
        env,
    );
    if (!created.ok) {
        baml.sys.panic("gh pr create failed: " + tail(created.stderr, 5));
    };
    created.stdout.trim()
}

// ==================== Outcome (audit trail + what the evals read) ====================

class HandleOutcome {
    kind: "fixed" | "hard" | "gate_failed" | "agent_stopped",
    branch: string?,
    pr: string?,
    turns: int,
    seconds: int,
    timed_out: bool,
    gate: GateResult?,
    report: FixReport?,
    design_doc: string?,
}

function stopped_outcome(sb: Sandbox, started: baml.time.Instant) -> HandleOutcome {
    let path = sb.run_dir + "/transcript.jsonl";
    HandleOutcome {
        kind: "agent_stopped",
        branch: sb.branch,
        pr: null,
        turns: read_turns(sb),
        seconds: elapsed_s(started),
        timed_out: baml.fs.exists(path) && !baml.fs.read(path).includes("\"type\":\"result\""),
        gate: null,
        report: null,
        design_doc: null,
    }
}

function write_outcome(sb: Sandbox, o: HandleOutcome) -> null {
    baml.fs.write(sb.run_dir + "/outcome.json", baml.json.stringify_pretty(o.to_json()));
    log.info(
        {
            "issue_branch": sb.branch,
            "kind": o.kind,
            "turns": o.turns,
            "seconds": o.seconds,
            "timed_out": o.timed_out,
        },
    );
    null
}

function read_outcome(issue: Issue) -> HandleOutcome? {
    let path = run_dir_for(branch_for(issue)) + "/outcome.json";
    if (!baml.fs.exists(path)) {
        return null;
    };
    baml.json.from_string<HandleOutcome>(baml.fs.read(path))
}

// ==================== Eval ====================
//
//   fix_in_budget  Easy/Medium reference issues, DryRun: the agent must land
//                  a fix that passes the gate inside its time budget.
//   design_doc     Hard reference issues: the design doc must match the
//                  reference (LLM-judged similarity >= 0.7).
//
// Both are hours of wall-clock and are opt-in:
//   ATB2_ISSUES=4587 tools/atb2/setup_database.sh -- baml test -i 'root::fix_in_budget::*'

function judge_design_doc(ours: string, reference: string) -> IssueJudgment {
    client: Triage
    prompt: `
        Two design docs for the same BAML toolchain bug, each a suggested fix plus a
        write-up. REFERENCE is the maintainers'; OURS was produced by an agent from the
        ticket. Score how well OURS stands in for REFERENCE: same root cause, same
        proposed change and files, same test plan. Wording and length do not matter.
        1.0 = interchangeable; 0.7 = same plan, minor gaps; 0.4 = related but a shepherd
        would rewrite it; 0.0 = wrong root cause.

        REFERENCE:
        ${reference}

        OURS:
        ${ours}

        ${ctx.output_format}
    `
}

/// ATB2_ISSUES="4587,4335" limits the eval to those issues; unset = all.
function handle_issues_selected() -> string[]? {
    let raw = baml.env.get("ATB2_ISSUES");
    if (raw == null) {
        return null;
    };
    (raw ?? "").split(",")
        .map((s: string) -> string {
            s.trim()
        })
        .filter((s: string) -> bool {
            s.length() > 0
        })
}

function selected(only: string[]?, key: string) -> bool {
    only == null || (only ?? []).includes(key)
}

testset "fix_in_budget" with testing.PassRate(0.6) {
    if (supabase_configured()) {
        ensure_cache();
        let issues = load_issues();
        let only = handle_issues_selected();
        for (let key in issues.keys()) {
            let row = issues.get(key) ?? baml.sys.panic("unreachable");
            if (row.time_budget_s != null && selected(only, key)) {
                test ("#" + key) {
                    // everything derived from `key` in here (loop-capture caveat)
                    let issue = load_issues().get(key) ?? baml.sys.panic("no triage_issues row for #" + key);
                    let r = issue.reference_issue;
                    let target = with_difficulty(reference_as_issue(issue.number, r), r.difficulty);
                    handle_issue(target, mode = HandleMode.DryRun);
                    let out = read_outcome(target) ?? baml.sys.panic("no outcome for #" + key);
                    log.info(
                        {
                            "issue": key,
                            "kind": out.kind,
                            "seconds": out.seconds,
                            "budget": issue.time_budget_s,
                            "turns": out.turns,
                            "gate": match (out.gate) {
                                null => "not run",
                                let g: GateResult => gate_summary(g),
                            },
                        },
                    );
                    assert.equal(out.kind, "fixed");
                }
            };
        }
    };
}

testset "design_doc" with testing.PassRate(0.5) {
    if (supabase_configured()) {
        ensure_cache();
        let issues = load_issues();
        let only = handle_issues_selected();
        for (let key in issues.keys()) {
            let row = issues.get(key) ?? baml.sys.panic("unreachable");
            if (row.design_doc != null && selected(only, key)) {
                test ("#" + key) {
                    let issue = load_issues().get(key) ?? baml.sys.panic("no triage_issues row for #" + key);
                    let r = issue.reference_issue;
                    let target = with_difficulty(reference_as_issue(issue.number, r), Difficulty.Hard);
                    let handled = handle_issue(target, mode = HandleMode.DryRun);
                    let ours = handled.design_doc ?? baml.sys.panic("no design doc produced for #" + key);
                    let j = judge_design_doc(ours, issue.design_doc ?? "");
                    log.info({ "issue": key, "similarity": j.similarity, "missing": j.missing });
                    assert.is_true(j.similarity >= 0.7);
                }
            };
        }
    };
}

// ==================== Unit tests (token-free) ====================

function sample_stream() -> string {
    "{\"type\":\"system\",\"subtype\":\"init\"}\n"
        + "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"looking\"}]}}\n"
        + "not json at all\n"
        + "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"Bash\"}]}}\n"
        + "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\"}]}}\n"
        + "{\"type\":\"result\",\"subtype\":\"success\",\"num_turns\":7,\"result\":\"{\\\"summary\\\":\\\"done\\\"}\"}\n"
}

function gh_issue(number: int, difficulty: Difficulty?) -> Issue {
    Issue {
        id: "GH-" + number.to_string(),
        title: "0.17.0: `throws` clause with an unresolved type panics the compiler (index out of bounds)",
        description: "…",
        shepherd: null,
        subsystem: Subsystem.Compiler,
        repros: [],
        version: "0.17.0",
        feedback_ids: [],
        status: Open { state: "open" },
        comments: [],
        resolution_plan: null,
        difficulty: difficulty,
        design_doc: null,
    }
}

function sample_gate() -> GateResult {
    GateResult {
        steps: [
            GateStep { name: "fmt", ok: true, seconds: 3, tail: "" },
            GateStep { name: "clippy", ok: true, seconds: 400, tail: "" },
        ],
        ok: true,
        changed_crates: ["baml_compiler2_hir"],
    }
}

function sample_report() -> FixReport {
    FixReport {
        summary: "Guarded the type-ref lookup.",
        design: "Bounds check in `type_ref.rs:346`.",
        tests_added: ["crates/baml_tests/projects/diagnostic_errors/unresolved_throws/main.baml"],
        files_changed: ["crates/baml_compiler2_hir/src/type_ref.rs"],
        gate_passed: true,
        blockers: null,
    }
}

testset "handle_issue" {
    test "branch names are agent/gh-<n>-<slug>" {
        assert.equal(
            branch_for(gh_issue(4587, Difficulty.Easy)),
            "agent/gh-4587-0-17-0-throws-clause-with-an-unresolved",
        );
        assert.equal(github_number(gh_issue(4587, null)) ?? -1, 4587);
    }

    test "pipeline ids get a short slug and no github number" {
        let i = with_status(gh_issue(1, Difficulty.Easy), Open { state: "open" });
        let piped = Issue {
            id: "ISSUE-01j9x7abcdefg",
            title: i.title,
            description: i.description,
            shepherd: null,
            subsystem: i.subsystem,
            repros: [],
            version: i.version,
            feedback_ids: [],
            status: Open { state: "open" },
            comments: [],
            resolution_plan: null,
            difficulty: Difficulty.Easy,
            design_doc: null,
        };
        assert.is_true(branch_for(piped).starts_with("agent/issue-01j9x7ab-"));
        assert.equal(github_number(piped) == null, true);
    }

    test "slug collapses and trims" {
        assert.equal(slug("  Hello, World!! --x  "), "hello-world-x");
        assert.equal(slug("abcdefghij", max = 5), "abcde");
        assert.equal(slug("ab-cdefghij", max = 3), "ab");
    }

    test "worktree and run dirs never contain a slash from the branch" {
        assert.is_true(worktree_for("agent/gh-1-x").ends_with("/worktrees/agent__gh-1-x"));
        assert.is_true(run_dir_for("agent/gh-1-x").ends_with("/runs/agent__gh-1-x"));
    }

    test "parse_stream reads the result event" {
        let r = parse_stream(sample_stream());
        assert.equal(r.num_turns, 7);
        assert.equal(r.text, "{\"summary\":\"done\"}");
    }

    test "parse_stream without a result event counts assistant messages" {
        let cut = sample_stream().lines().slice(0, 5).join("\n");
        let r = parse_stream(cut);
        assert.equal(r.num_turns, 2);
        assert.equal(r.text, "");
        assert.equal(parse_stream("").num_turns, 0);
    }

    test "the agent env is an allowlist" {
        let env = sandbox_env(false);
        assert.is_true(env.has("PATH") && env.has("HOME"));
        assert.equal(env.get("CARGO_INCREMENTAL") ?? "", "0");
        assert.is_true(
            !env.has("GH_TOKEN")
                && !env.has("FEEDBACK_SUPABASE_KEY")
                && !env.has("ANTHROPIC_API_KEY"),
        );
    }

    test "claude argv mirrors the client's flags plus --tools" {
        let a = claude_args("hi", fix_tools());
        assert.equal(a.at(0) ?? "", "-p");
        assert.equal(a.at(1) ?? "", "hi");
        assert.is_true(
            a.includes("--no-session-persistence")
                && a.includes("stream-json")
                && a.includes("--tools"),
        );
        assert.equal(a.at(a.length() - 1) ?? "", "Read,Edit,Write,Glob,Grep,Bash");
    }

    test "time budgets" {
        assert.equal(time_budget_s(Difficulty.Easy), 1800);
        assert.equal(time_budget_s(Difficulty.Medium), 3600);
    }

    test "changed crates come from crates/<name>/ paths" {
        let names = "baml_language/crates/baml_compiler2_hir/src/type_ref.rs\nbaml_language/crates/baml_tests/projects/x/main.baml\nbaml_language/crates/baml_compiler2_hir/src/lib.rs\nREADME.md\n";
        assert.equal(changed_crates_from(names), ["baml_compiler2_hir", "baml_tests"]);
    }

    test "test_route follows the repro expectations" {
        let base = gh_issue(1, Difficulty.Easy);
        let with_repro = Issue {
            id: base.id,
            title: base.title,
            description: base.description,
            shepherd: null,
            subsystem: base.subsystem,
            repros: [
                Repro {
                    files: { "main.baml": "function f() -> int { \"x\" }" },
                    command: "baml check",
                    setup: null,
                    expectation: ShouldNotCompile { check: "should_not_compile", diagnostic_contains: null },
                },
            ],
            version: base.version,
            feedback_ids: [],
            status: Open { state: "open" },
            comments: [],
            resolution_plan: null,
            difficulty: Difficulty.Easy,
            design_doc: null,
        };
        assert.contains(test_route(with_repro), "diagnostic_errors");
        assert.contains(test_route(base), "Rust test");
    }

    test "pr title and body" {
        let issue = gh_issue(4587, Difficulty.Easy);
        assert.is_true(pr_title(issue).starts_with("fix(compiler): "));
        assert.is_true(pr_title(issue).length() <= 72);
        let body = pr_body(issue, sample_report(), sample_gate(), 12, 640);
        assert.is_true(body.starts_with("Fixes #4587"));
        assert.contains(body, "## Design");
        assert.contains(body, "## Metrics");
        assert.contains(body, "| agent turns | 12 |");
        assert.contains(body, "640s of 1800s budget");
        assert.contains(body, "## Gates");
        assert.contains(body, "`clippy` green (400s)");
    }

    test "handle_ready reasons" {
        assert.equal(handle_ready(gh_issue(1, null), false) ?? "go", "not gauged");
        assert.equal(handle_ready(gh_issue(1, Difficulty.Easy), false) ?? "go", "go");
        let taken = with_status(gh_issue(1, Difficulty.Easy), InProgress { state: "in_progress", pr: null });
        assert.equal(handle_ready(taken, false) ?? "go", "not open");
        assert.equal(handle_ready(taken, true) ?? "go", "go");
        assert.equal(
            handle_ready(with_design_doc(gh_issue(1, Difficulty.Hard), "doc"), false) ?? "go",
            "already has a design doc",
        );
    }

    test "with_status / with_design_doc change nothing else" {
        let i = gh_issue(7, Difficulty.Medium);
        let s = with_status(i, InProgress { state: "in_progress", pr: "https://x/pr/1" });
        assert.equal(s.id, i.id);
        assert.equal(s.difficulty ?? Difficulty.Hard, Difficulty.Medium);
        let d = with_design_doc(i, "doc");
        assert.equal(d.design_doc ?? "", "doc");
        assert.equal(d.title, i.title);
    }

    test "llm replies parse" {
        let r = implement_fix$parse(
            #"{ "summary": "s", "design": "d", "tests_added": ["a.baml"], "files_changed": ["x.rs"], "gate_passed": true, "blockers": null }"#,
        );
        assert.equal(r.tests_added.length(), 1);
        let d = write_design_doc$parse(#"{ "suggested_fix": "f", "doc": "## Problem\nx" }"#);
        assert.contains(render_design_doc(d), "## Suggested fix");
        let j = judge_design_doc$parse(#"{ "similarity": 0.7, "missing": null }"#);
        assert.equal(j.similarity, 0.7);
    }
}
