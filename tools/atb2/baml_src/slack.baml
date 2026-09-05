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
    let thread = if (slack_configured()) {
        issue_thread(issue.id) ?? feedback_thread(issue)
    } else {
        null
    };
    let posted = if (slack_configured()) {
        slack_post(text, thread)
    } else {
        null
    };
    if (kind == "issue_created" && posted != null && db_configured()) {
        let anchor = posted ?? baml.sys.panic("checked post");
        update(
            "issues",
            "id=" + eq(issue.id) + "&state=eq.awaiting_approval&dataset=" + eq(dataset()),
            { "slack_ts": anchor.ts, "slack_channel": anchor.channel }.to_json(),
        );
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
            let p: SlackRef => (thread ?? p).ts,
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
                let s: string => {
                    " · shepherd "
                        + match (shepherd_slack_id(issue.shepherd)) {
                            null => s,
                            let user: string => "<@" + user + ">",
                        }
                },
            }
        )
        + (
            match (issue.difficulty) {
                null => "",
                let d: Difficulty => " · " + d.to_string(),
            }
        )
        + "\nReact :white_check_mark: or :+1: to this message to approve the fix."
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

// ==================== Signed HTTP intake ====================

function hmac_sha256_hex(key: string, message: string) -> string {
    let block = 64;
    let key_bytes = key.to_utf8();
    let k = if (key_bytes.length() > block) {
        let h = baml.crypto.Sha256.new();
        h.update(key_bytes);
        h.finish()
    } else {
        key_bytes
    };
    let ipad = uint8array.zeroes(0) catch_all (e) {
        _ => baml.sys.panic("zeroes")
    };
    let opad = uint8array.zeroes(0) catch_all (e) {
        _ => baml.sys.panic("zeroes")
    };
    for (let i = 0; i < block; i += 1) {
        let b = k.at(i) ?? 0;
        ipad.push(b ^ 54);
        opad.push(b ^ 92);
    }
    let inner = baml.crypto.Sha256.new();
    inner.update(ipad);
    inner.update(message.to_utf8());
    let outer = baml.crypto.Sha256.new();
    outer.update(opad);
    outer.update(inner.finish());
    outer.finish().to_hex()
}

/// Header names are case insensitive. Reject ambiguous duplicates.
function slack_header(req: baml.http.Request, name: string) -> string? {
    let found: string? = null;
    for (let key in req.headers.keys()) {
        if (key.to_lower_case() == name) {
            if (found != null) {
                return null;
            };
            found = req.headers.get(key);
        };
    }
    found
}

/// Raw-body HMAC with Slack's five-minute replay window. No early exit
/// based on the matching prefix of the signature.
function slack_signature_ok(req: baml.http.Request, secret: string, now: bigint) -> bool {
    let ts = slack_header(req, "x-slack-request-timestamp") ?? "";
    let sig = slack_header(req, "x-slack-signature") ?? "";
    if (secret.length() == 0 || ts.length() == 0 || ts.length() > 16 || sig.length() != 67) {
        return false;
    };
    let timestamp = bigint.parse(ts) catch (_) {
        _ => {
            return false;
        }
    };
    if ((now - timestamp).abs() > 300n) {
        return false;
    };
    let expected = ("v0=" + hmac_sha256_hex(secret, "v0:" + ts + ":" + req.body)).to_utf8();
    let actual = sig.to_utf8();
    if (actual.length() != expected.length()) {
        return false;
    };
    let diff = 0;
    for (let i = 0; i < expected.length(); i += 1) {
        diff = diff | ((expected.at(i) ?? 0) ^ (actual.at(i) ?? 0));
    }
    diff == 0
}

class MentionRoute {
    kind: string,
    text: string,
    pr: string?,
}

function parse_mention(raw: string) -> MentionRoute {
    let text = raw.trim();
    if (text.starts_with("<@")) {
        let end = text.index_of(">");
        if (end != null) {
            text = text.slice((end ?? 0) + 1, text.length()).trim();
        };
    };
    let command = (text.split(" ").at(0) ?? "").to_lower_case();
    if (command == "babysit") {
        return MentionRoute { kind: "babysit", text: text, pr: pr_url_in(text) };
    };
    let shirt = text.to_lower_case().split(" ").some((word: string) -> bool {
        let clean = trim_punct(word);
        clean == "shirt" || clean == "tshirt" || clean == "t-shirt"
    });
    MentionRoute {
        kind: if (shirt) {
            "shirt"
        } else {
            "feedback"
        },
        text: text,
        pr: null,
    }
}

// ==================== Shepherd approval ====================

function shepherd_id_in(login: string?, config: string) -> string? {
    if (login == null) {
        return null;
    };
    let found: string? = null;
    for (let entry in config.split(",")) {
        let pair = entry.trim().split(":");
        if (
            pair.length() == 2
                && (pair.at(0) ?? "").trim().to_lower_case() == (login ?? "").to_lower_case()
        ) {
            let user = (pair.at(1) ?? "").trim();
            if (found != null || !slack_id(user) || !(user.starts_with("U") || user.starts_with("W"))) {
                return null;
            };
            found = user;
        };
    }
    found
}

function shepherd_slack_id(login: string?) -> string? {
    shepherd_id_in(login, baml.env.get("ATB2_SHEPHERDS") ?? "")
}

/// Reply under the original feedback thread. The approval anchor itself is
/// the bot's distinct message, so two issues in a thread cannot approve each other.
function feedback_thread(issue: Issue) -> SlackRef? {
    if (!db_configured()) {
        return null;
    };
    for (let id in issue.feedback_ids) {
        let rows = select("feedback", "select=origin&id=" + eq(id) + "&dataset=" + eq(dataset()) + "&limit=1");
        let row = rows.at(0) ?? baml.json.parse("{}");
        let channel = baml.json.path_or<string>(row, ".origin.channel", "");
        let ts = baml.json.path_or<string>(row, ".origin.ts", "");
        if (slack_id(channel) && slack_ts(ts)) {
            return SlackRef { channel: channel, ts: ts };
        };
    }
    null
}

class ApprovalReaction {
    user: string,
    channel: string,
    ts: string,
}

function approval_reaction(event: json) -> ApprovalReaction? {
    if (
        baml.json.path_or<string>(event, ".type", "") != "reaction_added"
            || baml.json.path_or<string>(event, ".item.type", "") != "message"
    ) {
        return null;
    };
    let emoji = baml.json.path_or<string>(event, ".reaction", "");
    if (emoji != "white_check_mark" && emoji != "+1") {
        return null;
    };
    let user = baml.json.path_or<string>(event, ".user", "");
    let channel = baml.json.path_or<string>(event, ".item.channel", "");
    let ts = baml.json.path_or<string>(event, ".item.ts", "");
    if (!slack_id(user) || !slack_id(channel) || !slack_ts(ts)) {
        return null;
    };
    ApprovalReaction { user: user, channel: channel, ts: ts }
}

function can_approve(issue: Issue, reaction: ApprovalReaction, mapping: string) -> bool {
    let awaiting = match (issue.status) {
        let s: AwaitingApproval => true,
        _ => false,
    };
    awaiting
        && issue.slack_ts == reaction.ts
        && issue.slack_channel == reaction.channel
        && shepherd_id_in(issue.shepherd, mapping) == reaction.user
}

function accept_approval(event: json) -> null {
    let parsed = approval_reaction(event);
    if (parsed == null) {
        return null;
    };
    if (!db_configured()) {
        throw IntakeError { message: "approval needs the store" };
    };
    let r = parsed ?? baml.sys.panic("checked reaction");
    let filter = "state=eq.awaiting_approval&slack_ts="
        + eq(r.ts)
        + "&slack_channel="
        + eq(r.channel)
        + "&dataset="
        + eq(dataset());
    let rows = rows_of(supabase_send("GET", "issues?select=*&" + filter + "&limit=2", "", "", timeout_s = 1));
    // Ambiguous anchors fail closed.
    if (rows.length() != 1) {
        return null;
    };
    let issue = issue_from_row(rows.at(0) ?? baml.json.parse("{}"));
    if (!can_approve(issue, r, baml.env.get("ATB2_SHEPHERDS") ?? "")) {
        return null;
    };
    let status = Approved { state: "approved", by: r.user };
    let saved = rows_of(
        supabase_send(
            "PATCH",
            "issues?" + filter + "&id=" + eq(issue.id) + "&shepherd=" + eq(issue.shepherd ?? ""),
            baml.json.stringify({ "status": status }.to_json()),
            "return=representation",
            timeout_s = 1,
        ),
    );
    if (saved.length() > 0) {
        spawn {
            notify(
                "approved",
                issue_from_row(saved.at(0) ?? baml.json.parse("{}")),
                "Approved by <@" + r.user + ">; queued for the next fix pass.",
                status.to_json(),
            ) catch_all (_) {
                _ => {
                    log.warn({ "approval": "narration failed; approval is stored" });
                    null
                },
            };
        };
    };
    null
}

function approval_fixture() -> Issue {
    let row = baml.json.from_json<map<string, json>>(
        issue_row(with_status(gh_issue(1, Difficulty.Easy), AwaitingApproval { state: "awaiting_approval" })),
    );
    row.set("shepherd", "owner".to_json());
    row.set("slack_ts", "123.4".to_json());
    row.set("slack_channel", "C1".to_json());
    issue_from_row(row.to_json())
}

testset "approval" {
    test "only the assigned shepherd can approve the exact message in its channel" {
        let issue = approval_fixture();
        let right = ApprovalReaction { user: "U1", channel: "C1", ts: "123.4" };
        assert.equal(can_approve(issue, right, "owner:U1"), true);
        assert.equal(can_approve(issue, right, "owner:U2"), false);
        assert.equal(can_approve(issue, right, ""), false);
        assert.equal(
            can_approve(issue, ApprovalReaction { user: "U1", channel: "C2", ts: "123.4" }, "owner:U1"),
            false,
        );
        assert.equal(
            can_approve(issue, ApprovalReaction { user: "U1", channel: "C1", ts: "123.0" }, "owner:U1"),
            false,
        );
        assert.equal(
            can_approve(with_status(issue, Approved { state: "approved", by: "U1" }), right, "owner:U1"),
            false,
        );
    }
    test "mapping is explicit and ambiguous duplicate mappings are refused" {
        assert.equal(shepherd_id_in("OWNER", "other:U2, owner:U1") ?? "", "U1");
        assert.equal(shepherd_id_in("owner", "owner:U1,owner:U2") == null, true);
        assert.equal(shepherd_id_in("owner", "owner:bad") == null, true);
        assert.equal(shepherd_id_in(null, "owner:U1") == null, true);
    }
    test "only approval emoji on a message becomes an approval" {
        let r = approval_reaction(
            baml.json.parse(
                `{"type":"reaction_added","reaction":"white_check_mark","user":"U1","item":{"type":"message","channel":"C1","ts":"123.4"}}`,
            ),
        );
        assert.equal(r == null, false);
        assert.equal(
            approval_reaction(
                baml.json.parse(
                    `{"type":"reaction_added","reaction":"heart","user":"U1","item":{"type":"message","channel":"C1","ts":"123.4"}}`,
                ),
            )
                == null,
            true,
        );
        assert.equal(
            approval_reaction(
                baml.json.parse(
                    `{"type":"reaction_removed","reaction":"+1","user":"U1","item":{"type":"message","channel":"C1","ts":"123.4"}}`,
                ),
            )
                == null,
            true,
        );
    }
    test "automatic handling refuses open and awaiting issues" {
        let pending = approval_fixture();
        assert.equal(issue_approved(pending), false);
        assert.equal(handle_one(pending, HandleMode.DryRun) == null, true);
        assert.equal(
            handle_one(with_status(pending, Open { state: "open" }), HandleMode.DryRun) == null,
            true,
        );
        let approved = with_status(pending, Approved { state: "approved", by: "U1" });
        assert.equal(issue_approved(approved), true);
        assert.equal(handle_ready(approved, false) == null, true);
        assert.equal(issue_from_row(issue_row(approved)).slack_ts ?? "", "123.4");
        assert.equal(issue_from_row(issue_row(approved)).slack_channel ?? "", "C1");
        assert.equal(
            with_status(approved, AwaitingApproval { state: "awaiting_approval" }).slack_ts == null,
            true,
        );
    }
}
