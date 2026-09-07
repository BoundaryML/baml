// issue -> fix (or design doc).
//
//   Easy / Medium: an agent in a git worktree on a fresh branch implements
//                  the fix, encodes the repro(s) as tests, and runs the
//                  targeted checks; BAML scans, pushes and opens a
//                  PR (Live) or stops before pushing (DryRun).
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
    /// Push the branch and open a PR when the gate passes.
    Live,
    /// Everything up to (not including) the push — what the evals run.
    DryRun,
}

// ==================== Pipeline ====================
/// A PR under review: merge_issue hands handle_issue the PR's branch and the
/// approved plan to implement on its exact head.
class Review {
    branch: string,
    /// The approved fix plan, including its CI and reviewer evidence.
    feedback: string,
    expected_head: string?,
    proposal_id: string?,
}

//#handle_issue(issue: Issue, mode: HandleMode = HandleMode.Live, update: bool = false, review: Review? = null, thread: SlackRef? = null) -> Issue
function handle_issue(
    issue: Issue,
    mode: HandleMode = HandleMode.Live,
    update: bool = false,
    review: Review? = null,
    /// A Slack thread that hears each stage: design pass, fix pass, gate, push.
    thread: SlackRef? = null,
) -> Issue {
    if (mode == HandleMode.Live && !db_configured()) {
        throw StoreError { message: "live fixes require the store so their PR can enter babysitting" };
    };
    if (review == null) {
        //# skip: not ready (not gauged / not open / already handled)
        let skip = handle_ready(issue, update);
        if (skip != null) {
            log.info({ "issue": issue.id, "skipped": skip });
            return issue;
        };
        //# prepare the requested CLI only when repros need it
        // Only a cache entry built from current canary can establish already-fixed.
        ensure_cache();
        if (!still_broken(issue)) {
            //## already fixed: nothing to do
            log.info({ "issue": issue.id, "kind": "already_fixed", "checked_with": check_cli() });
            return issue;
        };
    } else {
        //# under review: fetch the repo
        ensure_cache();
    };
    let difficulty = issue.difficulty ?? Difficulty.Medium;
    //# open a worktree: a fresh branch, or the PR's branch under review
    let sb = match (review) {
        null => open_sandbox(issue, update),
        // always from origin: a local branch kept after a failed round would
        // be stale against what a human may have pushed since
        let r: Review => open_branch_sandbox(r.branch, true),
    };
    let keep_branch = false;
    defer {
        //## always: tear the sandbox down
        close_sandbox(sb, keep_branch);
    }
    if (mode == HandleMode.Live) {
        bind_agent_session(
            sb,
            thread,
            if (review == null) {
                issue.id
            } else {
                null
            },
            match (issue.status) {
                let status: InProgress => status.pr,
                _ => null,
            },
            if (review == null) {
                "issue"
            } else {
                "babysit"
            },
        );
    };
    let started = baml.time.Instant.now();
    if (review != null) {
        if (
            review.expected_head == null
                || review.proposal_id == null
                || !babysit_push_allowed(review.proposal_id ?? "", review.expected_head ?? "")
                || git(sb.worktree, ["rev-parse", "HEAD"]).stdout.trim() != review.expected_head
        ) {
            write_outcome(sb, stopped_outcome(sb, started, reason = "missing approval or PR head changed"));
            tell_thread(thread, ":hand: missing approval or PR head changed; no fix was started");
            return issue;
        };
    };
    let doc = "";
    if (review == null) {
        //# design pass: investigate, propose the fix, write the plan (read-only)
        let design = run_agent_design(sb, issue, design_budget_s(difficulty));
        if (design == null) {
            //## stop: the design agent produced nothing
            write_outcome(sb, stopped_outcome(sb, started));
            tell_thread(thread, ":x: the design pass produced nothing; a human takes it from here");
            return issue;
        };
        doc = render_design_doc(design ?? baml.sys.panic("unreachable"));
        baml.fs.write(sb.run_dir + "/design.md", doc);
        if (difficulty == Difficulty.Hard) {
            //## hard: hand the design doc to the shepherd, no code change
            write_outcome(sb, outcome(sb, "hard", null, started, null, null, doc));
            tell_thread(
                thread,
                ":memo: gauged Hard: the design doc is written for the shepherd, no code was changed",
            );
            return with_design_doc(issue, doc);
        };
        tell_thread(thread, ":memo: design pass done, starting the fix pass on `" + sb.branch + "`");
    } else {
        //# under review: implement the approved proposal
        doc
            = "APPROVED PLAN: implement only this scope. If a different fix is needed, stop and report blockers; do not substitute another plan.\n\n"
                + review.feedback;
        baml.fs.write(sb.run_dir + "/review.md", doc);
    };
    // what the caller gets back: the design doc lands on the issue; review
    // feedback does not
    let out = if (review == null) {
        with_design_doc(issue, doc)
    } else {
        issue
    };
    //# fix pass: implement the plan, encode the repros as tests
    let report = run_agent_fix(sb, issue, doc, time_budget_s(difficulty));
    if (report == null) {
        //## stop: the fix agent ran out of budget or reported nothing
        keep_branch = mode == HandleMode.Live;
        write_outcome(sb, stopped_outcome(sb, started));
        tell_thread(thread, ":x: the fix agent stopped without a result on `" + sb.branch + "`");
        return out;
    };
    let fix = report ?? baml.sys.panic("unreachable");
    if (review != null && fix.blockers != null && (fix.blockers ?? "").trim().length() > 0) {
        keep_branch = mode == HandleMode.Live;
        write_outcome(
            sb,
            stopped_outcome(
                sb,
                started,
                reason = "approved plan blocked: " + (fix.blockers ?? "").trim(),
                report = fix,
            ),
        );
        tell_thread(
            thread,
            ":hand: the approved plan could not be completed as proposed; nothing pushed. Request a revised proposal.",
        );
        return out;
    };
    // CI on the PR is the build/test gate. The trusted push still scans secrets.
    checkpoint_uncommitted(sb);
    let gate: GateResult? = null;
    if (review != null) {
        //# under review: verify approval, announce, and push the fix
        if (mode == HandleMode.DryRun) {
            //## dry run: stop before the push
            write_outcome(sb, outcome(sb, "fixed", null, started, gate, fix, null));
            return out;
        };
        // The approval authorizes one exact base. Recheck immediately before
        // pushing, then use an explicit lease rather than a mutable tracking ref.
        if (
            !babysit_push_allowed(review.proposal_id ?? "", review.expected_head ?? "")
                || !git(
                    sb.worktree,
                    ["merge-base", "--is-ancestor", review.expected_head ?? "", "HEAD"],
                ).ok
        ) {
            write_outcome(
                sb,
                stopped_outcome(sb, started, reason = "proposal is no longer executable", report = fix),
            );
            tell_thread(thread, ":hand: proposal is no longer executable; nothing pushed");
            return out;
        };
        // This notification is required: a Slack outage stops the push.
        if (thread != null) {
            slack_post(
                ":arrow_up: approved fix ready; scanning and pushing to `" + sb.branch + "` now.",
                thread,
            );
        };
        record_pr_event(
            pr_url_in_from_issue(issue),
            "babysit_pushing",
            { "proposal_id": review.proposal_id }.to_json(),
            thread,
        );
        push_branch(sb, expected_head = review.expected_head);
        let pr = match (issue.status) {
            let s: InProgress => s.pr ?? "",
            _ => "",
        };
        //# the thread hears what was pushed
        tell_thread(
            thread,
            ":arrows_counterclockwise: pushed "
                + head_sha(sb)
                + " to "
                + pr
                + "; waiting for PR CI and review results"
                + "\n"
                + fix.summary.trim()
                + "\n_files: "
                + fix.files_changed.join(", ")
                + "_",
        );
        write_outcome(sb, outcome(sb, "fixed", pr, started, gate, fix, null));
        return out;
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
    //# push the branch and open the PR
    let pr = push_and_open_pr(sb, draft);
    write_outcome(sb, outcome(sb, "fixed", pr, started, gate, fix, doc));
    let progressed = handoff_issue_pr(issue, pr, doc, thread);
    tell_thread(
        thread,
        ":rocket: PR opened; babysitting starts after the fix is saved: "
            + pr
            + "\n"
            + fix.summary.trim(),
    );
    log.info(
        { "issue": issue.id, "pr": pr, "turns": read_turns(sb), "seconds": elapsed_s(started) },
    );
    progressed
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
        let s: Approved => null,
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
    // An issue requiring a concrete compiler triggers a cache lookup/build.
    // A cache miss/failure is inconclusive, never evidence that the issue is fixed.
    let cli = cli_for_version(issue.version);
    if (cli == null) {
        return true;
    };
    // An older version cannot establish whether current canary is already fixed.
    let head = git(cache_repo(), ["rev-parse", "HEAD"]);
    if (!head.ok || !(cli ?? "").includes("/" + head.stdout.trim() + "/baml-cli")) {
        return true;
    };
    let i = 0;
    for (let r in issue.repros) {
        i += 1;
        if (check_on_canary(r, i, cli = cli) != Verdict.Fixed) {
            return true;
        };
    }
    false
}

function check_cli() -> string {
    atb2_home() + "/target/debug/baml-cli"
}

/// Controller-only request: agents cannot write the shared cache or access its socket.
function cli_for_version(version: string) -> string? {
    if (atb2_home() != "/data") {
        return check_cli();
    };
    let out = baml.sys.exec(
        "/usr/bin/python3",
        ["-I", "/usr/local/lib/atb2/cli-cache-service.py", version],
        baml.sys.ProcessOptions { env: { "PATH": "/usr/bin:/bin" }, timeout_ms: 2550000 },
    );
    if (out.exit_code != 0) {
        log.warn({ "cli_version": version, "cache": "unavailable; repro remains inconclusive" });
        return null;
    };
    out.stdout.to_string().trim()
}

/// Write the repro to a scratch project and run it with check_cli().
function check_on_canary(r: Repro, n: int, cli: string? = null) -> Verdict {
    //# write the repro to a scratch project
    let repro_id = baml.id.new();
    let dir = atb2_home() + "/repro-check/" + repro_id;
    baml.fs.mkdir(dir + "/baml_src", baml.fs.MkdirOptions { recursive: true });
    defer {
        baml.fs.remove_dir_all(dir) catch (_) {
            _ => null,
        };
        baml.fs.remove_dir_all(atb2_home() + "/agent-cache/repro-check/" + repro_id) catch (_) {
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
    let out = run_with(Cmd { program: cli ?? check_cli(), args: args, cwd: dir, timeout_ms: 180000 }, env);
    //# map exit code + diagnostics to a verdict
    let verdict = verdict_from_check(r.expectation, out.exit_code, out.stdout, out.stderr);
    log.info(
        {
            "repro_check": n,
            "cli": cli ?? check_cli(),
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
    let reset_anchor = match (status) {
        let s: AwaitingApproval => true,
        let s: Open => true,
        _ => false,
    };
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
        slack_ts: if (reset_anchor) {
            null
        } else {
            issue.slack_ts
        },
        slack_channel: if (reset_anchor) {
            null
        } else {
            issue.slack_channel
        },
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
        slack_ts: issue.slack_ts,
        slack_channel: issue.slack_channel,
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
    atb2_home() + "/agent-cache/target"
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

/// Allowlisted controller subprocess environment. Untrusted checkout commands
/// additionally enter the Linux mount/process sandbox, which replaces HOME and
/// the environment again. Claude receives only a temporary broker capability;
/// its persistent login stays outside that boundary.
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
        match (github_token()) {
            null => null,
            let t: string => env.set("GH_TOKEN", t),
        };
    }
    env
}

/// The token gh and git push with: GH_TOKEN, GITHUB_TOKEN, or the old
/// bot's ATB_GITHUB_TOKEN (Infisical boundary-tools).
function github_token() -> string? {
    baml.env.get("GH_TOKEN") ?? baml.env.get("GITHUB_TOKEN") ?? baml.env.get("ATB_GITHUB_TOKEN")
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
    let untrusted = c.cwd.starts_with(atb2_home() + "/worktrees/")
        || c.cwd.starts_with(atb2_home() + "/repro-check/")
        || (c.cwd == cache_repo() + "/baml_language");
    let awake = if (untrusted) {
        keep_awake(
            "/usr/bin/python3",
            ["-I", "/usr/local/lib/atb2/sandbox.py", c.cwd, c.program].concat(c.args),
        )
    } else {
        keep_awake(c.program, c.args)
    };
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
    base_head: string?,
    source_head: string,
}

/// One cached clone, fetched each run. Safe to call from parallel tests:
/// the fetch is retried when another child holds the index lock.
function ensure_cache() -> null {
    //# get the repo
    let repo = cache_repo();
    //# mkdir the atb2 home
    baml.fs.mkdir(atb2_home(), baml.fs.MkdirOptions { recursive: true });
    //# mkdir the target dir
    baml.fs.mkdir(atb2_home() + "/agent-cache", baml.fs.MkdirOptions { recursive: true });
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

function branch_exists_on_origin(branch: string) -> bool {
    git(cache_repo(), ["ls-remote", "--exit-code", "--heads", "origin", branch]).ok
}

function open_sandbox(issue: Issue, update: bool) -> Sandbox {
    open_branch_sandbox(branch_for(issue), update)
}

/// The sandbox for any branch: a fresh one off origin/canary, or (`update`)
/// the branch as it is on origin. merge_issue opens PR branches this way.
function open_branch_sandbox(branch: string, update: bool) -> Sandbox {
    let wt = worktree_for(branch) + "-" + baml.id.new();
    let rd = run_dir_for(branch);
    if (baml.fs.exists(wt)) {
        baml.fs.remove_dir_all(wt);
    };
    if (baml.fs.exists(rd)) {
        baml.fs.remove_dir_all(rd);
    };
    baml.fs.mkdir(rd, baml.fs.MkdirOptions { recursive: true });
    baml.fs.mkdir(atb2_home() + "/worktrees", baml.fs.MkdirOptions { recursive: true });
    // Independent object storage and metadata: never mount the controller's
    // linked-worktree .git directory into an agent sandbox.
    let clone = git(cache_repo(), ["clone", "--no-local", "--no-hardlinks", "--no-checkout", cache_repo(), wt]);
    if (!clone.ok) {
        baml.sys.panic("isolated clone failed");
    };
    let reused = branch_exists_on_origin(branch);
    if (reused && !update) {
        baml.sys.panic("branch already exists; explicit update required");
    };
    if (reused) {
        if (!git(wt, ["fetch", repo_url(), "refs/heads/" + branch]).ok) {
            baml.sys.panic("isolated branch fetch failed");
        };
    };
    let base = if (reused) {
        "FETCH_HEAD"
    } else {
        "origin/canary"
    };
    if (!git(wt, ["checkout", "-b", branch, base]).ok) {
        baml.sys.panic("isolated checkout failed");
    };
    git(wt, ["config", "remote.origin.pushurl", "no_push://atb2"]);
    git(wt, ["config", "user.name", "atb2"]);
    git(wt, ["config", "user.email", "atb2@boundaryml.com"]);
    let base_head = if (reused) {
        git(wt, ["rev-parse", "HEAD"]).stdout.trim()
    } else {
        null
    };
    Sandbox {
        branch: branch,
        worktree: wt,
        run_dir: rd,
        reused_branch: reused,
        base_head: base_head,
        source_head: git(wt, ["rev-parse", "HEAD"]).stdout.trim(),
    }
}

function close_sandbox(sb: Sandbox, keep_branch: bool) -> null {
    // Build output is private to this invocation and can be large. Keep the
    // conversation checkout if needed, but reclaim its completed build cache.
    let cache = atb2_home()
        + "/agent-cache/worktrees/"
        + sb.worktree.replace_all(atb2_home() + "/worktrees/", "");
    if (baml.fs.exists(cache)) {
        baml.fs.remove_dir_all(cache);
    };
    let session_index = atb2_home()
        + "/agent-sessions/workspaces/"
        + sb.worktree.replace_all(atb2_home() + "/worktrees/", "");
    if (baml.fs.exists(session_index)) {
        release_agent_session(sb);
    };
    if (!keep_branch && !baml.fs.exists(session_index) && baml.fs.exists(sb.worktree)) {
        baml.fs.remove_dir_all(sb.worktree);
    };
    if ((baml.env.get("ATB2_KEEP_RUNS") ?? "1") == "0") {
        baml.fs.remove_dir_all(sb.run_dir) catch (_) {
            _ => null
        };
    };
    null
}

/// Whatever the agent left uncommitted is committed before the worktree is
/// removed, so nothing is lost before the trusted secret scan and push.
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
client Handle = claude_code.ClaudeCodeClient.new(model = "claude-fable-5");

function agent_model() -> string {
    baml.env.get("ATB2_MODEL") ?? "claude-fable-5"
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
        "--safe-mode",
        "--setting-sources",
        "",
        "--strict-mcp-config",
        "--mcp-config",
        "{\"mcpServers\":{}}",
        "--settings",
        "{\"disableAllHooks\":true}",
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
            program: "claude",
            args: claude_args(prompt, tools),
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
        exit_code: if (r.exit_code != 0) {
            r.exit_code
        } else {
            parsed.exit_code
        },
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
    let failed = false;
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
            failed = failed || baml.json.path_or<bool>(ev, ".is_error", false);
            text = baml.json.path_or<string>(ev, ".result", "");
        };
    }
    AgentRun {
        text: if (failed) {
            ""
        } else {
            text
        },
        num_turns: if (saw_result) {
            turns
        } else {
            assistants
        },
        exit_code: if (failed || !saw_result) {
            1
        } else {
            0
        },
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
        + "The Rust workspace is `baml_language/` (run cargo from there); `CARGO_TARGET_DIR` is private to this checkout, so "
        + "the built CLI is at `$CARGO_TARGET_DIR/debug/baml-cli`, not `target/`.\n"
        + "Rules:\n"
        + "- Stay on this branch. Never `git push`, never checkout or create another branch, never touch `.git` directly.\n"
        + "- Never create or modify files outside this worktree.\n"
        + "- Do not add Co-Authored-By or session trailers to commits.\n"
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
    /// Agent-reported validation only; PR CI is authoritative.
    gate_passed: bool,
    /// Actual reasons the approved scope cannot be completed. JSON null when complete;
    /// put unrelated failures, caveats and validation notes in tests, not here.
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

        Fix this issue in the BAML toolchain. Below is either the plan a read-only design pass
        wrote, or (when this branch is already a PR) the CI failures and reviewer comments to
        address on it; follow it unless the code proves it wrong, and if you deviate, say so in
        \`summary\`. For review feedback: read the branch's own diff first
        (\`git diff origin/canary...HEAD\`), address ALL of it and nothing else, and when a
        comment is wrong leave the code and explain the disagreement in \`summary\`.
        For an approved plan, do not change scope: report an actual required deviation in \`blockers\`.
        Set \`blockers\` to JSON null when the approved scope is complete. Never write "None"
        or validation notes there: any nonempty blocker stops the run before a push.
        Put unrelated check failures and validation limitations in \`tests\`; do not claim they
        are pre-existing without evidence. If they prevent the approved work, they are blockers.

        Title: ${issue.title}
        Subsystem: ${issue.subsystem.to_string()}

        ${issue.description}

        Resolution plan from triage (a starting point, not gospel):
        ${issue.resolution_plan ?? "(none)"}

        Repros (${issue.repros.length().to_string()}):
        ${issue.repros.map((r: Repro) -> string { format_repro(r) }).join("\n\n")}

        The plan, or the feedback to address:
        ${plan}

        How to work:
        1. Reproduce FIRST. Turn every repro above into a test before touching the fix, placed
           in the affected component's test suite. For Rust/BAML changes, follow
           baml_language/TEST_INSTRUCTIONS.md. For Python controller changes, use its Python
           tests; do not invent a Rust repro for behavior Rust cannot observe.
           Suggested route for Rust/BAML changes: ${test_route}
           Confirm the new test fails before the fix and passes after it.
        2. Make the smallest correct fix. Cite the files you changed.
        3. Run focused checks for the changed component when useful. CI on the PR is the
           build/test gate; do not run a full workspace build, clippy, nextest or corpus suite
           automatically. Existing BAML CLIs are available read-only under /data/cli-cache,
           indexed by version and source revision in index.json. The controller builds a version
           lazily only when an issue with repros needs it; builds are shared and immutable.
           Use the matching cached CLI
           when available; /data/target/debug/baml-cli is the runner's pinned version.
           Do not claim a cached CLI includes your unbuilt edits. If testing a compiler change
           needs a fresh build, report what you actually verified and let PR CI validate it.
        4. Commit everything. \`git status --porcelain\` must be empty. Do not push.
        5. Report in the shape of this team's PRs: \`problem\` (with the repro in a fenced baml code block),
           \`after\` (quote the exact diagnostic/value the repro now produces; cite files), a
           \`boundaries\` table of the cases around the fix (compiles / rejected / value, and why),
           and \`tests\` (paths, what each asserts, snapshots refreshed). Declarative, no hedging.

        ${ctx.output_format()}
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
    let prompt = implement_fix@render_prompt(issue, sb.branch, test_route(issue), plan).text();
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
    implement_fix@parse(session.text) catch (_) {
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

        ${ctx.output_format()}
    `
}

function render_design_doc(d: DesignDoc) -> string {
    "## Suggested fix\n\n" + d.suggested_fix + "\n\n" + d.doc
}

function run_agent_design(sb: Sandbox, issue: Issue, timeout_s: int) -> DesignDoc? {
    //# render the prompt: rules + ticket + repros, read-only
    let prompt = write_design_doc@render_prompt(issue, sb.branch).text();
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
    write_design_doc@parse(session.text) catch (_) {
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
        // Every nonzero exit fails closed. Log tails are display-only and
        // cannot establish that a failed run contained only known failures.
        ok
            = cargo(
                "nextest baml_tests",
                ["nextest", "run", "-p", "baml_tests", "--no-fail-fast"],
                1800000,
            );
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
        //## the tree must be clean after --accept
        // --accept must not have produced unreviewed snapshot churn
        let status = git(worktree, ["status", "--porcelain"]);
        let clean = status.ok && status.stdout.trim().length() == 0;
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

function pr_body(issue: Issue, r: FixReport, g: GateResult?, turns: int, seconds: int) -> string {
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
        + match (g) {
            null => "CI on this PR is the validation gate. No local pipeline gate was run.\n",
            let checked: GateResult => validation_checklist(checked),
        }
        + "\n_Opened by atb2 `handle_issue` ("
        + (issue.difficulty ?? Difficulty.Medium).to_string().to_lower_case()
        + ", "
        + turns.to_string()
        + " agent turns, "
        + (seconds / 60).to_string()
        + " min)._\n"
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

/// The worktree's HEAD, short, for the thread.
function head_sha(sb: Sandbox) -> string {
    git(sb.worktree, ["rev-parse", "--short", "HEAD"]).stdout.trim()
}

/// Live only. Export objects without credentials, then push from fresh trusted
/// metadata to a fixed repository with a lease captured before agent execution.
function push_branch(sb: Sandbox, expected_head: string? = null) -> null {
    let push = run_with(
        Cmd {
            program: "/usr/bin/python3",
            args: [
                "-I",
                "/usr/local/lib/atb2/push.py",
                sb.worktree,
                sb.branch,
                expected_head ?? sb.base_head ?? "",
                sb.source_head,
            ],
            cwd: atb2_home(),
            timeout_ms: 600000,
        },
        sandbox_env(true),
    );
    if (!push.ok) {
        baml.sys.panic("trusted push failed");
    };
    null
}

/// Live only. Pushes the branch and opens a PR — or updates the
/// existing PR for the branch.
function push_and_open_pr(sb: Sandbox, draft: PrDraft) -> string {
    //# push through trusted metadata
    let env = sandbox_env(true);
    push_branch(sb);
    //# update the existing PR for this branch, if there is one
    let body_path = sb.run_dir + "/pr_body.md";
    let existing = run_with(
        Cmd {
            program: "gh",
            args: ["pr", "view", sb.branch, "--repo", repo_slug(), "--json", "url", "--jq", ".url"],
            cwd: atb2_home(),
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
                cwd: atb2_home(),
                timeout_ms: 120000,
            },
            env,
        );
        return existing.stdout.trim();
    };
    //# otherwise open a PR
    let created = run_with(
        Cmd {
            program: "gh",
            args: [
                "pr",
                "create",
                "--repo",
                repo_slug(),
                "--base",
                "canary",
                "--head",
                sb.branch,
                "--title",
                draft.title,
                "--body-file",
                body_path,
            ],
            cwd: atb2_home(),
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

function last_result_event(transcript: string) -> json? {
    let result: json? = null;
    for (let line in transcript.lines()) {
        let ev = baml.json.parse(line) catch (_) {
            _ => null
        };
        if (
            ev != null
                && baml.json.path_or<string>(ev ?? baml.json.parse("{}"), ".type", "") == "result"
        ) {
            result = ev;
        };
    }
    result
}

/// Reads the last agent transcript for the reason the run stopped.
function stop_reason(path: string) -> string? {
    if (!baml.fs.exists(path)) {
        return "no transcript was written";
    };
    let ev = last_result_event(baml.fs.read(path));
    if (ev == null) {
        return "agent ended without a result; timeout or interruption is unverified";
    };
    if (baml.json.path_or<bool>(ev ?? baml.json.parse("{}"), ".is_error", false)) {
        return "Claude CLI reported an error; inspect the private transcript";
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
function stopped_outcome(
    sb: Sandbox,
    started: baml.time.Instant,
    reason: string? = null,
    report: FixReport? = null,
) -> HandleOutcome {
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
        // A missing result is not evidence of a timeout (manual kills and crashes also omit it).
        timed_out: false,
        gate: null,
        report: report,
        design_doc: null,
        reason: reason ?? stop_reason(last),
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

        ${ctx.output_format()}
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
                    // the store, when configured, gets the eval's rows marked dataset=eval
                    // (run_tests.sh exports ATB2_DATASET=eval); never mixed with live ones
                    save_issue(target);
                    save_run(target, out, HandleMode.DryRun);
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
        slack_ts: null,
        slack_channel: null,
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
            slack_ts: null,
            slack_channel: null,
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
                && !env.has("ANTHROPIC_API_KEY")
                && !env.has("CLAUDE_CODE_OAUTH_TOKEN"),
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

    test "the insta step gates on snapshot state" {
        let settled = "        FAIL [ 115.989s] (1622/2042) baml_tests::baml_src baml_test\nerror: test run failed\n\ndone: no snapshots to review\n";
        assert.is_true(settled.includes("no snapshots to review"));
    }

    test "failed gate output cannot become a pass when truncated" {
        let step = gate_step(
            "failure",
            Cmd {
                program: "/bin/sh",
                args: [
                    "-c",
                    "echo 'FAIL [0.1s] unexpected_regression'; i=0; while [ $i -lt 65 ]; do echo ordinary-output; i=$((i+1)); done; exit 100",
                ],
                cwd: "/tmp",
                timeout_ms: 10000,
            },
        );
        assert.equal(step.exit_code, 100);
        assert.is_true(!step.ok);
        assert.is_true(!step.tail.includes("unexpected_regression"));
    }

    test "time budgets" {
        assert.equal(time_budget_s(Difficulty.Trivial), 1800);
        assert.equal(time_budget_s(Difficulty.Easy), 3600);
        assert.equal(time_budget_s(Difficulty.Medium), 7200);
        assert.equal(time_budget_s(Difficulty.Hard), 1800);
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
            slack_ts: null,
            slack_channel: null,
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

    test "blocked and revoked runs preserve their reason instead of claiming parse failure" {
        let sb = Sandbox {
            branch: "test",
            worktree: "/tmp/unused",
            run_dir: "/tmp/atb2-stop-" + baml.id.new(),
            reused_branch: true,
            base_head: null,
            source_head: "head",
        };
        let report = sample_report();
        let stopped = stopped_outcome(
            sb,
            baml.time.Instant.now(),
            reason = "approved plan blocked: needs a schema change",
            report = report,
        );
        assert.equal(stopped.reason, "approved plan blocked: needs a schema change");
        assert.equal(stopped.timed_out, false);
        assert.equal(stopped.report?.summary, report.summary);
        assert.equal(stopped.gate, null);
        let revoked = stopped_outcome(sb, baml.time.Instant.now(), reason = "proposal is no longer executable");
        assert.equal(revoked.reason, "proposal is no longer executable");
        assert.equal(revoked.report, null);
    }

    test "fenced fix reports with embedded code and blocker notes parse" {
        let row = baml.json.from_json<map<string, json>>(sample_report().to_json());
        row.set("problem", "Example:\n```baml\nfunction Example() -> int { 1 }\n```".to_json());
        row.set(
            "blockers",
            "None for the requested fix; unrelated checks need investigation".to_json(),
        );
        let r = implement_fix@parse("```json\n" + baml.json.stringify(row.to_json()) + "\n```");
        assert.contains(r.problem, "function Example");
        assert.contains(r.blockers ?? "", "unrelated checks");
    }

    test "CLI error results and truncated streams fail closed" {
        let error = parse_stream(`{"type": "result", "is_error": true, "result": "a plausible report", "num_turns": 2}`);
        assert.equal(error.exit_code, 1);
        assert.equal(error.text, "");
        assert.equal(parse_stream(`{"type":"assistant"}`).exit_code, 1);
        assert.equal(
            parse_stream(`{"type": "result", "is_error": false, "result": "ok"}`).text,
            "ok",
        );
        assert.equal(
            last_result_event("bad line\n" + `{"type": "result", "result": "ok"}`) != null,
            true,
        );
        assert.equal(last_result_event(`{"type":"assistant","text":"result"}`), null);
    }

    test "llm replies parse" {
        let r = implement_fix@parse(
            `{ "summary": "s", "problem": "p", "after": "a", "boundaries": [{ "scenario": "x", "result": "compiles", "why": "y" }], "tests": "t", "tests_added": ["a.baml"], "files_changed": ["x.rs"], "gate_passed": true, "blockers": null }`,
        );
        assert.equal(r.boundaries.length(), 1);
        assert.equal(r.tests_added.length(), 1);
        let d = write_design_doc@parse(`{ "suggested_fix": "f", "doc": "## Problem\\nx" }`);
        assert.contains(render_design_doc(d), "## Suggested fix");
        let j = judge_design_doc@parse(`{ "similarity": 0.7, "missing": null }`);
        assert.equal(j.similarity, 0.7);
    }
}

function pr_url_in_from_issue(issue: Issue) -> string {
    match (issue.status) {
        let s: InProgress => s.pr ?? "",
        _ => "",
    }
}

/// Save the PR association before making its shared babysitter runnable.
/// If queue insertion fails, pipeline reconciliation recovers this in-progress issue.
function handoff_issue_pr(issue: Issue, pr: string, doc: string, thread: SlackRef?) -> Issue {
    let progressed = with_status(with_design_doc(issue, doc), InProgress { state: "in_progress", pr: pr });
    save_issue(progressed);
    // Reconciliation queues this only after handle_one has saved the outcome
    // and handle_issue has closed its sandbox. Never wake a worker here.
    progressed
}
