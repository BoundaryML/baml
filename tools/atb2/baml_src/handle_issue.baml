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

// ==================== Pipeline ====================

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
    // 126 / 127: the CLI did not run at all (run_with could not start it,
    // or caffeinate could not exec it). That says nothing about the bug.
    if (exit_code == 126 || exit_code == 127) {
        return Verdict.Inconclusive;
    };
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
        Difficulty.Easy => 3600,
        Difficulty.Medium => 7200,
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
/// is an allowlist of environment variables: none from this process leaks
/// unless named here. The agent runs with `with_gh = false` and never sees
/// GH_TOKEN, FEEDBACK_SUPABASE_KEY, ANTHROPIC_API_KEY or anything else.
///
/// This guards the environment only. HOME is the host's, because the
/// Claude Code CLI reads its login from there, so files under it
/// (~/.ssh, ~/.config/gh, ~/.aws) stay readable to the agent's Bash. The
/// git credential paths are cut off below; the rest is a known gap.
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
        baml.sys.ProcessOptions { cwd: c.cwd, env: env, timeout_ms: c.timeout_ms, stdin: null },
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

// ==================== Unit tests (token-free) ====================

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
        // the CLI never ran: not a verdict on the bug either way
        assert.equal(
            verdict_from_check(snc, 127, "", "could not start: baml check"),
            Verdict.Inconclusive,
        );
        assert.equal(verdict_from_check(sc, 126, "", ""), Verdict.Inconclusive);
        let ev = ShouldEvaluateTo { check: "should_evaluate_to", expected: 2 };
        assert.equal(verdict_from_check(ev, 0, "2\n", ""), Verdict.Fixed);
        assert.equal(verdict_from_check(ev, 0, "3\n", ""), Verdict.StillBroken);
        assert.equal(
            verdict_from_check(RequiresInspection { check: "requires_inspection", instructions: "look" }, 0, "", ""),
            Verdict.Inconclusive,
        );
    }

    test "time budgets" {
        assert.equal(time_budget_s(Difficulty.Trivial), 1800);
        assert.equal(time_budget_s(Difficulty.Easy), 3600);
        assert.equal(time_budget_s(Difficulty.Medium), 7200);
        assert.equal(time_budget_s(Difficulty.Hard), 1800);
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
}
