// PR -> green -> merged. Give it a PR; it watches the PR until it merges
// or closes. Each time CI fails or the reviewer (CodeRabbit by default)
// leaves comments, it investigates and hands a scoped fix plan to handle_issue
// for implementation, scanning and a push. It never merges a PR.
// Stops when the PR merges or closes, when it is green and nothing is new,
// or after `max_rounds`.
//
//   cd tools/atb2 && baml run -e 'merge_issue("https://github.com/BoundaryML/baml/pull/4634")'
//   ATB2_REVIEWERS=coderabbitai[bot],sxlijin   # whose comments are acted on
//   PRs from forks are refused: the sandbox and the push know origin only.
//   ATB2_POLL_S=120  ATB2_MAX_WAIT_S=14400
//
// Works on any PR of the repo, not only the ones handle_issue opened.
// Audit trail: ~/.atb2/merge/<branch>/state.json (what has been addressed)
// and round-<n>.outcome.json (how each round ended), plus handle_issue's
// review.md / fix.jsonl / outcome.json for the latest round.

// ==================== Pipeline ====================

/// How a merge_issue call ended.
class MergeResult {
    /// merged: the PR merged. green: checks pass or only confirmed workflow blockers remain
    /// (workflow_changes_needed explains the exception), and nothing else is new; waiting
    /// on humans. closed: closed without merging. waiting_on_checks: gave
    /// up waiting for checks (ATB2_MAX_WAIT_S). out_of_rounds: `max_rounds`
    /// spent with feedback still pending. dry_run: one round, nothing pushed.
    /// round_failed: a round ended without a push (gate failed, agent
    /// stopped); nothing on the PR changed, so a human takes it from here.
    /// fork_refused: the PR's branch lives in another repository. Its code
    /// would run on this host, and the sandbox and the push only know
    /// origin, so a fork's branch is never touched.
    kind: "merged"
        | "green"
        | "closed"
        | "waiting_on_checks"
        | "out_of_rounds"
        | "dry_run"
        | "round_failed"
        | "fork_refused"
        | "awaiting_approval",
    pr: string,
    rounds: int,
    workflow_changes_needed: string?,
    workflow_fingerprint: string?,
}

//#merge_issue(pr: string, mode: HandleMode = HandleMode.Live, max_rounds: int = 7, issue_id: string? = null, thread: SlackRef? = null, mention: string? = null) -> MergeResult
function round_text(round: int, kind: string, fb: PrFeedback) -> string {
    (
        if (kind == "fixed") {
            ":arrows_counterclockwise: "
        } else {
            ":warning: "
        }
    )
        + "round "
        + round.to_string()
        + ": "
        + fb.checks.length().to_string()
        + " failing check"
        + (
            if (fb.checks.length() == 1) {
                ""
            } else {
                "s"
            }
        )
        + ", "
        + fb.comments.length().to_string()
        + " reviewer comment"
        + (
            if (fb.comments.length() == 1) {
                ""
            } else {
                "s"
            }
        )
        + " → "
        + kind
        + (
            if (kind == "fixed") {
                ", pushed"
            } else {
                ", nothing pushed"
            }
        )
}

/// The PR as an Issue, so handle_issue can work on it: the PR's title and
/// body are the ticket; the status carries the PR URL for the outcome.
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
        slack_ts: null,
        slack_channel: null,
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

/// The PR as gh sees it, or null when gh cannot read it after retries: the
/// caller ends the run as round_failed instead of taking the process down.
function pr_snapshot(url: string) -> PrSnapshot? {
    //# title, body, branch, head, state
    let view = gh_view(url);
    if (!view.ok) {
        log.error({ "pr": url, "gh_pr_view": tail(view.stderr, 3) });
        return null;
    };
    let v = baml.json.parse(view.stdout);
    let number = baml.json.path_or<int>(v, ".number", 0);
    // Failed or malformed API reads are unknown, never evidence of a green PR.
    let check_result = gh(["pr", "checks", url, "--json", "name,bucket,link"]);
    let checks = parse_checks(check_result.stdout);
    if (![0, 1, 8].includes(check_result.exit_code)) {
        return null;
    };
    let pulls = "repos/" + repo_slug() + "/pulls/" + number.to_string();
    let inline = gh(["api", "--paginate", "--slurp", pulls + "/comments?per_page=100"]);
    let reviews = gh(["api", "--paginate", "--slurp", pulls + "/reviews?per_page=100"]);
    if (!inline.ok || !reviews.ok) {
        return null;
    };
    let comments = parse_comments(inline.stdout).concat(parse_comments(reviews.stdout, reviews = true));
    // Checks and comments were fetched separately. Discard a mixed-head view.
    let after = gh_view(url);
    if (
        !after.ok
            || baml.json.path_or<string>(baml.json.parse(after.stdout), ".headRefOid", "")
                != baml.json.path_or<string>(v, ".headRefOid", "")
    ) {
        return null;
    };
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
    baml.json.from_string<CheckRow[]>(s) catch (_) {
        _ => {
            throw StoreError { message: "GitHub check data unavailable" };
        },
    }
}

class GitHubReviewAuthor {
    login: string,
}

class GitHubReviewRow {
    id: int,
    user: GitHubReviewAuthor,
    body: string,
    path: string?,
    line: int?,
    state: string?,
}

/// gh --paginate --slurp returns an array of pages. Decode every page here;
/// gh does not support combining --slurp with --jq.
function parse_comments(s: string, reviews: bool = false) -> ReviewComment[] {
    let pages = baml.json.from_string<GitHubReviewRow[][]>(s) catch (_) {
        _ => {
            throw StoreError { message: "GitHub review data unavailable" };
        },
    };
    let comments: ReviewComment[] = [];
    for (let page in pages) {
        for (let row in page) {
            if (reviews && (row.state != "CHANGES_REQUESTED" || row.body == "")) {
                continue;
            };
            comments.push(
                ReviewComment {
                    id: row.id,
                    login: row.user.login,
                    path: if (reviews) {
                        null
                    } else {
                        row.path
                    },
                    line: if (reviews) {
                        null
                    } else {
                        row.line
                    },
                    body: row.body,
                },
            );
        }
    }
    comments
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
    let seen_runs: string[] = [];
    for (let c in fb.checks) {
        let id = run_id_of(c.link);
        let log = match (id) {
            null => "(no actions run linked)",
            let i: string => {
                if (seen_runs.includes(i)) {
                    "(failure logs included above for workflow run " + i + ")"
                } else {
                    seen_runs.push(i);
                    failed_steps_tail(gh(["run", "view", i, "--repo", repo_slug(), "--log-failed"]).stdout, 80)
                }
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
    let failing = if (snap.pending_checks > 0 || snap.checks.length() == 0) {
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

// ==================== The request queue ====================
// Slack intake inserts a babysit_requests row. The merge loop in the Fly
// runner process claims it and runs merge_issue with the Slack thread attached.
// `request_merge(pr)` queues one by hand.

/// One queued request, as the Slack ingress wrote it.
class MergeRequest {
    id: int,
    pr: string,
    channel: string,
    thread_ts: string,
    requested_by: string?,
    status: string,
}

/// The closing line of a merge_issue thread.
function result_text(r: MergeResult) -> string {
    let rounds = r.rounds.to_string()
        + (
            if (r.rounds == 1) {
                " round"
            } else {
                " rounds"
            }
        );
    match (r.kind) {
        "merged" => ":white_check_mark: merged after " + rounds + ": " + r.pr,
        "green" => {
            if (r.workflow_changes_needed != null) {
                ":large_green_circle: Green, with workflow changes needed: "
                    + r.pr
                    + "\n"
                    + (r.workflow_changes_needed ?? "")
                    + "\nGitHub checks may still be red; only workflow blockers remain. No workflow fixes were attempted."
            } else {
                ":large_green_circle: green after "
                    + rounds
                    + ", no unaddressed configured-reviewer feedback; still watching, waiting on a human to merge "
                    + r.pr
            }
        },
        "closed" => ":no_entry_sign: closed without merging: " + r.pr,
        "out_of_rounds" => {
            ":hourglass: out of rounds ("
                + rounds
                + ") with feedback still pending on "
                + r.pr
                + "; a human takes it from here"
        },
        "round_failed" => {
            ":x: a round ended without a push (gate failed or the agent stopped) on "
                + r.pr
                + "; see ~/.atb2/merge for the outcome"
        },
        "awaiting_approval" => "Historical run paused; request a new run for " + r.pr,
        "waiting_on_checks" => ":hourglass: gave up waiting for checks on " + r.pr,
        "dry_run" => ":test_tube: dry run done on " + r.pr + ", nothing pushed",
        "fork_refused" => {
            ":no_entry: "
                + r.pr
                + " comes from a fork; its branch is not built or pushed from this host"
        },
    }
}

// ==================== The queue ====================

/// Queues a request by hand, the way the Slack ingress does for a mention.
/// `thread` is where to narrate; null = no Slack, the result is only stored.
function claim_merge_request(runner: string) -> MergeRequest? {
    let rows = select(
        "babysit_requests",
        "select=id,pr,channel,thread_ts,requested_by,status&status=eq.queued&dataset="
            + eq(dataset())
            + "&order=finished_at.asc.nullsfirst,created_at&limit=1",
    );
    let r = match (rows.at(0)) {
        null => {
            return null;
        },
        let j: json => request_from_row(j),
    };
    let claimed = supabase_send(
        "PATCH",
        "babysit_requests?id=eq." + r.id.to_string() + "&status=eq.queued",
        baml.json.stringify(
            { "status": "running", "claimed_by": runner, "claimed_at": "now()" }.to_json(),
        ),
        "return=representation",
    );
    if (rows_of(claimed).length() == 0) {
        // another runner got it first
        return null;
    };
    r
}

function request_from_row(j: json) -> MergeRequest {
    MergeRequest {
        id: baml.json.path_or<int>(j, ".id", 0),
        pr: baml.json.path_or<string>(j, ".pr", ""),
        channel: baml.json.path_or<string>(j, ".channel", ""),
        thread_ts: baml.json.path_or<string>(j, ".thread_ts", ""),
        requested_by: baml.json.path_or<string?>(j, ".requested_by", null),
        status: baml.json.path_or<string>(j, ".status", "queued"),
    }
}

/// Marks a request finished, but only while this runner still owns it: a
/// request reclaimed by another runner after a stale lease is theirs now,
/// and this result is dropped with a warning rather than overwriting.
function finish_merge_request(id: int, runner: string, status: string, result: json) -> null {
    let rows = rows_of(
        supabase_send(
            "PATCH",
            "babysit_requests?id=eq."
                + id.to_string()
                + "&claimed_by="
                + eq(runner)
                + "&status=eq.running",
            baml.json.stringify(
                { "status": status, "result": result, "finished_at": "now()" }.to_json(),
            ),
            "return=representation",
        ),
    );
    if (rows.length() == 0) {
        log.warn({ "request": id, "runner": runner, "warning": "no longer ours; result dropped" });
    };
    null
}

/// Requests left `running` longer than `after_s` (a runner died) go back to
/// the queue.
function reclaim_stale_requests(after_s: int) -> int {
    let stale = select(
        "babysit_requests",
        "select=id,claimed_by&status=eq.running&dataset="
            + eq(dataset())
            + "&claimed_at=lt."
            + url_encode(iso_seconds_ago(after_s)),
    );
    for (let r in stale) {
        // the requeue names the stale owner, so a runner that finished in
        // the meantime (status no longer running) is left alone
        update(
            "babysit_requests",
            "id=eq."
                + baml.json.path_or<int>(r, ".id", 0).to_string()
                + "&status=eq.running&claimed_by="
                + eq(baml.json.path_or<string>(r, ".claimed_by", "")),
            { "status": "queued", "claimed_by": null, "claimed_at": null }.to_json(),
        );
    }
    stale.length()
}

/// `now - s` as an ISO instant, for a PostgREST timestamp filter.
function iso_seconds_ago(s: int) -> string {
    baml.time.Instant.now().as<baml.ops.Subtract<baml.time.Duration, Output = baml.time.Instant>>
        .sub(baml.time.Duration.from_seconds(s))
        .to_string()
}

/// The thread a request came from, or null when it was queued without Slack.
function merge_request_thread(r: MergeRequest) -> SlackRef? {
    if (r.channel.length() == 0 || r.thread_ts.length() == 0) {
        null
    } else {
        SlackRef { channel: r.channel, ts: r.thread_ts }
    }
}

// ==================== The runner ====================

//#merge_issue_loop(once: bool = false, mode: HandleMode = HandleMode.Live, reclaim_after_s: int = 21600) -> int
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

class BabysitProposal {
    id: string,
    pr: string,
    head: string,
    fingerprint: string,
    summary: string,
    plan: string,
    status: string,
    requested_by: string?,
    channel: string?,
    slack_ts: string?,
}

function proposal_from_row(row: json) -> BabysitProposal {
    baml.json.from_json<BabysitProposal>(row)
}

function babysit_fingerprint(snap: PrSnapshot, fb: PrFeedback) -> string {
    let hash = baml.crypto.Sha256.new();
    hash.update(
        (
            "workflow-policy-v1\n"
                + snap.head
                + "\n"
                + baml.json.stringify(fb.to_json())
                + "\n"
                + baml.json.stringify(snap.checks.to_json())
        )
            .to_utf8(),
    );
    hash.finish().to_hex()
}

class WorkflowOnly {
    /// Public summary naming every remaining blocker, evidence that it is workflow-only,
    /// and the workflow changes a human still needs to make. Never include secrets or raw logs.
    workflow_changes_needed: string,
}

function proposal_current(p: BabysitProposal, head: string, fingerprint: string) -> bool {
    p.head == head && p.fingerprint == fingerprint
}

/// Plan and claim one automatic round on this exact PR head. Claims are not approvals.
function automatic_fix_path(id: string) -> string {
    if (
        id.length() == 0
            || id.length() > 100
            || !id.chars().every((c: string) -> bool {
                c.is_ascii_alphanumeric() || c == "_" || c == "-"
            })
    ) {
        throw StoreError { message: "invalid automatic fix id" };
    };
    atb2_home() + "/automatic-fixes/" + id + ".json"
}

function finish_automatic_fix(id: string, status: string) -> null {
    let path = automatic_fix_path(id);
    let row = baml.json.from_string<map<string, json>>(baml.fs.read(path));
    let fix = baml.json.from_json<map<string, json>>(row.get("fix") ?? baml.json.parse("{}"));
    fix.set("status", status.to_json());
    row.set("fix", fix.to_json());
    baml.fs.write(path, baml.json.stringify(row.to_json()));
    null
}

/// A live request and exact head are required; no approval identity is involved.
function babysit_push_allowed(id: string, head: string) -> bool {
    if (id.length() == 0 || head.length() != 40) {
        return false;
    };
    let path = automatic_fix_path(id);
    if (!baml.fs.exists(path)) {
        return false;
    };
    let row = baml.json.parse(baml.fs.read(path));
    if (
        baml.json.path_or<string>(row, ".fix.head", "") != head
            || baml.json.path_or<string>(row, ".fix.status", "") != "executing"
    ) {
        return false;
    };
    if (baml.json.path_or<string>(row, ".dataset", "") != dataset()) {
        return false;
    };
    let request = baml.json.path_or<int>(row, ".request_id", 0);
    select(
        "babysit_requests",
        "select=id&id=eq."
            + request.to_string()
            + "&status=eq.running&dataset="
            + eq(dataset())
            + "&limit=1",
    )
        .length()
        == 1
}

/// One queue for issue-created PRs and direct babysit requests. Failed/terminal
/// requests need a new explicit request; pending approvals retain their owner.
function request_blocks_babysit(row: json, proposals: json[]) -> bool {
    if (
        baml.json.path_or<string>(row, ".result.kind", "") != "awaiting_approval"
            || baml.json.path_or<string>(row, ".status", "") != "done"
    ) {
        return true;
    };
    let id = baml.json.path_or<int>(row, ".id", 0);
    proposals
        .filter((p: json) -> bool {
            baml.json.path_or<int>(p, ".request_id", -1) == id
                && ["pending", "approved", "executing"].includes(
                    baml.json.path_or<string>(p, ".status", ""),
                )
        })
        .length()
        > 0
}

function active_babysit_request(pr: string) -> int? {
    let rows = select(
        "babysit_requests",
        "select=id,status,result&pr="
            + eq(pr)
            + "&dataset="
            + eq(dataset())
            + "&or=(status.in.(queued,running),and(status.eq.done,result->>kind.in.(green,waiting_on_checks,awaiting_approval)))&order=id&limit=101",
    );
    if (rows.length() > 100) {
        throw StoreError { message: "too many active babysit requests; refusing ambiguous ownership" };
    };
    let proposals = select(
        "babysit_proposals",
        "select=request_id,status&pr="
            + eq(pr)
            + "&dataset="
            + eq(dataset())
            + "&status=in.(pending,approved,executing)&limit=101",
    );
    if (proposals.length() > 100) {
        throw StoreError { message: "too many active proposals; refusing ambiguous ownership" };
    };
    for (let row in rows) {
        if (request_blocks_babysit(row, proposals)) {
            return baml.json.path_or<int>(row, ".id", 0);
        };
    }
    null
}

function resumable_babysit(kind: string) -> bool {
    kind == "green" || kind == "waiting_on_checks"
}

function recover_and_claim_babysit(runner: string, reclaim_after_s: int) -> MergeRequest? {
    reclaim_stale_requests(reclaim_after_s);
    // Keep observing standalone PRs too. Preserve finished_at for fair rotation.
    update(
        "babysit_requests",
        "status=eq.done&result->>kind=in.(green,waiting_on_checks)&dataset=" + eq(dataset()),
        { "status": "queued", "claimed_by": null, "claimed_at": null }.to_json(),
    );
    let claimed = claim_merge_request(runner);
    if (claimed != null) {
        let r = claimed ?? baml.sys.panic("checked request");
        let canonical = active_babysit_request(r.pr);
        if (canonical != null && canonical != r.id) {
            finish_merge_request(r.id, runner, "done", { "kind": "duplicate", "request_id": canonical }.to_json());
            return null;
        };
    };
    claimed
}

function issue_id_for_pr(pr: string) -> string? {
    if (!db_configured()) {
        return null;
    };
    let rows = select("issues", "select=id&status->>pr=" + eq(pr) + "&dataset=" + eq(dataset()) + "&limit=2");
    if (rows.length() != 1) {
        null
    } else {
        baml.json.path_or<string>(rows.at(0) ?? baml.json.parse("{}"), ".id", "")
    }
}

/// Public lifecycle metadata and the explicitly public proposed-fix summary.
/// Full implementation plans, raw CI diagnostics and transcripts stay private.
function record_pr_event(pr: string, kind: string, details: json, thread: SlackRef?) -> null {
    if (!db_configured()) {
        return null;
    };
    let payload = baml.json.from_json<map<string, json>>(details);
    payload.set("pr", pr.to_json());
    let previous = select(
        "events",
        "select=kind,payload&payload->>pr="
            + eq(pr)
            + "&dataset="
            + eq(dataset())
            + "&order=created_at.desc,id.desc&limit=1",
    );
    let last = previous.at(0) ?? baml.json.parse("{}");
    if (
        baml.json.path_or<string>(last, ".kind", "") == kind
            && baml.json.stringify(baml.json.path_or<json>(last, ".payload", baml.json.parse("{}")))
                == baml.json.stringify(payload.to_json())
    ) {
        return null;
    };
    log_event(kind, issue_id_for_pr(pr), null, payload.to_json(), thread?.channel, thread?.ts);
    null
}

function pr_activity_link(pr: string) -> string {
    let base = (baml.env.get("ATB2_UI_URL") ?? "").trim();
    if (!base.starts_with("https://")) {
        return pr;
    };
    let number = pr.split("/pull/").at(1) ?? "";
    if (
        number.length() == 0
            || !number.chars().every((c: string) -> bool {
                c >= "0" && c <= "9"
            })
    ) {
        return pr;
    };
    "<" + base + "/prs/" + number + "?dataset=" + dataset() + "|View babysitter activity>"
}

function retire_babysit_proposals(request_id: int) -> null {
    update(
        "babysit_proposals",
        "request_id=eq."
            + request_id.to_string()
            + "&status=in.(pending,approved)&dataset="
            + eq(dataset()),
        { "status": "stale" }.to_json(),
    );
    null
}
