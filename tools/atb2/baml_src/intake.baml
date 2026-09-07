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

testset "intake" {
    test "a baml_feedback event becomes a report with its files decoded" {
        let ev = baml.json.parse(
            `{"id":"ev-1","timestamp":"2026-09-01T10:00:00Z","distinct_id":"anon-1","properties":{"title":"throws panics","description":"index out of bounds","report_id":"r-42","cli_version":"0.17.0","email":"a@b.c","files":[{"name":"main.baml","mime":"text/plain","size_bytes":5,"content_base64":"aGVsbG8="}]}}`,
        );
        let fb = feedback_from_posthog(ev) ?? baml.sys.panic("no feedback");
        assert.equal(fb.id, "PH-r-42");
        assert.equal(fb.title, "throws panics");
        assert.equal(fb.author.email ?? "", "a@b.c");
        assert.equal(fb.author.device_id == null, true);
        assert.equal(fb.toolchain ?? "", "0.17.0");
        assert.equal(fb.files.get("main.baml") ?? "", "hello");
    }

    test "an anonymous report is attributed to the CLI's distinct id" {
        let ev = baml.json.parse(
            `{"id":"ev-2","distinct_id":"anon-2","properties":{"description":"only a description\\nsecond line","report_id":"r-43"}}`,
        );
        let fb = feedback_from_posthog(ev) ?? baml.sys.panic("no feedback");
        assert.equal(fb.title, "only a description");
        assert.equal(fb.author.device_id ?? "", "anon-2");
        assert.equal(author_identity(fb.author), "anon-2");
    }

    test "an empty event is not a report" {
        assert.equal(
            feedback_from_posthog(baml.json.parse(`{"id":"ev-3","properties":{}}`)) == null,
            true,
        );
    }

    test "a Slack origin recovers the message ts" {
        let fb = feedback_from_slack(
            baml.json.parse(`{"type":"message","user":"U1","ts":"1725000000.000100","text":"x"}`),
            "C1",
        )
            ?? baml.sys.panic("no feedback");
        assert.equal(
            baml.json.path_or<string>(slack_origin(fb, "C1"), ".ts", ""),
            "1725000000.000100",
        );
    }

    test "intakes are off without their variables" {
        assert.equal(posthog_configured(), false);
        assert.equal(posthog_feedback(null).feedback.length(), 0);
        assert.equal(posthog_feedback(null).resume == null, true);
        assert.equal(posthog_feedback(null).ran, false);
        assert.equal(slack_feedback(null).ran, false);
        assert.equal(slack_feedback(null).feedback.length(), 0);
        assert.equal(slack_feedback(null).resume == null, true);
    }
}

// ==================== Slack Events HTTP server ====================

/// One process owns the HTTP listener, pipeline and merge queue.
function main_ingress(
    port: int = 8080,
    every_s: int = 300,
    mode: HandleMode = HandleMode.Live,
) -> never {
    spawn { runner_loop(every_s = every_s, mode = mode) };
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
        if (!accept_babysit_approval(event)) {
            accept_approval(event);
        };
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
    SlackMention {
        event_id: event_id,
        team: team,
        user: user,
        ts: ts,
        thread: SlackRef { channel: channel, ts: anchor },
        route: route,
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
        files: {},
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

testset "slack_http" {
    test "HMAC SHA256 matches standard vectors including a long key" {
        assert.equal(
            hmac_sha256_hex("key", "The quick brown fox jumps over the lazy dog"),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8",
        );
        assert.equal(
            hmac_sha256_hex("", ""),
            "b613679a0814d9ec772f95d778c35fc5ff1697c493715653c6c712144292c5ad",
        );
    }
    test "HMAC hashes keys longer than its block" {
        assert.equal(
            hmac_sha256_hex(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "test",
            ),
            "1228d938c9fd0321973d610248b62e03d404697ee5cb7196ac17d7fd18c610ca",
        );
    }
    test "signature covers raw bytes and rejects stale or future replays" {
        let req = signed_slack_request("{}", "1000", "test-secret");
        assert.equal(slack_signature_ok(req, "test-secret", 1000n), true);
        assert.equal(slack_signature_ok(req, "test-secret", 1301n), false);
        assert.equal(slack_signature_ok(req, "test-secret", 699n), false);
        assert.equal(slack_signature_ok(req, "wrong", 1000n), false);
        assert.equal(slack_signature_ok(req, "", 1000n), false);
        let changed = baml.http.Request { method: req.method, url: req.url, headers: req.headers, body: "{ }" };
        assert.equal(slack_signature_ok(changed, "test-secret", 1000n), false);
    }
    test "signed challenge is JSON escaped and does not touch the store" {
        let body = `{"type":"url_verification","challenge":"a\\\"b"}`;
        let out = slack_events(signed_slack_request(body, "1000", "secret"), "secret", 1000n);
        assert.equal(out.status_code, 200);
        assert.equal(
            baml.json.path_or<string>(baml.json.parse(out.text()), ".challenge", ""),
            "a\"b",
        );
        assert.equal(
            slack_events(signed_slack_request("broken", "1000", "secret"), "secret", 1000n).status_code,
            400,
        );
    }
    test "mention commands are anchored and links may be Slack wrapped" {
        assert.equal(
            parse_mention("<@U1> babysit <https://github.com/BoundaryML/baml/pull/42|PR>").pr ?? "",
            "https://github.com/BoundaryML/baml/pull/42",
        );
        assert.equal(parse_mention("a babysit bug").kind, "feedback");
        assert.equal(parse_mention("get me a tshirt").kind, "shirt");
        assert.equal(parse_mention("shirtless function").kind, "feedback");
    }
    test "retry identity and original thread are preserved" {
        let m = slack_mention(
            baml.json.parse(
                `{"event_id":"Ev1","team_id":"T1","event":{"type":"app_mention","channel":"C1","user":"U1","ts":"123.4","thread_ts":"123.0","text":"<@BOT> parser broke"}}`,
            ),
        )
            ?? baml.sys.panic("no mention");
        assert.equal(m.thread.ts, "123.0");
        assert.equal(m.event_id, "Ev1");
        assert.equal(mention_feedback(m).id, "SL-T1-C1-123-4");
        assert.equal(mention_feedback(m).title, "parser broke");
        assert.equal(slack_mention(baml.json.parse(`{"event":{"bot_id":"B1"}}`)) == null, true);
    }
    test "health is public but unsigned event intake is forbidden" {
        assert.equal(
            slack_http(baml.http.Request { method: "GET", url: "/health", headers: {}, body: "" }).status_code,
            200,
        );
        assert.equal(
            slack_http(baml.http.Request { method: "POST", url: "/slack/events", headers: {}, body: "{}" }).status_code,
            403,
        );
    }
}
