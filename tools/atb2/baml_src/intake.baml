// The front door. Everything that brings work into the pipeline enters
// here, and each entry point calls the pipeline stage it needs with the
// Slack thread it came from; the stages answer in that thread through
// slack.baml.
//
//   PostHog   `baml feedback` posts a `baml_feedback` event; posthog_feedback
//             turns new events into Feedback (run_pipeline's ingest stage)
//   Slack     @bammy mentions and reactions arrive at the HTTP server here
//             babysit <PR> -> the approval queue, a report -> triage,
//             a reaction -> approval
//   by hand   `baml run -e 'request_merge("<PR url>")'`, `run_pipeline()`
//
//   ATB2_POSTHOG_API_KEY / _PROJECT_ID / _HOST, falling back to ATB_POSTHOG_*
//   (Infisical boundary-tools, prod). Host defaults to https://us.posthog.com.
//   The PostHog cursor is the newest event timestamp seen.

// ==================== PostHog ====================

/// PostHog could not be read. Thrown so the loop logs it and goes on.
class IntakeError {
    message: string,
}

function posthog_configured() -> bool {
    posthog_env("API_KEY") != null && posthog_env("PROJECT_ID") != null
}

/// ATB2_POSTHOG_<name>, else the old ATB_POSTHOG_<name>.
function posthog_env(name: string) -> string? {
    baml.env.get("ATB2_POSTHOG_" + name) ?? baml.env.get("ATB_POSTHOG_" + name)
}

/// The API host; https only, since the key goes in a bearer header.
function posthog_host() -> string {
    let h = (posthog_env("HOST") ?? "https://us.posthog.com").trim();
    if (!h.starts_with("https://")) {
        throw IntakeError { message: "ATB2_POSTHOG_HOST must be https, got " + h };
    };
    if (h.ends_with("/")) {
        h.slice(0, h.length() - 1)
    } else {
        h
    }
}

class PosthogFeedback {
    feedback: Feedback[],
    /// The newest event timestamp seen; the `after` cursor once the scan is
    /// complete (see `resume`).
    newest: string?,
    /// Set when the page cap stopped the scan: the `next` link to continue
    /// from on the next call. Until it is null, `after` must not move, or
    /// the unvisited (older) events would be skipped for good.
    resume: string?,
    /// False when the intake is not configured: nothing was scanned, and the
    /// cursor state must be left exactly as it was.
    ran: bool,
}

/// `baml_feedback` events after `after` (ISO timestamp, or null for the
/// last 100), as Feedback, oldest first. Follows PostHog's `next` links so
/// a burst larger than one page is not skipped past when the cursor moves;
/// a run takes at most `max_pages` pages and leaves the rest for the next.
function posthog_feedback(
    after: string?,
    resume: string? = null,
    max_pages: int = 10,
) -> PosthogFeedback {
    if (!posthog_configured()) {
        return PosthogFeedback { feedback: [], newest: after, resume: null, ran: false };
    };
    let key = posthog_env("API_KEY") ?? "";
    let project = posthog_env("PROJECT_ID") ?? "";
    let host = posthog_host();
    //# continue an interrupted scan, or start after the cursor
    let url: string? = resume
        ?? (
            host
                + "/api/projects/"
                + project
                + "/events/?event=baml_feedback&limit=100"
                + (
                    match (after) {
                        null => "",
                        let a: string => "&after=" + url_encode(a),
                    }
                )
        );
    let events: json[] = [];
    let pages = 0;
    while (url != null && pages < max_pages) {
        let out = posthog_get(url ?? "", key, host);
        events = events.concat(baml.json.path_or<json[]>(out, ".results", []));
        url = baml.json.path_or<string?>(out, ".next", null);
        pages += 1;
    }
    let newest = after;
    let feedback: Feedback[] = [];
    // PostHog returns newest first
    for (let ev in events.reverse()) {
        let ts = baml.json.path_or<string>(ev, ".timestamp", "");
        if (ts.length() > 0 && (newest == null || ts > (newest ?? ""))) {
            newest = ts;
        };
        match (feedback_from_posthog(ev)) {
            null => null,
            let fb: Feedback => feedback.push(fb),
        };
    }
    PosthogFeedback { feedback: feedback, newest: newest, resume: url, ran: true }
}

/// One page of the events API. Only the configured host is ever called:
/// a `next` link that points elsewhere is refused.
function posthog_get(url: string, key: string, host: string) -> json {
    if (!url.starts_with(host + "/")) {
        throw IntakeError { message: "refusing to follow a link off " + host };
    };
    let res = baml.http.send(
        baml.http.Request {
            method: "GET",
            url: url,
            headers: { "Authorization": "Bearer " + key, "Accept": "application/json" },
            body: "",
        },
        timeout = baml.time.Duration.from_seconds(30),
    ) catch (_) {
        _ => {
            throw IntakeError { message: "posthog: request failed" };
        },
    };
    let text = res.text() catch (_) {
        _ => "",
    };
    if (!res.ok()) {
        throw IntakeError { message: "posthog: " + res.status_code.to_string() + ": " + tail(text, 2) };
    };
    baml.json.parse(text) catch (_) {
        _ => baml.json.parse("{}"),
    }
}

/// One `baml_feedback` event as a report, or null when it has no text.
/// The id is the CLI's report_id (also in `<BAML_HOME>/feedback.json` on
/// the reporter's machine), so a re-sent report lands on the same row.
function feedback_from_posthog(ev: json) -> Feedback? {
    let title = baml.json.path_or<string>(ev, ".properties.title", "").trim();
    let description = baml.json.path_or<string>(ev, ".properties.description", "").trim();
    if (title.length() == 0 && description.length() == 0) {
        return null;
    };
    let report_id = baml.json.path_or<string?>(ev, ".properties.report_id", null)
        ?? baml.json.path_or<string>(ev, ".id", "");
    let email = baml.json.path_or<string?>(ev, ".properties.email", null);
    let distinct = baml.json.path_or<string?>(ev, ".distinct_id", null);
    Feedback {
        id: "PH-" + report_id,
        title: if (title.length() > 0) {
            title
        } else {
            first_line(description, 120)
        },
        body: description,
        source: FeedbackSource.BamlFeedback,
        author: Author {
            email: email,
            github: null,
            // anonymous reports carry only the CLI's distinct id
            device_id: if (email == null) {
                distinct
            } else {
                null
            },
        },
        toolchain: baml.json.path_or<string?>(ev, ".properties.cli_version", null),
        files: files_from_posthog(baml.json.path_or<json[]>(ev, ".properties.files", [])),
        comments: [],
        issue_ids: [],
    }
}

/// `files` as the CLI sends them: [{name, mime, size_bytes, content_base64}].
/// Text that does not decode is kept as its base64, never dropped.
function files_from_posthog(files: json[]) -> map<string, string> {
    let out: map<string, string> = {};
    for (let f in files) {
        let name = baml.json.path_or<string>(f, ".name", "attachment");
        let b64 = baml.json.path_or<string>(f, ".content_base64", "");
        let content = string.from_utf8(uint8array.from_base64(b64)) catch (_) {
            _ => b64,
        };
        out.set(name, content);
    }
    out
}

/// Where a Slack report came from, for the feedback row's `origin`.
function slack_origin(fb: Feedback, channel: string) -> json {
    {
        "kind": "slack",
        "channel": channel,
        "ts": fb.id.slice(3, fb.id.length()).replace_all("-", "."),
    }
        .to_json()
}

// ==================== Unit tests (token-free) ====================

// ==================== Slack Events HTTP server ====================

/// One process owns the HTTP listener, pipeline and merge queue.
function main_ingress(
    port: int = 8080,
    every_s: int = 300,
    mode: HandleMode = HandleMode.Live,
    stages: string? = null,
) -> never {
    // ATB2_STAGES narrows the loop from the environment (Fly strips quotes
    // from process arguments, so a string literal cannot be passed there).
    let want = stages ?? baml.env.get("ATB2_STAGES") ?? "ingest,triage,verify,handle,merge,intuit";
    spawn { runner_loop(every_s = every_s, mode = mode, stages = want) };
    serve_slack(port = port)
}

/// Also callable alone for local HTTP verification, without starting agents.
function serve_slack(port: int = 8080) -> never {
    let server = baml.http.Server.bind("0.0.0.0:" + port.to_string()) catch (_) {
        _ => baml.sys.panic("cannot bind Slack listener"),
    };
    server.serve(
        slack_http,
        max_body_size = 131072,
        max_connections = 64,
        header_read_timeout = baml.time.Duration.from_seconds(5),
    ) catch (_) {
        _ => baml.sys.panic("Slack listener stopped"),
    }
}

function intake_response(status: int, body: json) -> baml.http.Response {
    baml.http.Response.new(
        status,
        { "content-type": "application/json" },
        baml.json.stringify(body).to_utf8(),
    )
}

function slack_http(req: baml.http.Request) -> baml.http.Response throws never {
    slack_http_inner(req) catch_all (_) {
        _ => baml.http.Response.new(503, { "content-type": "application/json" }, "{}".to_utf8()),
    }
}

function slack_http_inner(req: baml.http.Request) -> baml.http.Response {
    let path = req.url.split("?").at(0) ?? req.url;
    if (path == "/health" && req.method == "GET") {
        return intake_response(200, { "ok": true }.to_json());
    };
    if (path == "/ui") {
        return website_http(req);
    };
    if (path != "/slack/events") {
        return intake_response(404, baml.json.parse("{}"));
    };
    if (req.method != "POST") {
        return intake_response(405, baml.json.parse("{}"));
    };
    slack_events(
        req,
        baml.env.get("ATB2_SLACK_SIGNING_SECRET") ?? baml.env.get("ATB_SLACK_SIGNING_SECRET") ?? "",
        baml.time.Instant.now().to_timestamp_seconds(),
    )
}

function slack_events(req: baml.http.Request, secret: string, now: bigint) -> baml.http.Response {
    if (req.body.to_utf8().length() > 131072) {
        return intake_response(413, baml.json.parse("{}"));
    };
    if (!slack_signature_ok(req, secret, now)) {
        return intake_response(403, baml.json.parse("{}"));
    };
    let envelope = baml.json.parse(req.body) catch (_) {
        _ => {
            return intake_response(400, baml.json.parse("{}"));
        },
    };
    let kind = baml.json.path_or<string>(envelope, ".type", "");
    if (kind == "url_verification") {
        let challenge = baml.json.path_or<string?>(envelope, ".challenge", null);
        if (challenge == null) {
            return intake_response(400, baml.json.parse("{}"));
        };
        return intake_response(200, { "challenge": challenge }.to_json());
    };
    if (kind != "event_callback") {
        return intake_response(200, baml.json.parse("{}"));
    };
    let event = baml.json.path_or<json>(envelope, ".event", baml.json.parse("{}"));
    if (baml.json.path_or<string>(event, ".type", "") == "reaction_added") {
        accept_cancellation(event);
        return intake_response(200, baml.json.parse("{}"));
    };
    if (baml.json.path_or<string>(event, ".type", "") != "app_mention") {
        return intake_response(200, baml.json.parse("{}"));
    };
    let mention = slack_mention(envelope);
    if (mention == null) {
        return intake_response(200, baml.json.parse("{}"));
    };
    let m = mention ?? baml.sys.panic("checked mention");
    // Save before ACK: a failed insert returns 503 so Slack retries. The
    // single insert has a two-second timeout; Slack replies run after ACK.
    let inserted = persist_mention(m);
    if (inserted) {
        spawn {
            slack_post(mention_ack(m), m.thread) catch_all (_) {
                _ => {
                    log.warn({ "slack": "intake acknowledgement failed" });
                    null
                },
            };
        };
    };
    intake_response(200, baml.json.parse("{}"))
}

class SlackMention {
    event_id: string,
    team: string,
    user: string,
    ts: string,
    thread: SlackRef,
    route: MentionRoute,
    /// Text previews of files attached to the message, name -> content.
    files: map<string, string>,
}

/// A top-level mention (not a reply inside a thread) that is not a command:
/// it is a report, and goes straight to the feedback table.
function mention_is_report(m: SlackMention) -> bool {
    m.route.kind == "feedback" && m.ts == m.thread.ts
}

function slack_id(s: string) -> bool {
    s.length() > 0
        && s.length() <= 80
        && s.chars().every((c: string) -> bool {
            (c >= "A" && c <= "Z") || (c >= "a" && c <= "z") || (c >= "0" && c <= "9")
        })
}

function slack_ts(s: string) -> bool {
    s.length() > 0
        && s.length() <= 32
        && s.chars().every((c: string) -> bool {
            (c >= "0" && c <= "9") || c == "."
        })
}

function slack_mention(envelope: json) -> SlackMention? {
    let event = baml.json.path_or<json>(envelope, ".event", baml.json.parse("{}"));
    if (
        baml.json.path_or<string?>(event, ".bot_id", null) != null
            || baml.json.path_or<string?>(event, ".subtype", null) != null
    ) {
        return null;
    };
    let event_id = baml.json.path_or<string>(envelope, ".event_id", "");
    let team = baml.json.path_or<string>(envelope, ".team_id", "");
    let channel = baml.json.path_or<string>(event, ".channel", "");
    let user = baml.json.path_or<string>(event, ".user", "");
    let ts = baml.json.path_or<string>(event, ".ts", "");
    let anchor = baml.json.path_or<string>(event, ".thread_ts", ts);
    if (
        !slack_id(event_id)
            || !slack_id(team)
            || !slack_id(channel)
            || !slack_id(user)
            || !slack_ts(ts)
            || !slack_ts(anchor)
    ) {
        return null;
    };
    let route = parse_mention(baml.json.path_or<string>(event, ".text", ""));
    if (route.text.length() == 0) {
        return null;
    };
    let files: map<string, string> = {};
    for (let f in baml.json.path_or<json[]>(event, ".files", [])) {
        let name = baml.json.path_or<string>(f, ".name", "");
        let preview = baml.json.path_or<string?>(f, ".preview", null) ?? baml.json.path_or<string?>(f, ".plain_text", null);
        if (name.length() > 0 && preview != null && (preview ?? "").length() > 0) {
            files.set(name, preview ?? "");
        };
    }
    SlackMention {
        event_id: event_id,
        team: team,
        user: user,
        ts: ts,
        thread: SlackRef { channel: channel, ts: anchor },
        route: route,
        files: files,
    }
}

function mention_feedback(m: SlackMention) -> Feedback {
    Feedback {
        id: "SL-" + m.team + "-" + m.thread.channel + "-" + m.ts.replace_all(".", "-"),
        title: first_line(m.route.text, 120),
        body: m.route.text,
        source: FeedbackSource.Slack,
        author: Author { email: null, github: null, device_id: "slack:" + m.user },
        toolchain: toolchain_mentioned(m.route.text),
        files: m.files,
        comments: [],
        issue_ids: [],
    }
}

function persist_mention(m: SlackMention) -> bool {
    if (!db_configured()) {
        throw IntakeError { message: "Slack intake needs the store" };
    };
    let table = "";
    let row: map<string, json> = {};
    if (m.route.kind == "babysit" && m.route.pr != null) {
        // Both intake paths share the same request ownership. The worker also
        // coalesces races before it starts a proposal or implementation.
        table = "babysit_requests";
        row
            = {
                "pr": m.route.pr.to_json(),
                "channel": m.thread.channel.to_json(),
                "thread_ts": m.thread.ts.to_json(),
                "requested_by": m.user.to_json(),
                "kind": "babysit".to_json(),
                "dataset": dataset().to_json(),
            };
    } else if (mention_is_report(m)) {
        // "@bammy there is an issue with X": the whole message is the report.
        // It lands in the feedback table now and is triaged on the next run;
        // the thread hears when the issue and its PR exist.
        table = "feedback";
        let fb = mention_feedback(m);
        guard_dataset(fb.id);
        let feedback = baml.json.from_json<map<string, json>>(
            feedback_row(
                fb,
                { "kind": "slack", "team": m.team, "channel": m.thread.channel, "ts": m.thread.ts }
                    .to_json(),
            ),
        );
        feedback.set("slack_thread_ts", m.thread.ts.to_json());
        row = feedback;
        let text = supabase_send(
            "POST",
            "feedback?on_conflict=id",
            baml.json.stringify([row.to_json()]),
            "resolution=ignore-duplicates,return=representation",
            timeout_s = 2,
        );
        let stored = rows_of(text).length() > 0;
        if (stored) {
            log_event(
                "ingested",
                null,
                fb.id,
                { "summary": "Report received from a Slack mention.", "source": "Slack" }.to_json(),
                m.thread.channel,
                m.thread.ts,
            );
        };
        return stored;
    } else if (m.route.kind != "babysit") {
        table = "agent_turns";
        let fb = mention_feedback(m);
        guard_dataset(fb.id);
        let feedback = baml.json.from_json<map<string, json>>(
            feedback_row(
                fb,
                { "kind": "slack", "team": m.team, "channel": m.thread.channel, "ts": m.thread.ts }
                    .to_json(),
            ),
        );
        feedback.set("slack_thread_ts", m.thread.ts.to_json());
        feedback.set("slack_event_id", m.event_id.to_json());
        row
            = {
                "team": m.team.to_json(),
                "channel": m.thread.channel.to_json(),
                "thread_ts": m.thread.ts.to_json(),
                "requested_by": m.user.to_json(),
                "prompt": m.route.text.to_json(),
                "feedback": feedback.to_json(),
                "dataset": dataset().to_json(),
            };
    } else {
        // Informational replies create no queue work.
        return true;
    };
    row.set("slack_event_id", m.event_id.to_json());
    let text = supabase_send(
        "POST",
        table + "?on_conflict=slack_event_id",
        baml.json.stringify([row]),
        "resolution=ignore-duplicates,return=representation",
        timeout_s = 2,
    );
    rows_of(text).length() > 0
}

function mention_ack(m: SlackMention) -> string {
    if (m.route.kind == "shirt") {
        return "Queued your request in this thread.";
    };
    if (mention_is_report(m)) {
        return "Logged as feedback `" + mention_feedback(m).id + "`. Triage runs within five minutes; the issue and its PR land in this thread.";
    };
    if (m.route.kind == "babysit") {
        if (m.route.pr == null) {
            return "Use babysit followed by a GitHub PR URL.";
        };
        return "Queued this PR for babysitting.";
    };
    "Queued your request; I will reply in this thread."
}

function signed_slack_request(
    body: string,
    timestamp: string,
    secret: string,
) -> baml.http.Request {
    baml.http.Request {
        method: "POST",
        url: "/slack/events",
        headers: {
            "X-Slack-Request-Timestamp": timestamp,
            "X-Slack-Signature": "v0=" + hmac_sha256_hex(secret, "v0:" + timestamp + ":" + body),
        },
        body: body,
    }
}

// ==================== GitHub issues ====================

/// One controller poller owns the GitHub cursor in the live runner. No agents,
/// approvals, PostHog credentials, or website OAuth are needed to ingest reports.
function github_intake_loop() -> never {
    let tick = 0;
    while (true) {
        github_intake_tick();
        //# every fifth minute: comments on reported GitHub issues -> the store
        if (tick % 5 == 0) {
            let synced = sync_github_comments() catch (_) {
                _ => {
                    log.warn({ "intake": "github_comments", "result": "sync failed; retrying later" });
                    0
                },
            };
            if (synced > 0) {
                log.info({ "intake": "github_comments", "synced": synced });
            };
        };
        tick += 1;
        baml.sys.sleep(baml.time.Duration.from_seconds(60));
    }
}

function github_intake_tick() -> int {
    if (dataset() != "live" || !db_configured()) {
        return 0;
    };
    if (github_token() == null) {
        log.warn({ "intake": "github", "result": "missing GitHub token" });
        return 0;
    };
    let rows = ingest_github_issues() catch (_) {
        IntakeError => {
            log.warn(
                {
                    "intake": "github",
                    "result": "read failed; cursor retained, retrying next minute",
                },
            );
            return 0;
        },
        StoreError => {
            log.warn(
                {
                    "intake": "github",
                    "result": "store unavailable; unfinished page retries next minute",
                },
            );
            return 0;
        },
    };
    log.info({ "intake": "github", "ingested": rows.length() });
    rows.length()
}

class GithubIssueCursor {
    /// Highest repository issue/PR number from the last completed scan.
    after: int,
    /// Offset page for a bounded scan; new arrivals are picked up next scan.
    page: int,
    newest: int,
}

function github_issue_page(page: int) -> json[] {
    let response = gh(
        [
            "api",
            "--method",
            "GET",
            "repos/" + repo_slug() + "/issues",
            "-f",
            "state=all",
            "-f",
            "sort=created",
            "-f",
            "direction=desc",
            "-f",
            "per_page=100",
            "-f",
            "page=" + page.to_string(),
        ],
    );
    if (!response.ok) {
        throw IntakeError { message: "GitHub issue intake failed; cursor retained (check Issues read permission or rate limits)" };
    };
    baml.json.from_string<json[]>(response.stdout) catch (_) {
        _ => {
            throw IntakeError { message: "Malformed GitHub response; cursor retained" };
        },
    }
}

function feedback_from_github(row: json) -> Feedback? {
    // The repository issues endpoint also returns PRs. Those enter babysitting separately.
    if (baml.json.path_or<json?>(row, ".pull_request", null) != null) {
        return null;
    };
    let number = baml.json.path_or<int>(row, ".number", 0);
    let title = baml.json.path_or<string>(row, ".title", "");
    let author = baml.json.path_or<string>(row, ".user.login", "");
    if (number <= 0 || title.length() == 0 || author.length() == 0) {
        throw IntakeError { message: "Malformed GitHub issue; cursor retained" };
    };
    let url = "https://github.com/" + repo_slug() + "/issues/" + number.to_string();
    //# the comment chain is evidence too: fetch it when the issue has one
    let comments = if (baml.json.path_or<int>(row, ".comments", 0) > 0) {
        github_comments(number) catch (_) {
            IntakeError => {
                log.warn({ "intake": "github", "issue": number, "result": "comments unavailable; ingesting without them" });
                []
            },
        }
    } else {
        []
    };
    Feedback {
        // GH- is reserved for eval reference issues. GHI- is a live report.
        id: "GHI-" + number.to_string(),
        title: title,
        body: (baml.json.path_or<string?>(row, ".body", null) ?? "") + "\n\nGitHub issue: " + url,
        source: FeedbackSource.Github,
        author: Author { email: null, github: author, device_id: null },
        toolchain: null,
        files: {},
        comments: comments,
        issue_ids: [],
    }
}

// ==================== GitHub comments (one way: GitHub -> store) ====================

/// The issue number behind a GHI- report id; null for every other source.
function github_issue_number(feedback_id: string) -> int? {
    if (!feedback_id.starts_with("GHI-")) {
        return null;
    };
    baml.json.from_string<int>(feedback_id.slice(4, feedback_id.length())) catch (_) {
        _ => null,
    }
}

/// The comments on one GitHub issue, oldest first, as store comments. The
/// permalink is the identity a later sync de-duplicates on.
function github_comments(number: int) -> Comment[] {
    let response = gh(
        [
            "api",
            "--method",
            "GET",
            "repos/" + repo_slug() + "/issues/" + number.to_string() + "/comments",
            "-f",
            "per_page=100",
        ],
    );
    if (!response.ok) {
        throw IntakeError { message: "GitHub comments read failed for #" + number.to_string() };
    };
    let raw = baml.json.from_string<json[]>(response.stdout) catch (_) {
        _ => {
            throw IntakeError { message: "Malformed GitHub comments response" };
        },
    };
    raw
        .map((c: json) -> Comment {
            Comment {
                author: baml.json.path_or<string>(c, ".user.login", ""),
                body: baml.json.path_or<string>(c, ".body", ""),
                at: baml.json.path_or<string?>(c, ".created_at", null),
                source: "github",
                url: baml.json.path_or<string?>(c, ".html_url", null),
            }
        })
        .filter((c: Comment) -> bool {
            c.url != null && c.author.length() > 0
        })
}

/// Comments already known, by permalink.
function comment_urls(comments: Comment[]) -> string[] {
    comments
        .map((c: Comment) -> string {
            c.url ?? ""
        })
        .filter((u: string) -> bool {
            u.length() > 0
        })
}

/// Pull new GitHub comments onto every reported issue that came from
/// GitHub: the feedback row keeps the full chain, each linked issue gets
/// the same comments in its own `comments`, and the issue's timeline hears.
/// Never writes back to GitHub. Returns how many comments were new.
function sync_github_comments() -> int {
    if (dataset() != "live" || !db_configured() || github_token() == null) {
        return 0;
    };
    let rows = select(
        "feedback",
        "select=id,comments,issue_ids&source=eq.Github&issue_ids=neq.%7B%7D&dataset="
            + eq(dataset())
            + "&order=received_at.desc&limit=100",
    );
    let synced = 0;
    for (let row in rows) {
        synced += sync_feedback_comments(row) catch (_) {
            IntakeError => 0,
            StoreError => 0,
        };
    }
    synced
}

/// One report's comment chain, brought up to date. Writes are
/// compare-and-set on the previous value so a comment left on the website
/// at the same moment is never overwritten; a lost race retries next sync.
function sync_feedback_comments(row: json) -> int {
    let id = baml.json.path_or<string>(row, ".id", "");
    let number = github_issue_number(id);
    if (number == null) {
        return 0;
    };
    let known = baml.json.path_or<Comment[]>(row, ".comments", []);
    let seen = comment_urls(known);
    let fresh = github_comments(number ?? 0).filter((c: Comment) -> bool {
        !seen.includes(c.url ?? "")
    });
    if (fresh.length() == 0) {
        return 0;
    };
    let old_json = baml.json.path_or<json[]>(row, ".comments", []);
    let updated = rows_of(
        supabase_send(
            "PATCH",
            "feedback?id=" + eq(id) + "&dataset=" + eq(dataset()) + "&comments=" + eq(baml.json.stringify(old_json.to_json())),
            baml.json.stringify({ "comments": known.concat(fresh).to_json() }.to_json()),
            "return=representation",
        ),
    );
    if (updated.length() != 1) {
        log.warn({ "intake": "github_comments", "feedback": id, "result": "row changed underneath; retrying next sync" });
        return 0;
    };
    for (let issue_id in baml.json.path_or<string[]>(row, ".issue_ids", [])) {
        match (load_issue(issue_id)) {
            null => null,
            let issue: Issue => {
                let have = comment_urls(issue.comments);
                let add = fresh.filter((c: Comment) -> bool {
                    !have.includes(c.url ?? "")
                });
                if (add.length() > 0) {
                    let issue_rows = select("issues", "select=comments&id=" + eq(issue_id) + "&dataset=" + eq(dataset()) + "&limit=1");
                    let before = baml.json.path_or<json[]>(issue_rows.at(0) ?? baml.json.parse("{}"), ".comments", []);
                    let changed = rows_of(
                        supabase_send(
                            "PATCH",
                            "issues?id=" + eq(issue_id) + "&dataset=" + eq(dataset()) + "&comments=" + eq(baml.json.stringify(before.to_json())),
                            baml.json.stringify({ "comments": issue.comments.concat(add).to_json() }.to_json()),
                            "return=representation",
                        ),
                    );
                    if (changed.length() == 1) {
                        for (let c in add) {
                            log_event(
                                "comment_synced",
                                issue_id,
                                id,
                                {
                                    "summary": compact_issue_text(c.body, 300),
                                    "author": c.author,
                                    "url": c.url,
                                    "source": "github",
                                    "at": c.at,
                                }
                                    .to_json(),
                                null,
                                null,
                            );
                        }
                    };
                };
                null
            },
        };
    }
    fresh.length()
}

/// Poll at most five pages per pass. Advance only after every row is stored.
/// Initial bootstrap imports the latest 100 repository entries, excluding PRs.
function ingest_github_issues() -> Feedback[] {
    let fresh: Feedback[] = [];
    if (dataset() != "live" || !db_configured() || github_token() == null) {
        return fresh;
    };
    let saved = get_cursor("github_issues");
    let cursor = match (saved) {
        null => GithubIssueCursor { after: 0, page: 1, newest: 0 },
        let value: string => {
            baml.json.from_string<GithubIssueCursor>(value) catch (_) {
                _ => {
                    throw IntakeError { message: "Malformed GitHub cursor; no scan attempted" };
                },
            }
        },
    };
    if (cursor.after < 0 || cursor.newest < cursor.after || cursor.page < 1) {
        throw IntakeError { message: "Invalid GitHub issue cursor" };
    };
    let pages = 0;
    while (pages < 5) {
        let rows = github_issue_page(cursor.page);
        let complete = rows.length() < 100 || saved == null;
        for (let row in rows) {
            let number = baml.json.path_or<int>(row, ".number", 0);
            if (number <= 0) {
                throw IntakeError { message: "Malformed GitHub issue number; cursor retained" };
            };
            if (number > cursor.newest) {
                cursor.newest = number;
            };
            if (number <= cursor.after) {
                complete = true;
            } else {
                match (feedback_from_github(row)) {
                    null => null,
                    let fb: Feedback => {
                        guard_dataset(fb.id);
                        let inserted = rows_of(
                            supabase_send(
                                "POST",
                                "feedback?on_conflict=id",
                                baml.json.stringify(
                                    [
                                        feedback_row(
                                            fb,
                                            {
                                                "kind": "github",
                                                "repository": repo_slug(),
                                                "issue": row,
                                            }
                                                .to_json(),
                                        ),
                                    ],
                                ),
                                "resolution=ignore-duplicates,return=representation",
                            ),
                        );
                        if (inserted.length() > 0) {
                            log_event(
                                "ingested",
                                null,
                                fb.id,
                                {
                                    "summary": "GitHub issue received.",
                                    "source": "Github",
                                    "github_issue": number,
                                }
                                    .to_json(),
                                null,
                                null,
                            );
                            fresh.push(fb);
                        };
                    },
                };
            };
        }
        if (complete) {
            set_cursor(
                "github_issues",
                baml.json.stringify(
                    GithubIssueCursor { after: cursor.newest, newest: cursor.newest, page: 1 }
                        .to_json(),
                ),
            );
            return fresh;
        };
        cursor.page += 1;
        set_cursor("github_issues", baml.json.stringify(cursor.to_json()));
        pages += 1;
    }
    fresh
}
