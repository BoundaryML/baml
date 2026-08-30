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
//#handle_issue(issue: Issue, mode: HandleMode = HandleMode.Live, update: bool = false) -> Issue
function handle_issue(
    issue: Issue,
    mode: HandleMode = HandleMode.Live,
    update: bool = false,
) -> Issue {
    //# skip: not ready (not gauged / not open / already handled)
    let skip = handle_ready(issue, update);
    if (skip != null) {
        log.info({ "issue": issue.id, "skipped": skip });
        return issue;
    };
    let difficulty = issue.difficulty ?? baml.sys.panic("handle_issue: gauge_issue first (" + issue.id + ")");
    //# is it still broken on canary HEAD?
    // canary is fetched and its baml-cli rebuilt (seconds when warm) so the
    // repros are checked against what a PR would actually land on
    ensure_cache();
    build_canary_cli();
    if (!still_broken(issue)) {
        //## already fixed: nothing to do
        log.info({ "issue": issue.id, "kind": "already_fixed", "checked_with": check_cli() });
        return issue;
    };
    //# open a worktree on a fresh branch
    let sb = open_sandbox(issue, update);
    let keep_branch = false;
    defer {
        //## always: tear the sandbox down
        close_sandbox(sb, keep_branch);
    }
    let started = baml.time.Instant.now();
    //# design pass: investigate, propose the fix, write the plan (read-only)
    let design = run_agent_design(sb, issue, design_budget_s(difficulty));
    if (design == null) {
        //## stop: the design agent produced nothing
        write_outcome(sb, stopped_outcome(sb, started));
        return issue;
    };
    let doc = render_design_doc(design ?? baml.sys.panic("unreachable"));
    baml.fs.write(sb.run_dir + "/design.md", doc);
    if (difficulty == Difficulty.Hard) {
        //## hard: hand the design doc to the shepherd, no code change
        write_outcome(sb, outcome(sb, "hard", null, started, null, null, doc));
        return with_design_doc(issue, doc);
    };
    //# fix pass: implement the plan, encode the repros as tests
    warm_build(sb);
    let report = run_agent_fix(sb, issue, doc, time_budget_s(difficulty));
    if (report == null) {
        //## stop: the fix agent ran out of budget or reported nothing
        keep_branch = mode == HandleMode.Live;
        write_outcome(sb, stopped_outcome(sb, started));
        return with_design_doc(issue, doc);
    };
    let fix = report ?? baml.sys.panic("unreachable");
    //# re-run the gate ourselves (the agent's own claim is never trusted)
    checkpoint_uncommitted(sb);
    let gate = run_gate(sb.worktree);
    if (!gate.ok) {
        //## stop: gate failed — branch kept for a human in Live mode
        keep_branch = mode == HandleMode.Live;
        write_outcome(sb, outcome(sb, "gate_failed", null, started, gate, fix, doc));
        return with_design_doc(issue, doc);
    };
    //# write the PR body
    let draft = PrDraft {
        title: pr_title(issue),
        body: pr_body(issue, fix, gate, read_turns(sb), elapsed_s(started)),
    };
    baml.fs.write(sb.run_dir + "/pr_body.md", draft.body);
    if (mode == HandleMode.DryRun) {
        //## dry run: stop before the push (what the evals run)
        write_outcome(sb, outcome(sb, "fixed", null, started, gate, fix, doc));
        return with_design_doc(issue, doc);
    };
    //# push the branch and open the draft PR
    let pr = push_and_open_pr(sb, draft);
    write_outcome(sb, outcome(sb, "fixed", pr, started, gate, fix, doc));
    log.info(
        { "issue": issue.id, "pr": pr, "turns": read_turns(sb), "seconds": elapsed_s(started) },
    );
    with_status(with_design_doc(issue, doc), InProgress { state: "in_progress", pr: pr })
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

/// True unless EVERY repro comes back Fixed on CANARY. Inconclusive (needs
/// a human, or a command that cannot be checked) counts as still broken —
/// the agent gets to look.
///
/// The check runs the cache's freshly built canary `baml-cli` out of
/// process (falling back to the installed toolchain before the cache
/// exists): a repro that crashes the compiler (#4468, #4587 ...) would
/// take this process down, and a fix already merged to canary would be
/// invisible to the installed release.
function still_broken(issue: Issue) -> bool {
    if (issue.repros.length() == 0) {
        return true;
    };
    let i = 0;
    for (let r in issue.repros) {
        i += 1;
        if (check_on_canary(r, i) != Verdict.Fixed) {
            return true;
        };
    }
    false
}

/// The `baml-cli` to check repros with: canary's own build when the cache
/// has been built, else whatever `baml` resolves to.
function check_cli() -> string {
    let built = target_dir() + "/debug/baml-cli";
    if (baml.fs.exists(built)) {
        built
    } else {
        "baml"
    }
}

/// Write the repro to a scratch project and run it with check_cli().
function check_on_canary(r: Repro, n: int) -> Verdict {
    //# write the repro to a scratch project
    let dir = atb2_home() + "/repro-check/" + baml.id.new();
    baml.fs.mkdir(dir + "/baml_src", baml.fs.MkdirOptions { recursive: true });
    defer {
        baml.fs.remove_dir_all(dir) catch (_) {
            _ => null,
        };
    }
    if (!r.files.has("baml.toml")) {
        baml.fs.write(dir + "/baml.toml", "[package]\nname = \"repro" + n.to_string() + "\"\n");
    };
    let prefixed = r.files.keys()
        .filter((k: string) -> bool {
            k.starts_with("baml_src/")
        })
        .length()
        > 0;
    for (let name in r.files.keys()) {
        let path = dir + "/" + repro_file_path(name, prefixed);
        let parent = path.slice(0, path.last_index_of("/") ?? 0);
        baml.fs.mkdir(parent, baml.fs.MkdirOptions { recursive: true });
        baml.fs.write(path, r.files.get(name) ?? "");
    }
    let env = sandbox_env(false);
    let v = baml.env.get("BAML_VERSION");
    if (v != null) {
        env.set("BAML_VERSION", v ?? "");
    };
    //# run it with canary's baml-cli (check, or run for should_evaluate_to)
    let evaluate = match (r.expectation) {
        let e: ShouldEvaluateTo => true,
        _ => false,
    };
    let args = if (evaluate) {
        ["run", "-e", "repro_main()"]
    } else {
        ["check"]
    };
    let out = run_with(Cmd { program: check_cli(), args: args, cwd: dir, timeout_ms: 180000 }, env);
    //# map exit code + diagnostics to a verdict
    let verdict = verdict_from_check(r.expectation, out.exit_code, out.stdout, out.stderr);
    log.info(
        {
            "repro_check": n,
            "cli": check_cli(),
            "exit_code": out.exit_code,
            "verdict": verdict.to_string(),
        },
    );
    verdict
}

/// Where a repro file goes in the scratch project: repros name files
/// either bare ("main.baml") or already under "baml_src/"; sources land in
/// baml_src/ either way, and baml.toml stays at the root.
/// Collapse a dataset/LLM-supplied path to a safe repo-relative one: drop
/// empty, "." and ".." segments and any leading "/", so it can never escape
/// the directory it is joined onto (check_on_canary writes <dir>/<name>).
function sanitize_rel_path(p: string) -> string {
    p.split("/")
        .filter((s: string) -> bool {
            s != "" && s != "." && s != ".."
        })
        .join("/")
}

function repro_file_path(name: string, any_prefixed: bool) -> string {
    let safe = sanitize_rel_path(name);
    if (safe == "baml.toml" || safe.starts_with("baml_src/") || any_prefixed) {
        safe
    } else {
        "baml_src/" + safe
    }
}

/// Map a subprocess check to a verdict (pure, unit-tested). A crash —
/// signal exit, timeout, panic text — is StillBroken for every expectation.
function verdict_from_check(
    expectation: Expectation,
    exit_code: int,
    stdout: string,
    stderr: string,
) -> Verdict {
    let crashed = exit_code >= 128
        || exit_code == 124
        || stderr.includes("panicked at")
        || stderr.includes("internal error");
    if (crashed) {
        return Verdict.StillBroken;
    };
    match (expectation) {
        let e: RequiresInspection => Verdict.Inconclusive,
        let e: ShouldCompile => {
            if (exit_code == 0) {
                Verdict.Fixed
            } else {
                Verdict.StillBroken
            }
        },
        let e: ShouldNotCompile => {
            if (exit_code == 0) {
                Verdict.StillBroken
            } else {
                let wanted = e.diagnostic_contains;
                if (wanted != null && !(stderr + stdout).includes(wanted ?? "")) {
                    Verdict.Inconclusive
                } else {
                    Verdict.Fixed
                }
            }
        },
        let e: ShouldEvaluateTo => {
            if (exit_code != 0) {
                Verdict.Inconclusive
            } else {
                let expected = baml.json.stringify(e.expected);
                let got = stdout.trim();
                if (got == expected || got.includes(expected)) {
                    Verdict.Fixed
                } else {
                    Verdict.StillBroken
                }
            }
        },
    }
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

/// Seconds the design pass gets (read-only investigation + plan).
function design_budget_s(d: Difficulty) -> int {
    match (d) {
        Difficulty.Hard => 1800,
        _ => 900,
    }
}

/// Seconds the fix agent gets. Also the process timeout: the CLI is killed
/// at the budget, which is what "did not finish in budget" means in the eval.
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
    // USER + TMPDIR: the Claude Code CLI finds its keychain login through them
    for (let k in ["USER", "LOGNAME", "TMPDIR", "CARGO_HOME", "RUSTUP_HOME", "RUSTUP_TOOLCHAIN"]) {
        let v = baml.env.get(k);
        if (v != null) {
            env.set(k, v ?? "");
        };
    }
    if (!with_gh) {
        // the agent has arbitrary Bash; keep it away from the host's git
        // credentials (osxkeychain holds a live github token) and from any
        // hook planted in the shared .git dir. SSH_AUTH_SOCK is withheld for
        // the same reason (ssh-based push / remote auth).
        env.set("GIT_CONFIG_GLOBAL", "/dev/null");
        env.set("GIT_CONFIG_SYSTEM", "/dev/null");
        env.set("GIT_CONFIG_NOSYSTEM", "1");
        env.set("GIT_TERMINAL_PROMPT", "0");
    }
    if (with_gh) {
        let tok = baml.env.get("GH_TOKEN") ?? baml.env.get("GITHUB_TOKEN");
        if (tok != null) {
            env.set("GH_TOKEN", tok ?? "");
        };
    }
    env
}

/// Runs every sandbox process under `caffeinate -i` on macOS: a laptop that
/// drifts into maintenance sleep mid-run loses the network, and the agent
/// dies with "Can't reach the API server" hours later (seen twice on #4506).
function keep_awake(program: string, args: string[]) -> Cmd {
    if (baml.fs.exists("/usr/bin/caffeinate")) {
        return Cmd {
            program: "/usr/bin/caffeinate",
            args: ["-i", program].concat(args),
            cwd: "",
            timeout_ms: 0,
        };
    };
    Cmd { program: program, args: args, cwd: "", timeout_ms: 0 }
}

function run_with(c: Cmd, env: map<string, string>) -> CmdResult {
    let label = c.program + " " + c.args.join(" ");
    let awake = keep_awake(c.program, c.args);
    let out = baml.sys.exec(
        awake.program,
        awake.args,
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
    //# get the repo
    let repo = cache_repo();
    //# mkdir the atb2 home
    baml.fs.mkdir(atb2_home(), baml.fs.MkdirOptions { recursive: true });
    //# mkdir the target dir
    baml.fs.mkdir(target_dir(), baml.fs.MkdirOptions { recursive: true });
    //# if the repo does not exist
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
    };
    //# worktree config must be on or the --worktree pushurl guard is a no-op
    git(repo, ["config", "extensions.worktreeConfig", "true"]);
    //# fetch canary (retried: parallel tests share the cache)
    let tries = 0;
    //# while the tries are less than 3
    while (tries < 3) {
        tries += 1;
        //# fetch the origin canary
        let f = git(repo, ["fetch", "--prune", "origin", "canary"]);
        //# if the fetch is ok, return null
        if (f.ok) {
            //## the cache checkout tracks canary HEAD, so build_canary_cli builds it
            git(repo, ["checkout", "-q", "canary"]);
            git(repo, ["reset", "-q", "--hard", "origin/canary"]);
            return null;
        };
        //# sleep for 5 seconds
        baml.sys.sleep(baml.time.Duration.from_seconds(5)) catch (_) {
            _ => null,
        };
    }
    //# panic if the git fetch failed three times
    baml.sys.panic("git fetch failed three times")
}

function branch_exists_locally(branch: string) -> bool {
    git(cache_repo(), ["rev-parse", "--verify", "--quiet", "refs/heads/" + branch]).ok
}

function branch_exists_on_origin(branch: string) -> bool {
    git(cache_repo(), ["ls-remote", "--exit-code", "--heads", "origin", branch]).ok
}

function open_sandbox(issue: Issue, update: bool) -> Sandbox {
    //# get the repo
    let repo = cache_repo();
    //# get the branch
    let branch = branch_for(issue);
    //# get the worktree
    let wt = worktree_for(branch);
    //# get the run dir
    let rd = run_dir_for(branch);
    //# if the worktree exists
    if (baml.fs.exists(wt)) {
        //# remove the worktree
        git(repo, ["worktree", "remove", "--force", wt]);
        //# if the worktree still exists, remove the dir all
        if (baml.fs.exists(wt)) {
            baml.fs.remove_dir_all(wt);
        };
    };
    //# prune the worktree
    git(repo, ["worktree", "prune"]);
    //# remove the run dir
    baml.fs.remove_dir_all(rd);
    baml.fs.mkdir(rd, baml.fs.MkdirOptions { recursive: true });
    //# mkdir the worktrees dir
    baml.fs.mkdir(atb2_home() + "/worktrees", baml.fs.MkdirOptions { recursive: true });
    //# set reused to false
    let reused = false;
    //# if the branch exists locally and update is true
    let add = if (branch_exists_locally(branch) && update) {
        //# set reused to true
        reused = true;
        //# add the worktree
        git(repo, ["worktree", "add", wt, branch])
    } else {
        //# if the branch exists locally
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
    //# if the add is not ok, panic
    if (!add.ok) {
        baml.sys.panic("worktree add failed: " + tail(add.stderr, 5));
    };
    //# config the worktree
    git(wt, ["config", "--worktree", "remote.origin.pushurl", "no_push://atb2"]);
    //# config the user name
    git(wt, ["config", "--worktree", "user.name", "atb2"]);
    git(wt, ["config", "--worktree", "user.email", "atb2@boundaryml.com"]);

    log.info({ "sandbox": wt, "branch": branch, "reused_branch": reused });
    Sandbox { branch: branch, worktree: wt, run_dir: rd, reused_branch: reused }
}

/// Runs from `defer`: never throws. Removes the worktree (and the local
/// branch unless kept for a human), then checks the cache is untouched.
function close_sandbox(sb: Sandbox, keep_branch: bool) -> null {
    let repo = cache_repo();
    //# remove the worktree
    let removed = git(repo, ["worktree", "remove", "--force", sb.worktree]);
    git(repo, ["worktree", "prune"]);
    if (baml.fs.exists(sb.worktree)) {
        // git refused (or the tree was already unregistered): delete it outright
        let rm = run(Cmd { program: "rm", args: ["-rf", sb.worktree], cwd: atb2_home(), timeout_ms: 600000 });
        if (baml.fs.exists(sb.worktree)) {
            log.warn(
                {
                    "sandbox": sb.worktree,
                    "warning": "worktree not removed",
                    "git": tail(removed.stderr, 2),
                    "rm": tail(rm.stderr, 2),
                },
            );
        };
    };
    //# delete the local branch unless a human should pick it up
    if (!keep_branch) {
        git(repo, ["branch", "-D", sb.branch]);
    };
    //# keep the run dir as the audit trail (ATB2_KEEP_RUNS=0 deletes it)
    if ((baml.env.get("ATB2_KEEP_RUNS") ?? "1") == "0") {
        baml.fs.remove_dir_all(sb.run_dir) catch (_) {
            _ => null,
        };
    };
    //# verify the cache repo is untouched
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

/// canary's own baml-cli, rebuilt from the cache checkout after each fetch,
/// so the repro pre-check and the gate both speak for canary HEAD. Seconds
/// when the shared target is warm.
function build_canary_cli() -> null {
    let b = run(
        Cmd {
            program: "cargo",
            args: ["build", "-p", "baml_cli", "--bin", "baml-cli"],
            cwd: cache_repo() + "/baml_language",
            timeout_ms: 2400000,
        },
    );
    if (!b.ok) {
        log.warn(
            {
                "build_canary_cli": "failed; the repro check falls back to the installed toolchain",
                "tail": tail(b.stderr, 3),
            },
        );
    };
    null
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
        // bypassPermissions: in -p mode a "requires approval" Bash call just
        // fails, and the agent needs cargo. The worktree is disposable and the
        // env is an allowlist, so this is the sandbox, not the prompt.
        "--permission-mode",
        "bypassPermissions",
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
    //# the CLI writes its stream straight to the transcript file
    // The CLI writes its stream straight to the transcript file, so the
    // record survives when the process is killed at the budget (exec
    // returns nothing on a timeout).
    let env = sandbox_env(false);
    env.set("ATB2_TRANSCRIPT", transcript_path);
    let r = run_with(
        Cmd {
            program: "sh",
            args: ["-c", "exec \"$@\" > \"$ATB2_TRANSCRIPT\" 2>&1", "atb2", "claude"].concat(
                claude_args(prompt, tools),
            ),
            cwd: cwd,
            timeout_ms: timeout_s * 1000,
        },
        env,
    );
    //# read the result event back: reply text + turn count
    let transcript = if (baml.fs.exists(transcript_path)) {
        baml.fs.read(transcript_path)
    } else {
        ""
    };
    let parsed = parse_stream(transcript);
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

/// Turns across both passes (design.jsonl + fix.jsonl).
function read_turns(sb: Sandbox) -> int {
    let total = 0;
    for (let name in ["design.jsonl", "fix.jsonl"]) {
        let path = sb.run_dir + "/" + name;
        if (baml.fs.exists(path)) {
            total += parse_stream(baml.fs.read(path)).num_turns;
        };
    }
    total
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

/// One row of the PR's "Behavioral boundaries" table.
class Boundary {
    scenario: string,
    result: string,
    why: string,
}

/// The agent's account of the fix, in the shape of the team's PRs
/// (see BoundaryML/baml#4621): Problem / After this PR / Behavioral
/// boundaries / where the tests live / what was validated.
class FixReport {
    /// 2-4 sentences: the root cause and the change made.
    summary: string,
    /// Markdown for "## Problem": what BAML did wrong, with the minimal
    /// program that shows it in a fenced ```baml block, and why it was wrong.
    problem: string,
    /// Markdown for "## After this PR": the new behavior, quoting the exact
    /// diagnostic or value the same program now produces, and the fix in
    /// one or two paragraphs citing the files touched.
    after: string,
    /// The "Behavioral boundaries" table: what compiles / is rejected /
    /// evaluates to what, and why, for the cases around the fix. 3-8 rows.
    boundaries: Boundary[],
    /// Markdown for the tests section: where the new tests live (paths),
    /// what each asserts, and any snapshots refreshed.
    tests: string,
    /// Paths of the tests that encode the issue's repro(s).
    tests_added: string[],
    files_changed: string[],
    /// What the agent believes about the gate; BAML re-runs it regardless.
    gate_passed: bool,
    /// Why it stopped short, if it did.
    blockers: string?,
}

function implement_fix(
    issue: Issue,
    branch: string,
    test_route: string,
    plan: string,
) -> FixReport {
    client: Handle
    prompt: `
        ${sandbox_rules(branch)}

        Fix this issue in the BAML toolchain. A read-only design pass has already investigated
        it and written the plan below; follow the plan unless the code proves it wrong, and if
        you deviate, say so in \`summary\`.

        Title: ${issue.title}
        Subsystem: ${issue.subsystem.to_string()}

        ${issue.description}

        Resolution plan from triage (a starting point, not gospel):
        ${issue.resolution_plan ?? "(none)"}

        Repros (${issue.repros.length().to_string()}):
        ${issue.repros.map((r: Repro) -> string { format_repro(r) }).join("\n\n")}

        The plan from the design pass:
        ${plan}

        How to work:
        1. Reproduce FIRST. Turn every repro above into a test before touching the fix, placed
           per baml_language/TEST_INSTRUCTIONS.md. For this issue: ${test_route}
           Confirm the new test fails before the fix and passes after it.
        2. Make the smallest correct fix. Cite the files you changed.
        3. Verify with FAST, targeted checks only — your time budget is short and the full gate
           (clippy, nextest, insta, the baml-cli corpus) is run by the pipeline after you finish:
             cargo fmt --all -- --config imports_granularity=Crate --config group_imports=StdExternalCrate
             cargo test -p <each crate you changed> <your new test name>      (the repro test: fails before, passes after)
             cargo test --lib -p <each crate you changed>
           If you added a diagnostic fixture or changed diagnostics, also: cargo insta test --test-runner=nextest --accept -p baml_tests
           and commit the .snap changes. Do not run the whole workspace's clippy or nextest.
        4. Commit everything. \`git status --porcelain\` must be empty. Do not push.
        5. Report in the shape of this team's PRs: \`problem\` (with the repro in a fenced baml code block),
           \`after\` (quote the exact diagnostic/value the repro now produces; cite files), a
           \`boundaries\` table of the cases around the fix (compiles / rejected / value, and why),
           and \`tests\` (paths, what each asserts, snapshots refreshed). Declarative, no hedging.

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

function run_agent_fix(sb: Sandbox, issue: Issue, plan: string, timeout_s: int) -> FixReport? {
    //# render the prompt: rules + ticket + repros + the design pass's plan
    let prompt = implement_fix$render_prompt(issue, sb.branch, test_route(issue), plan).text();
    //# one Claude Code session with its own tools, killed at the budget
    let session = run_claude(sb.worktree, prompt, fix_tools(), timeout_s, sb.run_dir + "/fix.jsonl");
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

        Investigate the repository and write the suggested fix and the plan a maintainer would
        hand to an engineer (or their agent): where the defect lives, what to change, which tests
        to add and where. Cite file paths and line ranges you actually read. If the issue turns
        out to be already fixed on this branch, say so plainly and make the plan "add the missing
        regression test".

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
    //# render the prompt: rules + ticket + repros, read-only
    let prompt = write_design_doc$render_prompt(issue, sb.branch).text();
    //# one Claude Code session, read-only tools, killed at the budget
    let session = run_claude(sb.worktree, prompt, design_tools(), timeout_s, sb.run_dir + "/design.jsonl");
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
    /// The process exit code; used to tell "ran, some tests failed" (nextest
    /// 100) apart from "did not complete" (compile error / timeout 124), so a
    /// non-completed step cannot be waved through on test-controllable output.
    exit_code: int,
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

/// Tests known to fail on canary itself; a gate failure that is only these
/// is not the agent's. ATB2_FLAKY_TESTS overrides (comma-separated names).
function flaky_tests() -> string[] {
    (baml.env.get("ATB2_FLAKY_TESTS") ?? "claude_code_client_preserves_process_wait_timeout").split(
        ",",
    )
}

/// `FAIL [...] crate::module test_name` lines of nextest output, minus the
/// known-flaky names.
function unexpected_failures(output: string, flaky: string[]) -> string[] {
    let out: string[] = [];
    for (let line in output.lines()) {
        let l = line.trim();
        if (l.starts_with("FAIL [")) {
            let parts = l.split(" ");
            let name = parts.at(parts.length() - 1) ?? "";
            if (!flaky.includes(name) && !out.includes(name)) {
                out.push(name);
            };
        };
    }
    out
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
            60,
        ),
        exit_code: r.exit_code,
    };
    log.info({ "gate": name, "ok": r.ok, "seconds": step.seconds });
    step
}

function run_gate(worktree: string) -> GateResult {
    let wd = worktree + "/baml_language";
    //# which crates did the agent touch?
    let crates = changed_crates(worktree);
    let steps: GateStep[] = [];
    let cargo = (name: string, args: string[], timeout_ms: int) -> bool {
        let s = gate_step(name, Cmd { program: "cargo", args: args, cwd: wd, timeout_ms: timeout_ms });
        steps.push(s);
        s.ok
    };
    //# fmt
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
    //# unit tests of each changed crate
    for (let crate in crates) {
        if (ok && crate != "baml_tests") {
            ok = cargo("cargo test --lib -p " + crate, ["test", "--lib", "-p", crate], 1800000);
        };
    }
    if (ok) {
        //## nextest on baml_tests (known-flaky failures subtracted)
        // --no-fail-fast, then subtract the known-flaky tests: one flaky test
        // must not sink a gate that canary itself would not pass
        let s = gate_step(
            "nextest baml_tests",
            Cmd {
                program: "cargo",
                args: ["nextest", "run", "-p", "baml_tests", "--no-fail-fast"],
                cwd: wd,
                timeout_ms: 1800000,
            },
        );
        let real = unexpected_failures(s.tail, flaky_tests());
        // nextest exits 100 only when it compiled and ran and some tests
        // failed; a compile error / timeout (124) exits otherwise. Gate the
        // known-flaky pass on that code, not on captured stdout, so a
        // non-completed run cannot be waved through with injected output.
        let ran_tests = s.exit_code == 100;
        let step = GateStep {
            name: s.name,
            ok: s.ok || (ran_tests && real.length() == 0),
            seconds: s.seconds,
            tail: if (s.ok || !ran_tests || real.length() > 0) {
                s.tail
            } else {
                "only known-flaky failures: " + s.tail
            },
            exit_code: s.exit_code,
        };
        steps.push(step);
        ok = step.ok;
    };
    if (ok) {
        //## insta: snapshots must be settled
        // insta re-runs the crate to refresh snapshots; test verdicts were
        // already taken by the nextest step above, so this step gates on
        // snapshot state only (the tree-clean check below catches churn)
        let s = gate_step(
            "insta baml_tests",
            Cmd {
                program: "cargo",
                args: ["insta", "test", "--test-runner=nextest", "--accept", "-p", "baml_tests"],
                cwd: wd,
                timeout_ms: 1800000,
            },
        );
        // settled = the run completed (exit 0, or 100 = test failures only, e.g.
        // the known-flaky test); a compile error / timeout does not count. The
        // tree-clean check below catches any real snapshot churn.
        let snapshots_settled = s.ok || s.exit_code == 100;
        steps.push(
            GateStep {
                name: s.name,
                ok: snapshots_settled,
                seconds: s.seconds,
                tail: s.tail,
                exit_code: s.exit_code,
            },
        );
        ok = snapshots_settled;
    };
    if (ok) {
        //## the tree must be clean after --accept
        // --accept must not have produced unreviewed snapshot churn
        let status = git(worktree, ["status", "--porcelain"]);
        let clean = status.stdout.trim().length() == 0;
        steps.push(
            GateStep {
                name: "insta clean",
                ok: clean,
                seconds: 0,
                tail: tail(status.stdout, 15),
                exit_code: if (clean) {
                    0
                } else {
                    1
                },
            },
        );
        ok = clean;
    };
    if (ok) {
        //## build baml-cli and run the BAML corpus with it
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
    let rows = r.boundaries
        .map((b: Boundary) -> string {
            "| "
                + b.scenario.replace("|", "\\|")
                + " | "
                + b.result.replace("|", "\\|")
                + " | "
                + b.why.replace("|", "\\|")
                + " |"
        })
        .join("\n");
    let boundaries = if (r.boundaries.length() == 0) {
        ""
    } else {
        "## Behavioral boundaries\n\n| Scenario | Result | Why |\n| --- | --- | --- |\n"
            + rows
            + "\n\n"
    };
    fixes
        + "## Problem\n\n"
        + r.problem.trim()
        + "\n\n"
        + "## After this PR\n\n"
        + r.after.trim()
        + "\n\n"
        + boundaries
        + "## Tests\n\n"
        + r.tests.trim()
        + "\n\n"
        + "## Validation\n\n"
        + validation_checklist(g)
        + "\n_Opened by atb2 `handle_issue` ("
        + (issue.difficulty ?? Difficulty.Medium).to_string().to_lower_case()
        + ", "
        + turns.to_string()
        + " agent turns, "
        + (seconds / 60).to_string()
        + " min); the checklist above was run by the pipeline after the agent finished._\n"
}

/// The gate as a checklist: one line per step, with the pass count when
/// the tool printed one.
function validation_checklist(g: GateResult) -> string {
    g.steps
        .map((s: GateStep) -> string {
            let count = passed_count(s.tail);
            "- ["
                + (
                    if (s.ok) {
                        "x"
                    } else {
                        " "
                    }
                )
                + "] `"
                + s.name
                + "`"
                + (
                    if (count != null) {
                        ": " + (count ?? "")
                    } else {
                        ""
                    }
                )
                + (
                    if (s.ok) {
                        ""
                    } else {
                        " — FAILED"
                    }
                )
        })
        .join("\n")
        + "\n"
}

/// "N passed" from cargo test / nextest summaries, if present.
function passed_count(output: string) -> string? {
    let found: string? = null;
    for (let line in output.lines()) {
        let l = line.trim();
        let i = l.index_of(" passed");
        if (
            i != null
                && (
                    l.starts_with("test result:")
                        || l.includes("tests run:")
                        || l.starts_with("Summary")
                )
        ) {
            let head = l.slice(0, i ?? 0);
            let words = head.split(" ");
            let num = words.at(words.length() - 1) ?? "";
            if (num.length() > 0) {
                found = num + " passed";
            };
        };
    }
    found
}

/// Live only. Lifts the worktree's push guard, pushes, and opens a draft
/// PR — or updates the existing PR for the branch.
function push_and_open_pr(sb: Sandbox, draft: PrDraft) -> string {
    //# lift the push guard and push the branch
    let env = sandbox_env(true);
    // pushurl is multi-valued, so it cannot be overridden with -c; the
    // no_push guard is removed here, and only here, for this one push
    git(sb.worktree, ["config", "--worktree", "--unset-all", "remote.origin.pushurl"]);
    let push = run_with(
        Cmd {
            program: "git",
            args: [
                "-c",
                "core.hooksPath=/dev/null",
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
    //# update the existing PR for this branch, if there is one
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
    //# otherwise open a draft PR
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
    /// Why the agent stopped (agent_stopped only): the CLI's error text,
    /// "killed at the time budget", or "reply did not parse".
    reason: string?,
}

/// Reads the last agent transcript for the reason the run stopped.
function stop_reason(path: string) -> string? {
    if (!baml.fs.exists(path)) {
        return "no transcript was written";
    };
    let lines = baml.fs.read(path).split("\n").filter((l: string) -> bool {
        l.includes("\"type\":\"result\"")
    });
    if (lines.length() == 0) {
        return "killed at the time budget";
    };
    let ev = baml.json.parse(lines[lines.length() - 1]) catch (_) {
        _ => baml.json.parse("{}"),
    };
    if (baml.json.path_or<bool>(ev, ".is_error", false)) {
        return "cli error: " + baml.json.path_or<string>(ev, ".result", "").slice(0, 300);
    };
    "reply did not parse"
}

function outcome(
    sb: Sandbox,
    kind: "fixed" | "hard" | "gate_failed" | "agent_stopped",
    pr: string?,
    started: baml.time.Instant,
    gate: GateResult?,
    report: FixReport?,
    doc: string?,
) -> HandleOutcome {
    HandleOutcome {
        kind: kind,
        branch: sb.branch,
        pr: pr,
        turns: read_turns(sb),
        seconds: elapsed_s(started),
        timed_out: false,
        gate: gate,
        report: report,
        design_doc: doc,
        reason: null,
    }
}

/// An agent pass ended without a result: budget exhausted (no `result`
/// event in its transcript) or an unparseable reply.
function stopped_outcome(sb: Sandbox, started: baml.time.Instant) -> HandleOutcome {
    let last = if (baml.fs.exists(sb.run_dir + "/fix.jsonl")) {
        sb.run_dir + "/fix.jsonl"
    } else {
        sb.run_dir + "/design.jsonl"
    };
    HandleOutcome {
        kind: "agent_stopped",
        branch: sb.branch,
        pr: null,
        turns: read_turns(sb),
        seconds: elapsed_s(started),
        timed_out: baml.fs.exists(last) && !baml.fs.read(last).includes("\"type\":\"result\""),
        gate: null,
        report: null,
        design_doc: null,
        reason: stop_reason(last),
    }
}

function write_outcome(sb: Sandbox, o: HandleOutcome) -> null {
    baml.fs.write(sb.run_dir + "/outcome.json", baml.json.stringify_pretty(o.to_json()));
    log.info(
        {
            "issue_branch": sb.branch,
            "kind": o.kind,
            "reason": o.reason,
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
//   ATB2_ISSUES=4587 tools/atb2/run_tests.sh -- baml test -i 'root::fix_in_budget::*'

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
            if (row.reference_issue.time_budget_s != null && selected(only, key)) {
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
                            "budget": issue.reference_issue.time_budget_s,
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
            if (row.reference_issue.design_doc != null && selected(only, key)) {
                test ("#" + key) {
                    let issue = load_issues().get(key) ?? baml.sys.panic("no triage_issues row for #" + key);
                    let r = issue.reference_issue;
                    let target = with_difficulty(reference_as_issue(issue.number, r), Difficulty.Hard);
                    let handled = handle_issue(target, mode = HandleMode.DryRun);
                    let ours = handled.design_doc ?? baml.sys.panic("no design doc produced for #" + key);
                    let j = judge_design_doc(ours, issue.reference_issue.design_doc ?? "");
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
            GateStep { name: "fmt", ok: true, seconds: 3, tail: "", exit_code: 0 },
            GateStep { name: "clippy", ok: true, seconds: 400, tail: "", exit_code: 0 },
        ],
        ok: true,
        changed_crates: ["baml_compiler2_hir"],
    }
}

function sample_report() -> FixReport {
    FixReport {
        summary: "Guarded the type-ref lookup.",
        problem: "A `throws` clause naming an unresolved type panics the compiler:\n\n```baml\nfunction f() -> int throws Nope { 1 }\n```",
        after: "The same program reports `E0002: unresolved type Nope`. Bounds check in `type_ref.rs:346`.",
        boundaries: [
            Boundary {
                scenario: "`throws Nope`",
                result: "`E0002`",
                why: "the type is looked up before lowering",
            },
        ],
        tests: "`crates/baml_tests/projects/diagnostic_errors/unresolved_throws/main.baml` asserts the exact diagnostic.",
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
        // no ambient git credentials or ssh agent for the agent, and the host
        // git config (osxkeychain helper, planted hooks) is neutralized
        assert.is_true(!env.has("SSH_AUTH_SOCK"));
        assert.equal(env.get("GIT_CONFIG_GLOBAL") ?? "", "/dev/null");
        assert.equal(env.get("GIT_CONFIG_SYSTEM") ?? "", "/dev/null");
    }

    test "repro paths cannot escape their directory" {
        assert.equal(sanitize_rel_path("baml_src/../../../.zshrc"), "baml_src/.zshrc");
        assert.equal(sanitize_rel_path("/etc/passwd"), "etc/passwd");
        assert.equal(sanitize_rel_path("../../x"), "x");
        assert.is_true(!repro_file_path("baml_src/../../x", false).includes(".."));
        assert.is_true(!repro_file_path("../../../etc/passwd", true).includes(".."));
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

    test "the claude process writes its own transcript" {
        let out = baml.sys.exec(
            "sh",
            [
                "-c",
                "exec \"$@\" > \"$ATB2_TRANSCRIPT\" 2>&1",
                "atb2",
                "echo",
                "{\"type\":\"result\",\"num_turns\":3,\"result\":\"ok\"}",
            ],
            baml.sys.ProcessOptions {
                cwd: null,
                env: {
                    "PATH": baml.env.get("PATH") ?? "/bin:/usr/bin",
                    "ATB2_TRANSCRIPT": "/tmp/atb2-transcript-test.jsonl",
                },
                timeout_ms: 10000,
                stdin: null,
                keep_stdin_open: null,
            },
        );
        assert.equal(out.exit_code, 0);
        assert.equal(parse_stream(baml.fs.read("/tmp/atb2-transcript-test.jsonl")).num_turns, 3);
        baml.fs.remove("/tmp/atb2-transcript-test.jsonl");
    }

    test "repro files land in baml_src/ whether or not the repro says so" {
        assert.equal(repro_file_path("main.baml", false), "baml_src/main.baml");
        assert.equal(repro_file_path("baml_src/main.baml", true), "baml_src/main.baml");
        assert.equal(repro_file_path("baml.toml", true), "baml.toml");
        assert.equal(repro_file_path("clients.baml", true), "clients.baml");
    }

    test "verdicts from an out-of-process check" {
        let sc = ShouldCompile { check: "should_compile" };
        assert.equal(verdict_from_check(sc, 0, "", ""), Verdict.Fixed);
        assert.equal(
            verdict_from_check(sc, 1, "", "error[E0002]: unresolved type"),
            Verdict.StillBroken,
        );
        assert.equal(
            verdict_from_check(sc, 134, "", "thread 'main' panicked at type_ref.rs:346"),
            Verdict.StillBroken,
        );
        assert.equal(
            verdict_from_check(sc, 1, "", "internal error: entered unreachable code"),
            Verdict.StillBroken,
        );
        let snc = ShouldNotCompile { check: "should_not_compile", diagnostic_contains: "E0002" };
        assert.equal(verdict_from_check(snc, 1, "", "error[E0002]: unresolved type"), Verdict.Fixed);
        assert.equal(
            verdict_from_check(snc, 1, "", "error[E0001]: mismatched"),
            Verdict.Inconclusive,
        );
        assert.equal(verdict_from_check(snc, 0, "", ""), Verdict.StillBroken);
        let ev = ShouldEvaluateTo { check: "should_evaluate_to", expected: 2 };
        assert.equal(verdict_from_check(ev, 0, "2\n", ""), Verdict.Fixed);
        assert.equal(verdict_from_check(ev, 0, "3\n", ""), Verdict.StillBroken);
        assert.equal(
            verdict_from_check(RequiresInspection { check: "requires_inspection", instructions: "look" }, 0, "", ""),
            Verdict.Inconclusive,
        );
    }

    test "the insta step gates on snapshot state" {
        let settled = "        FAIL [ 115.989s] (1622/2042) baml_tests::baml_src baml_test\nerror: test run failed\n\ndone: no snapshots to review\n";
        assert.is_true(settled.includes("no snapshots to review"));
    }

    test "known-flaky nextest failures are subtracted" {
        let out = "        PASS [   0.9s] (1/3) baml_tests::shell exec_with_cwd\n        FAIL [   1.1s] (2/3) baml_tests::shell claude_code_client_preserves_process_wait_timeout\n        FAIL [   0.2s] (3/3) baml_tests::foo real_regression\n";
        assert.equal(
            unexpected_failures(out, ["claude_code_client_preserves_process_wait_timeout"]),
            ["real_regression"],
        );
        assert.equal(
            unexpected_failures(out, ["claude_code_client_preserves_process_wait_timeout", "real_regression"])
                .length(),
            0,
        );
        assert.is_true(claude_args("x", fix_tools()).includes("bypassPermissions"));
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
        assert.contains(body, "## Problem");
        assert.contains(body, "## After this PR");
        assert.contains(body, "| `throws Nope` | `E0002` | the type is looked up before lowering |");
        assert.contains(body, "## Tests");
        assert.contains(body, "## Validation");
        assert.contains(body, "- [x] `clippy`");
        assert.contains(body, "12 agent turns, 10 min");
    }

    test "validation counts come from the tool summaries" {
        assert.equal(
            passed_count("running 3 tests\ntest result: ok. 46 passed; 0 failed") ?? "",
            "46 passed",
        );
        assert.equal(
            passed_count(
                "     Summary [ 183.785s] 1925/2039 tests run: 1924 passed (2 slow), 1 failed, 23 skipped",
            )
                ?? "",
            "1924 passed",
        );
        assert.equal(passed_count("nothing here") == null, true);
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
            #"{ "summary": "s", "problem": "p", "after": "a", "boundaries": [{ "scenario": "x", "result": "compiles", "why": "y" }], "tests": "t", "tests_added": ["a.baml"], "files_changed": ["x.rs"], "gate_passed": true, "blockers": null }"#,
        );
        assert.equal(r.boundaries.length(), 1);
        assert.equal(r.tests_added.length(), 1);
        let d = write_design_doc$parse(#"{ "suggested_fix": "f", "doc": "## Problem\nx" }"#);
        assert.contains(render_design_doc(d), "## Suggested fix");
        let j = judge_design_doc$parse(#"{ "similarity": 0.7, "missing": null }"#);
        assert.equal(j.similarity, 0.7);
    }
}
