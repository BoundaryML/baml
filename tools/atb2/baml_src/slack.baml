// Slack, both directions.
//
// Out: one thread per issue in the pipeline channel. The first message
// (issue triaged) opens the thread; every later step (PR opened, gate
// failed, needs a human, merged) replies under it, so a channel scroll
// reads one line per issue and the detail lives in the thread.
//
// In: reports posted in the intake channel become Feedback (source Slack),
// pulled by the pipeline's ingest step; the cursor is the newest `ts` seen.
//
//   ATB2_SLACK_BOT_TOKEN       xoxb-... (chat:write, channels:history, channels:read)
//   ATB2_SLACK_CHANNEL         where the pipeline reports (C.. id or #name)
//   ATB2_SLACK_INTAKE_CHANNEL  where people post bugs (C.. id); unset = no Slack intake
//
// The first two fall back to the old bot's ATB_SLACK_BOT_TOKEN and
// ATB_SLACK_FIX_CHANNEL (Infisical, boundary-tools), so the same app and
// channel serve both.
//
// Without a token every notification is a no-op and the pipeline runs as
// before. Message text is rendered by pure functions with token-free tests.

// ==================== Web API ====================

/// Slack could not be reached or said no. Thrown so a caller can log it
/// and go on: Slack being down must never stop a run.
class SlackError {
    message: string,
}

function slack_configured() -> bool {
    slack_token() != null
        && baml.env.get("ATB2_SLACK_CHANNEL") != null
        || slack_token() != null
        && baml.env.get("ATB_SLACK_FIX_CHANNEL") != null
}

function slack_token() -> string? {
    baml.env.get("ATB2_SLACK_BOT_TOKEN") ?? baml.env.get("ATB_SLACK_BOT_TOKEN")
}

function slack_channel() -> string {
    baml.env.get("ATB2_SLACK_CHANNEL") ?? baml.env.get("ATB_SLACK_FIX_CHANNEL") ?? ""
}

/// One Slack Web API call. POST with a JSON body, or GET with a query
/// string when `body` is null. Panics on transport failure or `ok: false`:
/// Slack that is configured but broken should be noticed, not skipped.
function slack_api(method: string, body: json?, query: string) -> json {
    let token = slack_token() ?? "";
    let url = "https://slack.com/api/"
        + method
        + (
            if (query.length() > 0) {
                "?" + query
            } else {
                ""
            }
        );
    let res = baml.http.send(
        baml.http.Request {
            method: if (body == null) {
                "GET"
            } else {
                "POST"
            },
            url: url,
            headers: {
                "Authorization": "Bearer " + token,
                "Content-Type": "application/json; charset=utf-8",
            },
            body: match (body) {
                null => "",
                let b: json => baml.json.stringify(b),
            },
        },
        timeout = baml.time.Duration.from_seconds(30),
    ) catch (_) {
        _ => {
            throw SlackError { message: method + " failed" };
        },
    };
    let text = res.text() catch (_) {
        _ => "{}",
    };
    let out = baml.json.parse(text) catch (_) {
        _ => baml.json.parse("{}"),
    };
    if (!baml.json.path_or<bool>(out, ".ok", false)) {
        throw SlackError { message: method + ": " + baml.json.path_or<string>(out, ".error", "not ok") };
    };
    out
}

/// Posts `text` (mrkdwn) to the pipeline channel, in `thread` when given.
/// Returns the message's channel + ts, the handle later replies need.
function slack_post(text: string, thread: SlackRef?) -> SlackRef {
    let body: map<string, json> = {
        "channel": match (thread) {
            null => slack_channel(),
            let t: SlackRef => t.channel,
        }
            .to_json(),
        "text": text.to_json(),
        "unfurl_links": false.to_json(),
    };
    match (thread) {
        null => null,
        let t: SlackRef => body.set("thread_ts", t.ts.to_json()),
    };
    let out = slack_api("chat.postMessage", body.to_json(), "");
    SlackRef {
        channel: baml.json.path_or<string>(out, ".channel", slack_channel()),
        ts: baml.json.path_or<string>(out, ".ts", ""),
    }
}

// ==================== Notifications ====================

/// Tells the channel what happened to an issue and records it. The first
/// notification for an issue opens its thread; later ones reply in it.
/// Always logs the event, Slack or not.
function notify(kind: string, issue: Issue, text: string, payload: json) -> null {
    let posted = if (slack_configured()) {
        let thread = issue_thread(issue.id);
        slack_post(text, thread)
    } else {
        null
    };
    log_event(
        kind,
        issue.id,
        null,
        payload,
        match (posted) {
            null => null,
            let p: SlackRef => p.channel,
        },
        match (posted) {
            null => null,
            let p: SlackRef => p.ts,
        },
    );
    log.info({ "issue": issue.id, "notified": kind, "slack": posted != null });
    null
}

/// Where the UI shows the issue, if the UI is deployed.
function issue_link(issue: Issue) -> string {
    match (baml.env.get("ATB2_UI_URL")) {
        null => issue.id,
        let base: string => "<" + base.trim() + "/issues/" + issue.id + "|" + issue.id + ">",
    }
}

function issue_created_text(issue: Issue) -> string {
    let n = issue.feedback_ids.length();
    ":new: *"
        + issue.title
        + "*\n"
        + issue_link(issue)
        + " · "
        + issue.subsystem.to_string()
        + " · from "
        + n.to_string()
        + (
            if (n == 1) {
                " report"
            } else {
                " reports"
            }
        )
        + " · "
        + issue.repros.length().to_string()
        + (
            if (issue.repros.length() == 1) {
                " repro"
            } else {
                " repros"
            }
        )
        + (
            match (issue.shepherd) {
                null => "",
                let s: string => " · shepherd " + s,
            }
        )
        + (
            match (issue.difficulty) {
                null => "",
                let d: Difficulty => " · " + d.to_string(),
            }
        )
}

function pr_opened_text(issue: Issue, o: HandleOutcome) -> string {
    ":rocket: draft PR opened for *"
        + issue.title
        + "*\n"
        + (o.pr ?? "(no url)")
        + " · "
        + o.turns.to_string()
        + " turns · "
        + minutes(o.seconds)
        + " · gate "
        + (
            match (o.gate) {
                null => "not run",
                let g: GateResult => {
                    if (g.ok) {
                        "green"
                    } else {
                        "red"
                    }
                },
            }
        )
}

function needs_human_text(issue: Issue, o: HandleOutcome) -> string {
    let why = match (o.kind) {
        "hard" => "gauged Hard: the design doc is ready for the shepherd, no code was changed",
        "gate_failed" => {
            "the fix is on branch `"
                + (o.branch ?? "?")
                + "` but the gate failed"
                + (
                    match (o.gate) {
                        null => "",
                        let g: GateResult => ": " + failed_steps(g),
                    }
                )
        },
        "agent_stopped" => {
            "the agent stopped without a result"
                + (
                    match (o.reason) {
                        null => "",
                        let r: string => " (" + r + ")",
                    }
                )
        },
        _ => o.kind,
    };
    ":raised_hand: *"
        + issue.title
        + "* needs a human\n"
        + why
        + (
            match (issue.shepherd) {
                null => "",
                let s: string => "\ncc " + s,
            }
        )
}

function merged_text(issue: Issue, pr: string) -> string {
    ":white_check_mark: merged *" + issue.title + "*\n" + pr
}

function merge_round_text(issue: Issue, pr: string, round: int, kind: string) -> string {
    (
        if (kind == "fixed") {
            ":arrows_counterclockwise: "
        } else {
            ":warning: "
        }
    )
        + "review round "
        + round.to_string()
        + " on "
        + pr
        + ": "
        + kind
}

function failed_steps(g: GateResult) -> string {
    g.steps
        .filter((s: GateStep) -> bool {
            !s.ok
        })
        .map((s: GateStep) -> string {
            s.name
        })
        .join(", ")
}

function minutes(seconds: int) -> string {
    if (seconds < 60) {
        seconds.to_string() + "s"
    } else {
        (seconds / 60).to_string() + "m"
    }
}

/// Posts in the thread when there is one and Slack is configured; never
/// fails the round over a Slack error.
function tell_thread(thread: SlackRef?, text: string) -> null {
    match (thread) {
        null => null,
        let t: SlackRef => {
            if (slack_configured()) {
                slack_post(text, t) catch (_) {
                    _ => SlackRef { channel: "", ts: "" },
                };
            };
            null
        },
    }
}

/// A PR url in a mention ("babysit this PR https://github.com/o/r/pull/12",
/// "<https://github.com/o/r/pull/12|pull/12>"), or null. Pure; the Slack ingress
/// has the same rule in Python.
function pr_url_in(text: string) -> string? {
    for (
        let raw in text.split(" ").flat_map((t: string) -> string[] {
            t.split("\n")
        })
    ) {
        let tok = raw.trim();
        // Slack wraps links: <url> or <url|label>
        let inner = if (tok.starts_with("<") && tok.includes(">")) {
            tok.slice(1, tok.index_of(">") ?? tok.length())
        } else {
            tok
        };
        let url = inner.split("|").at(0) ?? "";
        let core = trim_punct(url);
        if (core.starts_with("https://github.com/") && core.includes("/pull/")) {
            let n = core.split("/pull/").at(1) ?? "";
            let digits = n.split("/").at(0) ?? "";
            if (
                digits.length() > 0
                    && digits.chars().every((c: string) -> bool {
                        c >= "0" && c <= "9"
                    })
            ) {
                return (core.split("/pull/").at(0) ?? "") + "/pull/" + digits;
            };
        };
    }
    null
}

// ==================== Intake ====================

function slack_intake_channel() -> string? {
    baml.env.get("ATB2_SLACK_INTAKE_CHANNEL")
}

/// Reports posted in the intake channel after `oldest` (a Slack ts, or
/// null for the last 200 messages), as Feedback, oldest first. Thread
/// replies and bot messages are not reports.
function slack_feedback(oldest: string?, resume: string? = null) -> SlackFeedback {
    let channel = slack_intake_channel();
    if (channel == null || slack_token() == null) {
        return SlackFeedback { feedback: [], newest_ts: oldest, resume: null, ran: false };
    };
    let ch = channel ?? "";
    let base = "channel="
        + ch
        + "&limit=200"
        + (
            match (oldest) {
                null => "",
                let o: string => "&oldest=" + o,
            }
        );
    // every page (Slack pages with response_metadata.next_cursor), at most
    // ten per run so a flood cannot pin the loop; the rest waits a run
    let messages: json[] = [];
    let cursor: string? = resume;
    let pages = 0;
    while (pages < 10) {
        let query = base
            + (
                match (cursor) {
                    null => "",
                    let c: string => "&cursor=" + url_encode(c),
                }
            );
        let out = slack_api("conversations.history", null, query);
        messages = messages.concat(baml.json.path_or<json[]>(out, ".messages", []));
        let next = baml.json.path_or<string>(out, ".response_metadata.next_cursor", "");
        pages += 1;
        if (next.length() == 0) {
            cursor = null;
            break;
        };
        cursor = next;
    }
    let newest = oldest;
    let feedback: Feedback[] = [];
    // Slack returns newest first
    for (let m in messages.reverse()) {
        let ts = baml.json.path_or<string>(m, ".ts", "");
        if (ts.length() > 0 && (newest == null || ts > (newest ?? ""))) {
            newest = ts;
        };
        match (feedback_from_slack(m, ch)) {
            null => null,
            let fb: Feedback => feedback.push(fb),
        };
    }
    SlackFeedback { feedback: feedback, newest_ts: newest, resume: cursor, ran: true }
}

class SlackFeedback {
    feedback: Feedback[],
    /// The newest message ts seen; the `oldest` cursor once the scan is
    /// complete (see `resume`).
    newest_ts: string?,
    /// Set when the page cap stopped the scan: Slack's next_cursor to
    /// continue from. Until it is null, `oldest` must not move.
    resume: string?,
    /// False when the intake is not configured: nothing was scanned.
    ran: bool,
}

/// One Slack message as a report, or null when it is not one (a bot, a
/// join, a thread reply, an empty message). Pure.
function feedback_from_slack(m: json, channel: string) -> Feedback? {
    if (baml.json.path_or<string>(m, ".type", "") != "message") {
        return null;
    };
    if (
        baml.json.path_or<string?>(m, ".subtype", null) != null
            || baml.json.path_or<string?>(m, ".bot_id", null) != null
    ) {
        return null;
    };
    let ts = baml.json.path_or<string>(m, ".ts", "");
    let thread = baml.json.path_or<string?>(m, ".thread_ts", null);
    if (thread != null && (thread ?? "") != ts) {
        return null;
    };
    let text = baml.json.path_or<string>(m, ".text", "").trim();
    if (text.length() == 0) {
        return null;
    };
    let user = baml.json.path_or<string>(m, ".user", "unknown");
    // attached snippets come as files with a preview; the body carries a link to the rest
    let files: map<string, string> = {};
    for (let f in baml.json.path_or<json[]>(m, ".files", [])) {
        let name = baml.json.path_or<string>(f, ".name", "attachment");
        let preview = baml.json.path_or<string?>(f, ".preview", null);
        match (preview) {
            null => null,
            let p: string => files.set(name, p),
        };
    }
    Feedback {
        id: "SL-" + ts.replace_all(".", "-"),
        title: first_line(text, 120),
        body: text,
        source: FeedbackSource.Slack,
        author: Author { email: null, github: null, device_id: "slack:" + user },
        toolchain: toolchain_mentioned(text),
        files: files,
        comments: [],
        issue_ids: [],
    }
}

/// The first line, cut to `max` characters.
function first_line(text: string, max: int) -> string {
    let line = text.split("\n").at(0) ?? "";
    let chars = line.chars();
    if (chars.length() > max) {
        chars.slice(0, max - 1).join("") + "…"
    } else {
        line
    }
}

/// A toolchain version named in the text ("0.17.0", "0.18.0-nightly..."),
/// or null. Reports rarely say; the triage prompt asks when it matters.
function toolchain_mentioned(text: string) -> string? {
    for (
        let raw in text.split(" ").flat_map((t: string) -> string[] {
            t.split("\n")
        })
    ) {
        let tok = trim_punct(raw);
        if (looks_like_version(tok)) {
            return tok;
        };
    }
    null
}

/// "0.17.0", "0.18.0-nightly.20260824.b": three dot-separated numbers,
/// optionally a dash suffix.
function looks_like_version(tok: string) -> bool {
    let core = tok.split("-").at(0) ?? "";
    let parts = core.split(".");
    if (parts.length() != 3) {
        return false;
    };
    parts.every((p: string) -> bool {
        p.length() > 0
            && p.chars().every((c: string) -> bool {
                c >= "0" && c <= "9"
            })
    })
}

/// Strips the punctuation a version is wrapped in inside prose.
function trim_punct(tok: string) -> string {
    let t = tok;
    while (
        t.length() > 0
            && [",", ".", ";", ":", ")", "(", "`", "\"", "'"].includes(
                t.slice(t.length() - 1, t.length()),
            )
    ) {
        t = t.slice(0, t.length() - 1);
    }
    while (t.length() > 0 && ["(", "`", "\"", "'"].includes(t.slice(0, 1))) {
        t = t.slice(1, t.length());
    }
    t
}

// ==================== Unit tests (token-free) ====================

testset "slack" {
    test "the created line names the issue, subsystem, reports and repros" {
        let t = issue_created_text(with_difficulty(gh_issue(4587, null), Difficulty.Easy));
        assert.contains(t, "GH-4587");
        assert.contains(t, "Compiler");
        assert.contains(t, "0 reports");
        assert.contains(t, "0 repros");
        assert.contains(t, "Easy");
    }

    test "the needs-human line says why" {
        let g = sample_gate();
        let o = outcome_for_test("gate_failed", g, null);
        let t = needs_human_text(gh_issue(1, null), o);
        assert.contains(t, "gate failed");
        assert.contains(t, "agent/gh-1-x");
        let stopped = HandleOutcome {
            kind: "agent_stopped",
            branch: null,
            pr: null,
            turns: 0,
            seconds: 5,
            timed_out: true,
            gate: null,
            report: null,
            design_doc: null,
            reason: "killed at the time budget",
        };
        assert.contains(needs_human_text(gh_issue(1, null), stopped), "killed at the time budget");
        assert.contains(
            needs_human_text(gh_issue(1, null), outcome_for_test("hard", null, null)),
            "design doc",
        );
    }

    test "the PR line carries url, turns and gate" {
        let o = HandleOutcome {
            kind: "fixed",
            branch: "agent/x",
            pr: "https://github.com/BoundaryML/baml/pull/1",
            turns: 12,
            seconds: 640,
            timed_out: false,
            gate: sample_gate(),
            report: null,
            design_doc: null,
            reason: null,
        };
        let t = pr_opened_text(gh_issue(1, null), o);
        assert.contains(t, "pull/1");
        assert.contains(t, "12 turns");
        assert.contains(t, "10m");
    }

    test "a Slack message becomes a report; replies, bots and joins do not" {
        let m = baml.json.parse(
            `{"type":"message","user":"U1","ts":"1725000000.000100","text":"baml check panics on \`throws Nope\` with 0.17.0\\nstack: index out of bounds","files":[{"name":"main.baml","preview":"function f() -> int throws Nope { 1 }"}]}`,
        );
        let fb = feedback_from_slack(m, "C1") ?? baml.sys.panic("no feedback");
        assert.equal(fb.id, "SL-1725000000-000100");
        assert.equal(fb.source, FeedbackSource.Slack);
        assert.equal(fb.author.device_id ?? "", "slack:U1");
        assert.equal(fb.toolchain ?? "", "0.17.0");
        assert.equal(fb.files.get("main.baml") ?? "", "function f() -> int throws Nope { 1 }");
        assert.is_true(fb.title.starts_with("baml check panics"));
        assert.equal(
            feedback_from_slack(
                baml.json.parse(
                    `{"type":"message","user":"U1","ts":"2.0","thread_ts":"1.0","text":"reply"}`,
                ),
                "C1",
            )
                == null,
            true,
        );
        assert.equal(
            feedback_from_slack(baml.json.parse(`{"type":"message","bot_id":"B1","ts":"3.0","text":"bot"}`), "C1")
                == null,
            true,
        );
        assert.equal(
            feedback_from_slack(
                baml.json.parse(
                    `{"type":"message","subtype":"channel_join","user":"U1","ts":"4.0","text":"joined"}`,
                ),
                "C1",
            )
                == null,
            true,
        );
    }

    test "helpers" {
        assert.equal(first_line("a\nb", 10), "a");
        assert.equal(first_line("abcdef", 4), "abc…");
        assert.equal(toolchain_mentioned("on 0.18.0-nightly.1 it fails") ?? "", "0.18.0-nightly.1");
        assert.equal(toolchain_mentioned("no version") == null, true);
        assert.equal(minutes(59), "59s");
        assert.equal(minutes(600), "10m");
        assert.equal(slack_configured(), false);
    }

    test "a PR url comes out of a mention, Slack-wrapped or not" {
        assert.equal(
            pr_url_in("babysit this PR https://github.com/BoundaryML/baml/pull/4715") ?? "",
            "https://github.com/BoundaryML/baml/pull/4715",
        );
        assert.equal(
            pr_url_in("babysit <https://github.com/BoundaryML/baml/pull/4715|pull/4715> please")
                ?? "",
            "https://github.com/BoundaryML/baml/pull/4715",
        );
        assert.equal(
            pr_url_in("babysit <https://github.com/BoundaryML/baml/pull/4715/files>") ?? "",
            "https://github.com/BoundaryML/baml/pull/4715",
        );
        assert.equal(
            pr_url_in("babysit https://github.com/BoundaryML/baml/pull/4715.") ?? "",
            "https://github.com/BoundaryML/baml/pull/4715",
        );
        assert.equal(
            pr_url_in("babysit https://github.com/BoundaryML/baml/issues/4715") == null,
            true,
        );
        assert.equal(pr_url_in("hello") == null, true);
    }
}
