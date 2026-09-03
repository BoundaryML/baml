// PR -> green -> merged. Give it a PR; it watches the PR until it merges
// or closes. Each time CI fails or the reviewer (CodeRabbit by default)
// leaves comments, it gathers them into a brief and hands the PR's branch
// to handle_issue, which runs the fix pass, the gate and the push exactly
// as it does for a fresh issue. Stops when the PR merges or closes, when
// it is green and nothing is new, or after `max_rounds`.
//
//   cd tools/atb2 && baml run -e 'merge_issue("https://github.com/BoundaryML/baml/pull/4634")'
//   ATB2_REVIEWERS=coderabbitai[bot],sxlijin   # whose comments are acted on
//   ATB2_POLL_S=120  ATB2_MAX_WAIT_S=14400
//
// Works on any PR of the repo, not only the ones handle_issue opened.
// Audit trail: ~/.atb2/merge/<branch>/state.json (what has been addressed)
// and round-<n>.outcome.json (how each round ended), plus handle_issue's
// review.md / fix.jsonl / outcome.json for the latest round.

// ==================== Pipeline ====================

/// How a merge_issue call ended.
class MergeResult {
    /// merged: the PR merged. green: checks pass and nothing is new; waiting
    /// on humans. closed: closed without merging. waiting_on_checks: gave
    /// up waiting for checks (ATB2_MAX_WAIT_S). out_of_rounds: `max_rounds`
    /// spent with feedback still pending. dry_run: one round, nothing pushed.
    /// round_failed: a round ended without a push (gate failed, agent
    /// stopped); nothing on the PR changed, so a human takes it from here.
    /// fork_refused: the PR's branch lives in another repository; its code
    /// would run on this host (build scripts, tests, the agent), so it is
    /// not touched unless ATB2_ALLOW_FORKS=1.
    kind: "merged"
        | "green"
        | "closed"
        | "waiting_on_checks"
        | "out_of_rounds"
        | "dry_run"
        | "round_failed"
        | "fork_refused",
    pr: string,
    rounds: int,
}

//#merge_issue(pr: string, mode: HandleMode = HandleMode.Live, max_rounds: int = 3, issue_id: string? = null) -> MergeResult
function merge_issue(
    pr: string,
    mode: HandleMode = HandleMode.Live,
    max_rounds: int = 3,
    /// The store's issue the PR belongs to, for the merge_rounds rows.
    issue_id: string? = null,
) -> MergeResult {
    let started = baml.time.Instant.now();
    let first = pr_snapshot(pr);
    let state = read_merge_state(first.branch);
    let done = (
        kind: "merged"
            | "green"
            | "closed"
            | "waiting_on_checks"
            | "out_of_rounds"
            | "dry_run"
            | "round_failed"
            | "fork_refused",
    ) -> MergeResult {
        log.info({ "pr": pr, "result": kind, "rounds": state.rounds });
        MergeResult { kind: kind, pr: pr, rounds: state.rounds }
    };
    //# refuse: a fork's code would run on this host
    if (first.cross_repo && !forks_allowed()) {
        return done("fork_refused");
    };
    //# poll the PR until it merges, closes, or something needs fixing
    while (true) {
        let snap = pr_snapshot(pr);
        if (snap.merged) {
            //## merged: done
            return done("merged");
        };
        if (snap.state != "OPEN") {
            //## closed without merging: a human decided
            return done("closed");
        };
        //# checks still running: wait before judging anything
        // one round addresses CI failures and comments together; starting on
        // comments while CI runs would push over the run and cancel it
        if (snap.pending_checks > 0) {
            if (elapsed_s(started) >= max_wait_s()) {
                //## checks still running past the wait limit
                return done("waiting_on_checks");
            };
            log.info({ "pr": pr, "waiting_on_checks": snap.pending_checks });
            wait_poll();
            continue;
        };
        //# 1. failing checks, 2. reviewer comments, not yet addressed
        let fb = pending_feedback(snap, state);
        if (fb.checks.length() == 0 && fb.comments.length() == 0) {
            //## green and nothing new: waiting on humans
            return done("green");
        };
        if (state.rounds >= max_rounds) {
            //## out of rounds: a human takes it from here
            return done("out_of_rounds");
        };
        //# hand the branch and the brief to handle_issue (fix pass, gate, push)
        let brief = render_feedback(fb, failed_logs(fb));
        handle_issue(issue_from_pr(snap), mode = mode, review = Review { branch: snap.branch, feedback: brief });
        //# how the round ended: handle_issue says so in outcome.json, not in its return
        let round = state.rounds + 1;
        let kind = round_outcome(snap.branch, round);
        state = MergeState { rounds: round, handled_ids: state.handled_ids };
        //# the round goes to the store
        save_merge_round(
            issue_id,
            pr,
            round,
            kind,
            fb.comments.map((c: ReviewComment) -> int {
                c.id
            }),
            brief,
        );
        if (kind != "fixed") {
            //## the round ended without a push: nothing on the PR changed
            write_merge_state(snap.branch, state);
            return done("round_failed");
        };
        if (mode == HandleMode.DryRun) {
            //## dry run: one round, nothing pushed
            write_merge_state(snap.branch, state);
            return done("dry_run");
        };
        //# the fix is on the PR: the comments it answered are handled
        state
            = MergeState {
                rounds: round,
                handled_ids: state.handled_ids.concat(
                    fb.comments.map((c: ReviewComment) -> int {
                        c.id
                    }),
                ),
            };
        write_merge_state(snap.branch, state);
        //# wait for the push to land and its checks to register
        // right after a push GitHub lists no checks on the new head for a
        // moment; an empty list would read as green
        wait_for_new_head(pr, snap.head, started);
    }
    done("green")
}

/// The PR as an Issue, so handle_issue can work on it: the PR's title and
/// body are the ticket; the status carries the PR for the review comment.
function issue_from_pr(snap: PrSnapshot) -> Issue {
    Issue {
        id: "PR-" + snap.number.to_string(),
        title: snap.title,
        description: snap.body,
        shepherd: null,
        subsystem: Subsystem.Unknown,
        repros: [],
        version: "",
        feedback_ids: [],
        status: InProgress { state: "in_progress", pr: pr_url(snap.number) },
        comments: [],
        resolution_plan: null,
        difficulty: Difficulty.Medium,
        design_doc: null,
    }
}

function pr_url(number: int) -> string {
    "https://github.com/" + repo_slug() + "/pull/" + number.to_string()
}

/// How a review round ended, from the outcome.json handle_issue wrote for
/// the branch. A copy is kept per round, since the next round's sandbox
/// wipes the run dir.
function round_outcome(branch: string, round: int) -> string {
    let path = run_dir_for(branch) + "/outcome.json";
    if (!baml.fs.exists(path)) {
        return "agent_stopped";
    };
    let raw = baml.fs.read(path);
    baml.fs.mkdir(merge_dir(branch), baml.fs.MkdirOptions { recursive: true });
    baml.fs.write(merge_dir(branch) + "/round-" + round.to_string() + ".outcome.json", raw);
    let o = baml.json.parse(raw) catch (_) {
        _ => baml.json.parse("{}"),
    };
    baml.json.path_or<string>(o, ".kind", "agent_stopped")
}

function wait_poll() -> null {
    baml.sys.sleep(baml.time.Duration.from_seconds(poll_s())) catch (_) {
        _ => null,
    };
    null
}

/// After a push: waits until the PR's head moved off `old_head`, then one
/// more poll so the new head's check runs are registered.
function wait_for_new_head(pr: string, old_head: string, started: baml.time.Instant) -> null {
    while (true) {
        wait_poll();
        let head = pr_snapshot(pr).head;
        if (head != old_head) {
            return wait_poll();
        };
        if (elapsed_s(started) >= max_wait_s()) {
            log.warn({ "pr": pr, "warning": "head did not move after the push" });
            return null;
        };
    }
    null
}

// ==================== Config ====================

/// Seconds between polls while checks are running.
function poll_s() -> int {
    int.parse(baml.env.get("ATB2_POLL_S") ?? "120") catch (_) {
        _ => 120,
    }
}

/// How long one merge_issue call waits for checks, in total.
function max_wait_s() -> int {
    int.parse(baml.env.get("ATB2_MAX_WAIT_S") ?? "14400") catch (_) {
        _ => 14400,
    }
}

/// Whether a PR from a fork may be worked on. Off by default: the PR's
/// branch is built and tested here, and the agent runs Bash in it.
function forks_allowed() -> bool {
    (baml.env.get("ATB2_ALLOW_FORKS") ?? "0") == "1"
}

/// GitHub logins whose review comments are acted on. Humans are not on the
/// list by default: their comments wait for a human.
function reviewers() -> string[] {
    (baml.env.get("ATB2_REVIEWERS") ?? "coderabbitai[bot]").split(",")
        .map((s: string) -> string {
            s.trim()
        })
        .filter((s: string) -> bool {
            s.length() > 0
        })
}

// ==================== The PR, as seen through gh ====================

/// One row of `gh pr checks --json name,bucket,link`.
class CheckRow {
    name: string,
    /// pass | fail | pending | skipping | cancel
    bucket: string,
    link: string?,
}

/// A reviewer comment: inline (path + line) or a review body (path null).
class ReviewComment {
    id: int,
    login: string,
    path: string?,
    line: int?,
    body: string,
}

class PrSnapshot {
    number: int,
    title: string,
    body: string,
    branch: string,
    /// The branch lives in another repository (a fork).
    cross_repo: bool,
    /// The head commit: how a push is seen to have landed.
    head: string,
    /// OPEN | CLOSED | MERGED
    state: string,
    merged: bool,
    checks: CheckRow[],
    pending_checks: int,
    comments: ReviewComment[],
}

function gh(args: string[]) -> CmdResult {
    run_with(Cmd { program: "gh", args: args, cwd: atb2_home(), timeout_ms: 120000 }, sandbox_env(true))
}

/// `gh pr view`, retried: a run polls for hours and one dropped request
/// must not end it.
function gh_view(url: string) -> CmdResult {
    let attempt = 0;
    while (true) {
        let view = gh(
            [
                "pr",
                "view",
                url,
                "--json",
                "number,title,body,headRefName,headRefOid,isCrossRepository,state,mergedAt",
            ],
        );
        attempt = attempt + 1;
        if (view.ok || attempt >= 3) {
            return view;
        };
        log.warn({ "pr": url, "gh_pr_view_retry": attempt, "stderr": tail(view.stderr, 2) });
        baml.sys.sleep(baml.time.Duration.from_seconds(10)) catch (_) {
            _ => null,
        };
    }
    baml.sys.panic("unreachable")
}

function pr_snapshot(url: string) -> PrSnapshot {
    //# title, body, branch, head, state
    let view = gh_view(url);
    if (!view.ok) {
        baml.sys.panic("gh pr view failed: " + tail(view.stderr, 3));
    };
    let v = baml.json.parse(view.stdout);
    let number = baml.json.path_or<int>(v, ".number", 0);
    //# checks (gh exits non-zero when any fail; the JSON is still there)
    let checks = parse_checks(gh(["pr", "checks", url, "--json", "name,bucket,link"]).stdout);
    //# reviewer comments: inline review comments + reviews that request changes
    // a review that merely comments (CodeRabbit's walkthrough after every
    // push) is not feedback; counting it would keep the loop going forever
    let pulls = "repos/" + repo_slug() + "/pulls/" + number.to_string();
    let comments = parse_comments(
        gh(
            [
                "api",
                pulls + "/comments?per_page=100",
                "--jq",
                "[.[] | {id, login: .user.login, path, line, body}]",
            ],
        ).stdout,
    )
        .concat(
            parse_comments(
                gh(
                    [
                        "api",
                        pulls + "/reviews?per_page=100",
                        "--jq",
                        "[.[] | select(.state == \"CHANGES_REQUESTED\" and .body != \"\") | {id, login: .user.login, path: null, line: null, body}]",
                    ],
                ).stdout,
            ),
        );
    PrSnapshot {
        number: number,
        title: baml.json.path_or<string>(v, ".title", ""),
        body: baml.json.path_or<string>(v, ".body", ""),
        branch: baml.json.path_or<string>(v, ".headRefName", ""),
        head: baml.json.path_or<string>(v, ".headRefOid", ""),
        cross_repo: baml.json.path_or<bool>(v, ".isCrossRepository", true),
        state: baml.json.path_or<string>(v, ".state", "OPEN"),
        merged: baml.json.path_or<string>(v, ".mergedAt", "").length() > 0,
        checks: checks,
        pending_checks: checks
            .filter((c: CheckRow) -> bool {
                c.bucket == "pending"
            })
            .length(),
        comments: comments,
    }
}

function parse_checks(s: string) -> CheckRow[] {
    if (s.trim().length() == 0) {
        return [];
    };
    baml.json.from_string<CheckRow[]>(s) catch (_) {
        _ => [],
    }
}

function parse_comments(s: string) -> ReviewComment[] {
    if (s.trim().length() == 0) {
        return [];
    };
    baml.json.from_string<ReviewComment[]>(s) catch (_) {
        _ => [],
    }
}

/// The GitHub Actions run id in a check's link
/// (.../actions/runs/<id>/job/<job>), if it has one.
function run_id_of(link: string?) -> string? {
    let l = link ?? "";
    let marker = "/actions/runs/";
    let i = l.index_of(marker);
    if (i == null) {
        return null;
    };
    let rest = l.slice((i ?? 0) + marker.length(), l.length());
    let id = rest.split("/").at(0) ?? "";
    // digits only: the link is set by whoever posted the check, and the id
    // becomes a gh argument
    let digits = id.chars().filter((c: string) -> bool {
        c >= "0" && c <= "9"
    });
    if (id.length() == 0 || digits.length() != id.length()) {
        null
    } else {
        id
    }
}

/// The log of every failing check, by check name, via `gh run view
/// --log-failed`: the tail of each failed step, so the failing test names
/// are not lost under a long final step.
function failed_logs(fb: PrFeedback) -> map<string, string> {
    let logs: map<string, string> = {};
    for (let c in fb.checks) {
        let id = run_id_of(c.link);
        let log = match (id) {
            null => "(no actions run linked)",
            let i: string => {
                failed_steps_tail(gh(["run", "view", i, "--repo", repo_slug(), "--log-failed"]).stdout, 80)
            },
        };
        logs.set(c.name, log);
    }
    logs
}

/// `--log-failed` prints lines as `<job>\t<step>\t<text>`; the last `n`
/// lines of each step, in order.
function failed_steps_tail(log: string, n: int) -> string {
    let steps: string[] = [];
    let lines: map<string, string[]> = {};
    for (let l in log.split("\n")) {
        let cols = l.split("\t");
        let step = if (cols.length() >= 3) {
            (cols.at(0) ?? "") + " / " + (cols.at(1) ?? "")
        } else {
            ""
        };
        if (!steps.includes(step)) {
            steps.push(step);
        };
        lines.set(step, (lines.get(step) ?? []).concat([l]));
    }
    steps
        .map((st: string) -> string {
            let ls = lines.get(st) ?? [];
            let keep = if (ls.length() > n) {
                ls.slice(ls.length() - n, ls.length())
            } else {
                ls
            };
            keep.join("\n")
        })
        .join("\n")
}

// ==================== What is new since the last round ====================

/// Persisted per branch so a comment is put to the agent once.
class MergeState {
    rounds: int,
    /// Reviewer comment ids already put to the agent.
    handled_ids: int[],
}

/// Outside the run dir, which each round's sandbox wipes.
function merge_dir(branch: string) -> string {
    atb2_home() + "/merge/" + branch_dir_name(branch)
}

function merge_state_path(branch: string) -> string {
    merge_dir(branch) + "/state.json"
}

function read_merge_state(branch: string) -> MergeState {
    let path = merge_state_path(branch);
    if (!baml.fs.exists(path)) {
        return MergeState { rounds: 0, handled_ids: [] };
    };
    baml.json.from_string<MergeState>(baml.fs.read(path)) catch (_) {
        _ => MergeState { rounds: 0, handled_ids: [] },
    }
}

function write_merge_state(branch: string, s: MergeState) -> null {
    baml.fs.mkdir(merge_dir(branch), baml.fs.MkdirOptions { recursive: true });
    baml.fs.write(merge_state_path(branch), baml.json.stringify_pretty(s.to_json()));
    null
}

class PrFeedback {
    checks: CheckRow[],
    comments: ReviewComment[],
}

/// Failing checks (only once checks have settled) and reviewer comments
/// not yet handled. Pure, unit-tested. merge_issue only calls it once
/// checks have settled; the guard here keeps it honest on its own.
function pending_feedback(snap: PrSnapshot, state: MergeState) -> PrFeedback {
    let who = reviewers();
    //# failing checks, once every check has settled
    let failing = if (snap.pending_checks > 0) {
        []
    } else {
        snap.checks.filter((c: CheckRow) -> bool {
            c.bucket == "fail"
        })
    };
    //# reviewer comments not yet put to the agent
    let fresh = snap.comments.filter((c: ReviewComment) -> bool {
        who.includes(c.login) && !state.handled_ids.includes(c.id)
    });
    PrFeedback { checks: failing, comments: fresh }
}

/// The brief handle_issue gets as its plan: each failing check with its
/// log tail, each comment with where it points.
function render_feedback(fb: PrFeedback, logs: map<string, string>) -> string {
    let parts: string[] = [];
    //# one section per failing check, with its log
    for (let c in fb.checks) {
        parts.push(
            "### Failing check: " + c.name + "\n```\n" + (logs.get(c.name) ?? "(no log)") + "\n```",
        );
    }
    //# one section per comment, with where it points
    for (let c in fb.comments) {
        let where = match (c.path) {
            null => "review summary",
            let p: string => {
                p
                    + (
                        match (c.line) {
                            null => "",
                            let l: int => ":" + l.to_string(),
                        }
                    )
            },
        };
        parts.push(
            "### Comment "
                + c.id.to_string()
                + " ("
                + c.login
                + ") at "
                + where
                + "\n"
                + c.body.trim(),
        );
    }
    parts.join("\n\n")
}

// ==================== Unit tests (token-free) ====================

function snap(checks: CheckRow[], comments: ReviewComment[]) -> PrSnapshot {
    PrSnapshot {
        number: 1,
        title: "fix(compiler): x",
        body: "Fixes #1",
        branch: "agent/gh-1-x",
        head: "abc123",
        cross_repo: false,
        state: "OPEN",
        merged: false,
        checks: checks,
        pending_checks: checks
            .filter((c: CheckRow) -> bool {
                c.bucket == "pending"
            })
            .length(),
        comments: comments,
    }
}

function bot(id: int, path: string?, body: string) -> ReviewComment {
    ReviewComment { id: id, login: "coderabbitai[bot]", path: path, line: 12, body: body }
}

testset "merge_issue" {
    test "gh check rows parse; a failed gh call yields no rows" {
        let rows = parse_checks(
            `[{"name":"clippy","bucket":"fail","link":"https://github.com/BoundaryML/baml/actions/runs/123/job/456"},{"name":"fmt","bucket":"pass","link":null}]`,
        );
        assert.equal(rows.length(), 2);
        assert.equal(rows.at(0)?.bucket ?? "", "fail");
        assert.equal(parse_checks("").length(), 0);
        assert.equal(parse_checks("not json").length(), 0);
    }

    test "the actions run id comes out of the check link" {
        assert.equal(
            run_id_of("https://github.com/BoundaryML/baml/actions/runs/123/job/456") ?? "",
            "123",
        );
        assert.equal(run_id_of("https://example.com/status") == null, true);
        assert.equal(run_id_of(null) == null, true);
        // a link is whatever the check's author posted: only a numeric id is used
        assert.equal(run_id_of("https://github.com/x/y/actions/runs/--repo/1") == null, true);
        assert.equal(run_id_of("https://github.com/x/y/actions/runs/12abc/job/1") == null, true);
    }

    test "reviewer comments parse with and without a path" {
        let cs = parse_comments(
            `[{"id":10,"login":"coderabbitai[bot]","path":"crates/x/src/lib.rs","line":40,"body":"Use \`?\`."},{"id":11,"login":"coderabbitai[bot]","path":null,"line":null,"body":"Summary"}]`,
        );
        assert.equal(cs.length(), 2);
        assert.equal(cs.at(1)?.path == null, true);
        assert.equal(cs.at(0)?.line ?? 0, 40);
    }

    test "pending feedback: failing checks only once checks settle" {
        let s = snap(
            [
                CheckRow { name: "clippy", bucket: "fail", link: null },
                CheckRow { name: "nextest", bucket: "pending", link: null },
            ],
            [],
        );
        let fresh = MergeState { rounds: 0, handled_ids: [] };
        assert.equal(pending_feedback(s, fresh).checks.length(), 0);
        let settled = snap([CheckRow { name: "clippy", bucket: "fail", link: null }], []);
        assert.equal(pending_feedback(settled, fresh).checks.length(), 1);
    }

    test "pending feedback: only reviewers on the list, only once" {
        let s = snap(
            [],
            [
                bot(1, "a.rs", "x"),
                bot(2, null, "y"),
                ReviewComment { id: 3, login: "a-human", path: "a.rs", line: 1, body: "z" },
            ],
        );
        let fresh = MergeState { rounds: 0, handled_ids: [] };
        assert.equal(
            pending_feedback(s, fresh).comments.map((c: ReviewComment) -> int {
                c.id
            }),
            [1, 2],
        );
        let later = MergeState { rounds: 1, handled_ids: [1] };
        assert.equal(
            pending_feedback(s, later).comments.map((c: ReviewComment) -> int {
                c.id
            }),
            [2],
        );
    }

    test "the brief names each check and comment" {
        let fb = PrFeedback {
            checks: [CheckRow { name: "clippy", bucket: "fail", link: null }],
            comments: [bot(7, "crates/x/src/lib.rs", "Use `?` here."), bot(8, null, "LGTM otherwise")],
        };
        let brief = render_feedback(fb, { "clippy": "error: unused import" });
        assert.contains(brief, "### Failing check: clippy");
        assert.contains(brief, "error: unused import");
        assert.contains(brief, "### Comment 7 (coderabbitai[bot]) at crates/x/src/lib.rs:12");
        assert.contains(brief, "### Comment 8 (coderabbitai[bot]) at review summary");
    }

    test "the PR becomes an issue handle_issue can take" {
        let i = issue_from_pr(snap([], []));
        assert.equal(i.id, "PR-1");
        assert.equal(i.title, "fix(compiler): x");
        assert.equal(i.difficulty ?? Difficulty.Hard, Difficulty.Medium);
        assert.is_true(pr_url(1).ends_with("/pull/1"));
        // without a review, handle_issue would refuse it: the PR is not open work
        assert.equal(handle_ready(i, false) ?? "go", "not open");
    }

    test "the brief reaches the fix prompt as the plan" {
        let p = implement_fix@render_prompt(issue_from_pr(snap([], [])), "agent/gh-1-x", "a Rust test", "### Failing check: clippy")
            .text();
        assert.contains(p, "### Failing check: clippy");
        assert.contains(p, "fix(compiler): x");
    }

    test "merge state round-trips outside the run dir" {
        let branch = "agent/test-merge-state-" + baml.id.new();
        assert.equal(read_merge_state(branch).rounds, 0);
        write_merge_state(branch, MergeState { rounds: 2, handled_ids: [5, 6] });
        let s = read_merge_state(branch);
        assert.equal(s.rounds, 2);
        assert.equal(s.handled_ids, [5, 6]);
        assert.is_true(!merge_state_path(branch).starts_with(run_dir_for(branch)));
        baml.fs.remove_dir_all(merge_dir(branch));
    }

    test "a round's outcome is read back and kept per round" {
        let branch = "agent/test-round-outcome-" + baml.id.new();
        // no outcome.json: the agent never got as far as writing one
        assert.equal(round_outcome(branch, 1), "agent_stopped");
        baml.fs.mkdir(run_dir_for(branch), baml.fs.MkdirOptions { recursive: true });
        baml.fs.write(run_dir_for(branch) + "/outcome.json", `{"kind":"gate_failed"}`);
        assert.equal(round_outcome(branch, 2), "gate_failed");
        assert.is_true(baml.fs.exists(merge_dir(branch) + "/round-2.outcome.json"));
        baml.fs.remove_dir_all(run_dir_for(branch));
        baml.fs.remove_dir_all(merge_dir(branch));
    }

    test "the failed log keeps the tail of every failed step" {
        let log = "build\tcompile\terror A\nbuild\tcompile\terror B\nbuild\ttest\tFAIL x\nbuild\ttest\tline 2\nbuild\ttest\tline 3";
        let t = failed_steps_tail(log, 2);
        assert.contains(t, "error A");
        assert.contains(t, "error B");
        assert.contains(t, "line 3");
        assert.is_true(!t.includes("FAIL x"));
    }

    test "the review comment says what was addressed and asks CodeRabbit to resolve" {
        let note = review_comment(sample_report());
        assert.is_true(note.starts_with("atb2 review round"));
        assert.contains(note, "Guarded the type-ref lookup.");
        assert.contains(note, "@coderabbitai resolve");
    }

    test "config defaults" {
        assert.equal(reviewers(), ["coderabbitai[bot]"]);
        assert.equal(forks_allowed(), false);
        assert.is_true(poll_s() > 0 && max_wait_s() > poll_s());
    }
}
