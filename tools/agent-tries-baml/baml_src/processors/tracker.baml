// Cursor PR tracker — a periodic sweep that follows a dispatched fix agent to a
// mergeable PR. Ports services/linear_sync/tracker.py. Runs alongside LinearPush +
// FixDispatch in the linear_sync role (main.baml). NOT a claim loop: it is keyed
// off the issue `status` and uses only generic table verbs, so it is idempotent
// and safe to run every tick.
//
//   tocursor -- poll the agent until it opens a PR -> prprep. If Cursor doesn't
//               surface the prUrl once the run is over, fall back to GitHub and look
//               the PR up by the agent's branch; only a real no-PR -> needs_human.
//   prprep   -- read the PR's mergeability + CI + CodeRabbit state:
//                 merged -> closed; closed-unmerged -> needs_human;
//                 merge conflict -> dispatch a resolver (bounded); CI failing OR
//                 CodeRabbit blocking -> dispatch a refix (bounded) -> needs_human
//                 at the cap; CI passing AND CodeRabbit clear -> pr_ready
//   pr_ready -- a human merge -> closed; PR closed -> needs_human; a fresh merge
//               conflict -> resolve + pull back to prprep; NEW human review comments
//               (newer than the lastHumanCommentAt high-water mark) -> refix -> prprep.
//
// Full port of services/linear_sync/tracker.py. Signature verification on inbound
// webhooks is handled separately (ingress); this sweep only reads/writes via the API.

class TrackIssue {
    _id: string?,
    title: string?,
    status: string?,
    cursorAgentId: string?,
    fixSlackTs: string?,
    prUrl: string?,
    prBranch: string?,
    fixAttempts: float?, // Convex serializes numbers as floats
    lastFixedSha: string?,
    linearIssueId: string?,
    fixThreadTs: string?,
    lastHumanCommentAt: string?, // high-water mark for pr_ready human-review refix
}

// Bound deps for the sweep (built once in main.baml).
class TrackCtx {
    svc: ServiceClient,
    linear: LinearClient,
    cursor_key: string,
    github_token: string,
    model: string,
    slack_token: string,
    fix_channel: string,
    max_attempts: int,
}

class PrLoc {
    owner: string,
    repo: string,
    number: int,
}

// ---- entry point -----------------------------------------------------------

// Sweep forever on the given cadence. A transient sweep failure is swallowed and
// retried next tick rather than killing the loop. No-op when no Cursor key is set.
function run_cursor_tracker(ctx: TrackCtx, interval_ms: int, limit: int) -> int {
    baml.io.println("cursor-tracker starting (every " + baml.unstable.string(interval_ms) + "ms)");
    while (true) {
        if (ctx.cursor_key != "") {
            let _ = tracker_sweep_once(ctx, limit);
        }
        let _ = baml.sys.sleep(baml.time.Duration.from_milliseconds(interval_ms)) catch (_) {
            _ => null
        };
    }
    return 0;
}

// Track every issue currently in tocursor / fixing / prprep / pr_ready once.
// "fixing" is the legacy pre-tracker dispatched state, swept like tocursor.
function tracker_sweep_once(ctx: TrackCtx, limit: int) -> int {
    for (let status in ["tocursor", "fixing", "prprep", "pr_ready"]) {
        let raw = ctx.svc.list("issues", "status", status, "by_status_created", limit) catch (_) {
            _ => "[]"
        };
        let issues: TrackIssue[] = baml.json.from_string<TrackIssue[]>(raw) catch (_) {
            _ => []
        };
        for (let issue in issues) {
            let _ = track_one(ctx, issue);
        }
    }
    return 0;
}

function track_one(ctx: TrackCtx, issue: TrackIssue) -> int {
    let status = match (issue.status) {
        let s: string => s,
        null => "",
    };
    if (status == "tocursor" || status == "fixing") {
        return track_tocursor(ctx, issue);
    }
    if (status == "pr_ready") {
        return track_pr_ready(ctx, issue);
    }
    return track_prprep(ctx, issue);
}

// ---- helpers ---------------------------------------------------------------

// Working agent id, or "" if not real yet (a "pending:" marker isn't dispatched).
function tracker_agent_id(issue: TrackIssue) -> string {
    let aid = match (issue.cursorAgentId) {
        let a: string => a,
        null => match (issue.fixSlackTs) {
            let f: string => f,
            null => "",
        },
    };
    if (aid == "" || aid.starts_with("pending:")) {
        return "";
    }
    return aid;
}

function issue_id_or_empty(issue: TrackIssue) -> string {
    return match (issue._id) {
        let v: string => v,
        null => "",
    };
}

function issue_title_or(issue: TrackIssue) -> string {
    return match (issue.title) {
        let v: string => v,
        null => "(untitled)",
    };
}

function parse_pr_loc(url: string) -> PrLoc? {
    let parts = parse_pr_url(url);
    if (parts.length() < 3) {
        return null;
    }
    let owner = match (parts.at(0)) {
        let v: string => v,
        null => "",
    };
    let repo = match (parts.at(1)) {
        let v: string => v,
        null => "",
    };
    let num_s = match (parts.at(2)) {
        let v: string => v,
        null => "",
    };
    let number = baml.json.from_string<int>(num_s) catch (_) {
        _ => 0
    };
    if (owner == "" || repo == "" || number == 0) {
        return null;
    }
    return PrLoc { owner: owner, repo: repo, number: number };
}

// Post a tracker update into the issue's fix thread (best-effort, no-op w/o Slack).
function tracker_notify(ctx: TrackCtx, issue: TrackIssue, text: string) -> int {
    if (ctx.slack_token == "" || ctx.fix_channel == "") {
        return 0;
    }
    let thread = match (issue.fixThreadTs) {
        let t: string => t,
        null => "",
    };
    let _ = slack_post_message(ctx.slack_token, ctx.fix_channel, text, thread);
    return 0;
}

// Swap the issue's Linear status label. The three ways this silently no-op'd —
// a missing linearIssueId, an unconfigured LINEAR_STATUS_* env, and a swallowed
// Linear API error — are now logged to stderr so a broken ATB->Linear sync is
// diagnosable instead of invisible.
function tracker_set_linear(ctx: TrackCtx, issue: TrackIssue, status_name: string) -> int {
    let iid = issue_id_or_empty(issue);
    let lid = match (issue.linearIssueId) {
        let v: string => v,
        null => "",
    };
    if (lid == "") {
        baml.io
            .eprintln(
                "tracker_set_linear: issue " + iid + " has no linearIssueId; Linear NOT updated ("
                    + status_name + ")",
            );
        return 0;
    }
    let label = linear_status_id(status_name);
    if (label == "") {
        baml.io
            .eprintln(
                "tracker_set_linear: issue " + iid + " -> no LINEAR_STATUS_" + status_name
                    + " configured; Linear NOT updated",
            );
        return 0;
    }
    let _ = ctx.linear.set_status(lid, label) catch (e) {
        _ => {
            baml.io
                .eprintln(
                    "tracker_set_linear: issue " + iid + " linear=" + lid + " set_status("
                        + status_name + ") failed: " + baml.unstable.string(e),
                );
            0
        }
    };
    return 0;
}

function fix_attempts_of(issue: TrackIssue) -> int {
    return match (issue.fixAttempts) {
        let n: float => f2i(n),
        null => 0,
    };
}

// ---- tocursor: wait for the agent's PR -------------------------------------

function track_tocursor(ctx: TrackCtx, issue: TrackIssue) -> int {
    let id = issue_id_or_empty(issue);
    let agent_id = tracker_agent_id(issue);
    if (id == "" || agent_id == "") {
        return 0;
    }
    let pr = match (cursor_pr_for_agent(ctx.cursor_key, agent_id)) {
        let p: CursorPr => p,
        null => {
            return 0;
        },
    };
    let branch = match (pr.branch) {
        let b: string => b,
        null => "",
    };
    // Prefer the prUrl Cursor reports; else, once the run is over, fall back to
    // GitHub and look the PR up by the agent's branch — Cursor often leaves prUrl
    // null even after it opened one, which would otherwise strand a real PR.
    let url = match (pr.pr_url) {
        let u: string => u,
        null => {
            if (pr.run_terminal) {
                tracker_github_pr_fallback(ctx, pr, branch)
            } else {
                ""
            }
        },
    };
    if (url != "") {
        let _ = tracker_to_prprep(ctx, issue, id, url, branch);
        return 0;
    }
    // Run over and no PR on Cursor OR GitHub -> the agent genuinely produced none.
    if (pr.run_terminal) {
        let _ = ctx.svc.transition("issues", id, "needs_human", "status", "{}", true) catch (_) {
            _ => ""
        };
        let _ = tracker_set_linear(ctx, issue, "NEEDS_HUMAN");
        let _ = tracker_notify(
            ctx,
            issue,
            "Cursor agent finished with no PR for *"
                + issue_title_or(issue)
                + "* — needs a human.",
        );
    }
    return 0;
}

// Resolve a PR url via GitHub from the agent's reported repo + branch ("" if none).
function tracker_github_pr_fallback(ctx: TrackCtx, pr: CursorPr, branch: string) -> string {
    if (branch == "") {
        return "";
    }
    let repo_url = match (pr.repo_url) {
        let r: string => r,
        null => "",
    };
    let parts = parse_repo_url(repo_url);
    if (parts.length() < 2) {
        return "";
    }
    let owner = match (parts.at(0)) {
        let v: string => v,
        null => "",
    };
    let repo = match (parts.at(1)) {
        let v: string => v,
        null => "",
    };
    return github_open_pr_for_branch(ctx.github_token, owner, repo, branch);
}

// Advance an issue to prprep with its PR fields (marks the card dirty so LinearPush
// re-renders the body with the PR link). Shared by the Cursor + GitHub-fallback paths.
function tracker_to_prprep(
    ctx: TrackCtx,
    issue: TrackIssue,
    id: string,
    url: string,
    branch: string,
) -> int {
    let num = match (parse_pr_loc(url)) {
        let loc: PrLoc => baml.unstable.string(loc.number),
        null => "0",
    };
    let patch = "{\"prUrl\":"
        + jesc(url)
        + ",\"prBranch\":"
        + jesc(branch)
        + ",\"prNumber\":"
        + num
        + ",\"linearSyncStatus\":\"dirty\"}";
    let _ = ctx.svc.transition("issues", id, "prprep", "status", patch, true) catch (_) {
        _ => ""
    };
    let _ = tracker_set_linear(ctx, issue, "PR_PREP");
    let _ = tracker_notify(ctx, issue, "PR up for *" + issue_title_or(issue) + "*\n" + url);
    return 0;
}

// ---- prprep: read PR mergeability + CI + CodeRabbit, advance / fix / escalate -

function track_prprep(ctx: TrackCtx, issue: TrackIssue) -> int {
    let id = issue_id_or_empty(issue);
    let pr_url = match (issue.prUrl) {
        let u: string => u,
        null => "",
    };
    if (id == "" || pr_url == "") {
        return 0;
    }
    let loc = match (parse_pr_loc(pr_url)) {
        let l: PrLoc => l,
        null => {
            return 0;
        },
    };
    let gh = match (github_pr_state(ctx.github_token, loc.owner, loc.repo, loc.number)) {
        let g: GhPr => g,
        null => {
            return 0;
        },
    };

    // Refresh whether the current agent run has stopped (don't refix mid-push).
    let agent_id = tracker_agent_id(issue);
    let run_terminal = if (agent_id == "") {
        true
    } else {
        match (cursor_pr_for_agent(ctx.cursor_key, agent_id)) {
            let p: CursorPr => p.run_terminal,
            null => true,
        }
    };

    let merged = match (gh.merged) {
        let m: bool => m,
        null => false,
    };
    if (merged) {
        let _ = ctx.svc.transition("issues", id, "closed", "status", "{}", true) catch (_) {
            _ => ""
        };
        let _ = tracker_set_linear(ctx, issue, "MERGED");
        let _ = tracker_notify(ctx, issue, "Merged: *" + issue_title_or(issue) + "*\n" + pr_url);
        return 0;
    }
    let state = match (gh.state) {
        let s: string => s,
        null => "open",
    };
    if (state == "closed") {
        return tracker_pr_closed_unmerged(ctx, issue, pr_url);
    }
    let mstate = match (gh.mergeable_state) {
        let m: string => m,
        null => "",
    };
    // Merge conflict takes priority: a "dirty" PR can't merge even if CI is green.
    if (mstate == "dirty") {
        return tracker_resolve_conflict(ctx, issue, loc, gh, pr_url, run_terminal, false);
    }

    let sha = match (gh.head) {
        let h: GhRef => match (h.sha) {
            let s: string => s,
            null => "",
        },
        null => "",
    };
    let runs = if (sha == "") {
        "[]"
    } else {
        github_check_runs(ctx.github_token, loc.owner, loc.repo, sha) catch (_) {
            _ => "[]"
        }
    };
    let reviews = github_pr_reviews(ctx.github_token, loc.owner, loc.repo, loc.number) catch (_) {
        _ => "[]"
    };
    let ci = ci_state(runs);
    let cr = coderabbit_state(reviews);
    let st_patch = "{\"checkState\":" + jesc(ci) + ",\"coderabbitState\":" + jesc(cr) + "}";
    let _ = ctx.svc.update("issues", id, st_patch) catch (_) {
        _ => ""
    };

    if (ci == "passing" && cr == "clear" && run_terminal) {
        let _ = ctx.svc.transition("issues", id, "pr_ready", "status", "{}", true) catch (_) {
            _ => ""
        };
        let _ = tracker_set_linear(ctx, issue, "READY_TO_MERGE");
        let _ = tracker_notify(
            ctx,
            issue,
            "PR green & CodeRabbit-clear — ready to merge: *"
                + issue_title_or(issue)
                + "*\n"
                + pr_url,
        );
        return 0;
    }

    if (ci == "failing" || cr == "blocking") {
        if (run_terminal == false) {
            return 0; // agent still pushing — let it finish
        }
        let last_sha = match (issue.lastFixedSha) {
            let s: string => s,
            null => "",
        };
        if (sha != "" && sha == last_sha) {
            return 0; // already dispatched a fix for this exact commit
        }
        let attempts = fix_attempts_of(issue);
        if (attempts >= ctx.max_attempts) {
            let _ = ctx.svc.transition("issues", id, "needs_human", "status", "{}", true) catch (_) {
                _ => ""
            };
            let _ = tracker_set_linear(ctx, issue, "NEEDS_HUMAN");
            let _ = tracker_notify(
                ctx,
                issue,
                "*"
                    + issue_title_or(issue)
                    + "* still failing after "
                    + baml.unstable.string(ctx.max_attempts)
                    + " fix attempts — needs a human.\n"
                    + pr_url,
            );
            return 0;
        }
        let reasons = if (ci == "failing" && cr == "blocking") {
            "failing CI checks and CodeRabbit's requested changes"
        } else if (ci == "failing") {
            "failing CI checks"
        } else {
            "CodeRabbit's requested changes"
        };
        let branch = match (issue.prBranch) {
            let b: string => b,
            null => "",
        };
        let prompt = tracker_refix_prompt(issue_title_or(issue), pr_url, branch, reasons);
        let new_id = tracker_dispatch_fix(ctx, loc, pr_url, branch, prompt);
        let patch = "{\"cursorAgentId\":"
            + jesc(new_id)
            + ",\"fixAttempts\":"
            + baml.unstable.string(attempts + 1)
            + ",\"lastFixedSha\":"
            + jesc(sha)
            + "}";
        let _ = ctx.svc.update("issues", id, patch) catch (_) {
            _ => ""
        };
        // Quote the CodeRabbit items being addressed so the thread shows the specifics.
        let cr_previews = if (cr == "blocking") {
            let cmts = github_pr_review_comments(ctx.github_token, loc.owner, loc.repo, loc.number) catch (_) {
                _ => "[]"
            };
            comment_previews(coderabbit_comments(reviews, cmts), 3)
        } else {
            ""
        };
        let _ = tracker_notify(
            ctx,
            issue,
            "Responding to "
                + reasons
                + " on *"
                + issue_title_or(issue)
                + "* (attempt "
                + baml.unstable.string(attempts + 1)
                + "/"
                + baml.unstable.string(ctx.max_attempts)
                + ").\n"
                + cr_previews
                + "PR: "
                + pr_url,
        );
        return 0;
    }
    // else: CI pending or CodeRabbit hasn't reviewed yet -> wait for the next tick
    return 0;
}

// ---- pr_ready: re-check a ready PR for a human merge / close / fresh conflict --

function track_pr_ready(ctx: TrackCtx, issue: TrackIssue) -> int {
    let id = issue_id_or_empty(issue);
    let pr_url = match (issue.prUrl) {
        let u: string => u,
        null => "",
    };
    if (id == "" || pr_url == "") {
        return 0;
    }
    let loc = match (parse_pr_loc(pr_url)) {
        let l: PrLoc => l,
        null => {
            return 0;
        },
    };
    let gh = match (github_pr_state(ctx.github_token, loc.owner, loc.repo, loc.number)) {
        let g: GhPr => g,
        null => {
            return 0;
        },
    };
    let merged = match (gh.merged) {
        let m: bool => m,
        null => false,
    };
    if (merged) {
        let _ = ctx.svc.transition("issues", id, "closed", "status", "{}", true) catch (_) {
            _ => ""
        };
        let _ = tracker_set_linear(ctx, issue, "MERGED");
        let _ = tracker_notify(ctx, issue, "Merged: *" + issue_title_or(issue) + "*\n" + pr_url);
        return 0;
    }
    let state = match (gh.state) {
        let s: string => s,
        null => "open",
    };
    if (state == "closed") {
        return tracker_pr_closed_unmerged(ctx, issue, pr_url);
    }
    let mstate = match (gh.mergeable_state) {
        let m: string => m,
        null => "",
    };
    // A ready PR can conflict when the base branch moves — resolve + pull to prprep.
    if (mstate == "dirty") {
        let agent_id = tracker_agent_id(issue);
        let run_terminal = if (agent_id == "") {
            true
        } else {
            match (cursor_pr_for_agent(ctx.cursor_key, agent_id)) {
                let p: CursorPr => p.run_terminal,
                null => true,
            }
        };
        return tracker_resolve_conflict(ctx, issue, loc, gh, pr_url, run_terminal, true);
    }
    // Late human review: NEW reviewer comments (newer than the persisted high-water
    // mark) on an already-ready PR -> dispatch a fix and pull back to prprep. Bot
    // comments (CodeRabbit/Cursor) are skipped — those are handled in prprep's gate.
    return track_pr_ready_human_review(ctx, issue, loc, gh, pr_url);
}

function track_pr_ready_human_review(
    ctx: TrackCtx,
    issue: TrackIssue,
    loc: PrLoc,
    gh: GhPr,
    pr_url: string,
) -> int {
    let id = issue_id_or_empty(issue);
    let review_raw = github_pr_review_comments(ctx.github_token, loc.owner, loc.repo, loc.number) catch (_) {
        _ => "[]"
    };
    let convo_raw = github_issue_comments(ctx.github_token, loc.owner, loc.repo, loc.number) catch (_) {
        _ => "[]"
    };
    let hwm = match (issue.lastHumanCommentAt) {
        let h: string => h,
        null => "",
    };
    let fresh = new_human_comments(review_raw, convo_raw, hwm);
    if (fresh.length() == 0) {
        return 0;
    }
    let newest = newest_created_at(fresh);
    let attempts = fix_attempts_of(issue);
    if (attempts >= ctx.max_attempts) {
        // Advance the high-water mark so we don't re-flag the same comments forever.
        let p = "{\"lastHumanCommentAt\":" + jesc(newest) + "}";
        let _ = ctx.svc.transition("issues", id, "needs_human", "status", p, true) catch (_) {
            _ => ""
        };
        let _ = tracker_set_linear(ctx, issue, "NEEDS_HUMAN");
        let _ = tracker_notify(
            ctx,
            issue,
            "New review on *"
                + issue_title_or(issue)
                + "* but already at the fix-attempt cap — needs a human.\n"
                + pr_url,
        );
        return 0;
    }
    let sha = match (gh.head) {
        let h: GhRef => match (h.sha) {
            let s: string => s,
            null => "",
        },
        null => "",
    };
    let branch = match (issue.prBranch) {
        let b: string => b,
        null => "",
    };
    let prompt = tracker_refix_prompt(issue_title_or(issue), pr_url, branch, "reviewer comments:\n" + human_comment_summary(fresh));
    let new_id = tracker_dispatch_fix(ctx, loc, pr_url, branch, prompt);
    let patch = "{\"cursorAgentId\":"
        + jesc(new_id)
        + ",\"fixAttempts\":"
        + baml.unstable.string(attempts + 1)
        + ",\"lastFixedSha\":"
        + jesc(sha)
        + ",\"lastHumanCommentAt\":"
        + jesc(newest)
        + "}";
    let _ = ctx.svc.transition("issues", id, "prprep", "status", patch, true) catch (_) {
        _ => ""
    };
    let _ = tracker_set_linear(ctx, issue, "PR_PREP");
    let _ = tracker_notify(
        ctx,
        issue,
        "Responding to reviewer comments on *"
            + issue_title_or(issue)
            + "* (attempt "
            + baml.unstable.string(attempts + 1)
            + "/"
            + baml.unstable.string(ctx.max_attempts)
            + ").\n"
            + comment_previews(fresh, 3)
            + "PR: "
            + pr_url,
    );
    return 0;
}

// Human comments (type=="User") across the inline + conversation comment lists that
// are strictly newer than the high-water mark (ISO-8601 UTC -> lexical compare is
// chronological). Bots (CodeRabbit/Cursor) are skipped — handled by prprep's gate.
function new_human_comments(review_raw: string, convo_raw: string, hwm: string) -> GhComment[] {
    let out: GhComment[] = [];
    for (let raw in [review_raw, convo_raw]) {
        let comments: GhComment[] = baml.json.from_string<GhComment[]>(raw) catch (_) {
            _ => []
        };
        for (let c in comments) {
            let is_human = match (c.user) {
                let u: GhUser => match (u.type) {
                    let t: string => t == "User",
                    null => false,
                },
                null => false,
            };
            let created = match (c.created_at) {
                let ts: string => ts,
                null => "",
            };
            let body = match (c.body) {
                let b: string => b.trim(),
                null => "",
            };
            if (is_human && body != "" && (hwm == "" || created > hwm)) {
                out.push(c);
            }
        }
    }
    return out;
}

function newest_created_at(comments: GhComment[]) -> string {
    let newest = "";
    for (let c in comments) {
        let ts = match (c.created_at) {
            let t: string => t,
            null => "",
        };
        if (ts > newest) {
            newest = ts;
        }
    }
    return newest;
}

// A short "@author: body" markdown digest of the human comments for the fix prompt.
function human_comment_summary(comments: GhComment[]) -> string {
    let lines: string[] = [];
    for (let c in comments) {
        let who = match (c.user) {
            let u: GhUser => match (u.login) {
                let l: string => l,
                null => "reviewer",
            },
            null => "reviewer",
        };
        let body = match (c.body) {
            let b: string => b.split("\n").join(" ").trim(),
            null => "",
        };
        if (body != "") {
            lines.push("- @" + who + ": " + tail_head(body, 240));
        }
    }
    return lines.join("\n");
}

// First n chars of s (BAML has no head/truncate helper).
function tail_head(s: string, n: int) -> string {
    if (s.length() <= n) {
        return s;
    }
    return s.substring(0, n) catch (_) {
        _ => s
    };
}

// "> @author: <body>" quote lines for a Slack message (up to `limit`, each clipped).
// Empty -> "" so the caller can append it unconditionally. Ports tracker.py _previews.
function comment_previews(comments: GhComment[], limit: int) -> string {
    let lines: string[] = [];
    for (let c in comments) {
        if (lines.length() < limit) {
            let who = match (c.user) {
                let u: GhUser => match (u.login) {
                    let l: string => l,
                    null => "reviewer",
                },
                null => "reviewer",
            };
            let body = match (c.body) {
                let b: string => b.split("\n").join(" ").trim(),
                null => "",
            };
            if (body != "") {
                lines.push("> @" + who + ": " + tail_head(body, 160));
            }
        }
    }
    if (lines.length() == 0) {
        return "";
    }
    return lines.join("\n") + "\n";
}

function is_coderabbit(c: GhComment) -> bool {
    return match (c.user) {
        let u: GhUser => match (u.login) {
            let l: string => l.includes("coderabbit"),
            null => false,
        },
        null => false,
    };
}

// CodeRabbit's blocking feedback: CHANGES_REQUESTED reviews + its inline comments —
// quoted in the refix Slack note so the thread shows exactly what's being addressed.
function coderabbit_comments(reviews_raw: string, comments_raw: string) -> GhComment[] {
    let out: GhComment[] = [];
    let reviews: GhComment[] = baml.json.from_string<GhComment[]>(reviews_raw) catch (_) {
        _ => []
    };
    for (let r in reviews) {
        let changes = match (r.state) {
            let s: string => s == "CHANGES_REQUESTED",
            null => false,
        };
        if (is_coderabbit(r) && changes) {
            out.push(r);
        }
    }
    let comments: GhComment[] = baml.json.from_string<GhComment[]>(comments_raw) catch (_) {
        _ => []
    };
    for (let c in comments) {
        if (is_coderabbit(c)) {
            out.push(c);
        }
    }
    return out;
}

// ---- shared escalation / dispatch ------------------------------------------

function tracker_pr_closed_unmerged(ctx: TrackCtx, issue: TrackIssue, pr_url: string) -> int {
    let id = issue_id_or_empty(issue);
    let _ = ctx.svc.transition("issues", id, "needs_human", "status", "{}", true) catch (_) {
        _ => ""
    };
    let _ = tracker_set_linear(ctx, issue, "NEEDS_HUMAN");
    let _ = tracker_notify(
        ctx,
        issue,
        "PR for *"
            + issue_title_or(issue)
            + "* was closed without merging — needs a human (re-approve to retry).\n"
            + pr_url,
    );
    return 0;
}

// Dispatch an agent to resolve a base-branch merge conflict (bounded like a refix).
// from_pr_ready pulls the issue back to prprep; otherwise it stays in prprep.
function tracker_resolve_conflict(
    ctx: TrackCtx,
    issue: TrackIssue,
    loc: PrLoc,
    gh: GhPr,
    pr_url: string,
    run_terminal: bool,
    from_pr_ready: bool,
) -> int {
    let id = issue_id_or_empty(issue);
    if (run_terminal == false) {
        return 0; // an agent is still pushing; its next commit may resolve it
    }
    let sha = match (gh.head) {
        let h: GhRef => match (h.sha) {
            let s: string => s,
            null => "",
        },
        null => "",
    };
    let last_sha = match (issue.lastFixedSha) {
        let s: string => s,
        null => "",
    };
    if (sha != "" && sha == last_sha) {
        return 0; // already dispatched a resolution for this exact commit
    }
    let attempts = fix_attempts_of(issue);
    if (attempts >= ctx.max_attempts) {
        let _ = ctx.svc.transition("issues", id, "needs_human", "status", "{}", true) catch (_) {
            _ => ""
        };
        let _ = tracker_set_linear(ctx, issue, "NEEDS_HUMAN");
        let _ = tracker_notify(
            ctx,
            issue,
            "*"
                + issue_title_or(issue)
                + "* has a merge conflict but is already at the fix-attempt cap — needs a human.\n"
                + pr_url,
        );
        return 0;
    }
    let base = match (gh.base) {
        let b: GhRef => match (b.ref) {
            let r: string => r,
            null => "main",
        },
        null => "main",
    };
    let branch = match (issue.prBranch) {
        let b: string => b,
        null => "",
    };
    let prompt = tracker_conflict_prompt(issue_title_or(issue), pr_url, branch, base);
    let new_id = tracker_dispatch_fix(ctx, loc, pr_url, branch, prompt);
    let patch = "{\"cursorAgentId\":"
        + jesc(new_id)
        + ",\"fixAttempts\":"
        + baml.unstable.string(attempts + 1)
        + ",\"lastFixedSha\":"
        + jesc(sha)
        + "}";
    if (from_pr_ready) {
        let _ = ctx.svc.transition("issues", id, "prprep", "status", patch, true) catch (_) {
            _ => ""
        };
        let _ = tracker_set_linear(ctx, issue, "PR_PREP");
    } else {
        let _ = ctx.svc.update("issues", id, patch) catch (_) {
            _ => ""
        };
    }
    let _ = tracker_notify(
        ctx,
        issue,
        "Merge conflict with `"
            + base
            + "` on *"
            + issue_title_or(issue)
            + "* — sent back to Cursor to resolve (attempt "
            + baml.unstable.string(attempts + 1)
            + "/"
            + baml.unstable.string(ctx.max_attempts)
            + ").\nPR: "
            + pr_url,
    );
    return 0;
}

// Launch a fresh agent on the EXISTING PR branch (new_agent refix mode): pass
// pr_url + work_on_current_branch so commits land on that PR, not a new one.
function tracker_dispatch_fix(
    ctx: TrackCtx,
    loc: PrLoc,
    pr_url: string,
    branch: string,
    prompt: string,
) -> string {
    // Cursor requires bc-<uuid>; mint from /proc like FixDispatch. A plain
    // "bc-" + baml.id.new() is rejected with 400 "Agent ID must be in the format
    // 'bc-<uuid>'", so the refix silently never launched. Return the id Cursor
    // confirmed (its response "id"), falling back to the minted id.
    let agent_id = mint_agent_id();
    let repo_url = repo_url_from_pr(loc.owner, loc.repo);
    let ref = match (branch) {
        "" => "main",
        _ => branch,
    };
    let resp = cursor_launch_agent(ctx.cursor_key, prompt, repo_url, ref, agent_id, ctx.model, pr_url, true) catch (_) {
        _ => "{}"
    };
    let id_field = extract_field(resp, "id");
    if (id_field != "") {
        return id_field;
    }
    return agent_id;
}

function tracker_refix_prompt(
    title: string,
    pr_url: string,
    branch: string,
    reasons: string,
) -> string {
    let branch_note = match (branch) {
        "" => "",
        _ => " on branch `" + branch + "`",
    };
    return "You are fixing pull request "
        + pr_url
        + branch_note
        + ", which addresses: "
        + title
        + ".\nThe PR has unresolved feedback ("
        + reasons
        + "). Address EVERYTHING, then commit and push to the SAME pull request ("
        + pr_url
        + ") — do not open a new PR. Inspect the PR's failing checks and CodeRabbit review directly and fix them. Build + run the targeted tests for what you changed; do NOT run the full suite (CI re-runs it). Document every function/type you add or change with a docstring.";
}

function tracker_conflict_prompt(
    title: string,
    pr_url: string,
    branch: string,
    base: string,
) -> string {
    let branch_note = match (branch) {
        "" => "",
        _ => " (branch `" + branch + "`)",
    };
    return "Pull request "
        + pr_url
        + branch_note
        + ", which fixes: "
        + title
        + ", no longer merges cleanly — it conflicts with its base branch `"
        + base
        + "`.\nResolve it: bring the latest `"
        + base
        + "` into the PR branch and resolve EVERY conflict correctly — keep BOTH the base branch's changes and this PR's fix. Then commit and push to the SAME pull request ("
        + pr_url
        + ") — do not open a new PR. Confirm it builds + the targeted tests pass; do NOT run the full suite. Put a docstring on anything you add or modify.";
}
