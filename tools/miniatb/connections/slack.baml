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

function post_slack(feedback: root.Feedback, text: string) -> string? {
    if (feedback.channel == null || baml.env.get("MINIATB_SLACK_NOTIFY") != "1") {
        return null;
    };
    let response = baml.http.send(
        baml.http.Request {
            method: "POST",
            url: "https://slack.com/api/chat.postMessage",
            headers: {
                "Authorization": "Bearer " + (baml.env.get("ATB_SLACK_BOT_TOKEN") ?? ""),
                "Content-Type": "application/json",
            },
            body: baml.json.stringify(
                {
                    "channel": feedback.channel,
                    "thread_ts": feedback.thread,
                    "text": text,
                    "unfurl_links": false,
                    "unfurl_media": false,
                }
                    .to_json(),
            ),
        },
        timeout = baml.time.Duration.from_seconds(10),
    );
    let body = baml.json.parse(response.text());
    if (!response.ok() || !baml.json.path_or<bool>(body, ".ok", false)) {
        throw baml.errors.InvalidArgument { message: "Slack notification failed" };
    };
    let ts = baml.json.path<string>(body, ".ts");
    root.record(
        "notify",
        "completed",
        { "channel": feedback.channel, "thread": feedback.thread ?? ts }.to_json(),
    );
    ts
}

function notify(feedback: Feedback, text: string) -> null {
    post_slack(feedback, text);
    null
}

function response(status: int, body: json) -> baml.http.Response {
    baml.http.Response.new(
        status,
        { "content-type": "application/json" },
        baml.json.stringify(body).to_utf8(),
    )
}

function http(req: baml.http.Request) -> baml.http.Response throws never {
    accept(req) catch_all (_) {
        _ => baml.http.Response.new(503, { "content-type": "application/json" }, "{}".to_utf8())
    }
}

function accept(req: baml.http.Request) -> baml.http.Response {
    if (req.url == "/ui") {
        return website_http(req);
    };
    if (req.method == "GET" && req.url == "/health") {
        return response(200, { "ok": true }.to_json());
    };
    if (req.url != "/slack/events" || req.method != "POST") {
        return response(404, baml.json.parse("{}"));
    };
    if (
        !slack_signature_ok(
            req,
            baml.env.get("ATB_SLACK_SIGNING_SECRET") ?? "",
            baml.time.Instant.now().to_timestamp_seconds(),
        )
    ) {
        return response(403, baml.json.parse("{}"));
    };
    let envelope = baml.json.parse(req.body);
    let kind = baml.json.path_or<string>(envelope, ".type", "");
    if (kind == "url_verification") {
        return response(200, { "challenge": baml.json.path<string>(envelope, ".challenge") }.to_json());
    };
    if (
        kind == "event_callback"
            && baml.json.path_or<string>(envelope, ".event.type", "") == "reaction_added"
    ) {
        cancel_from_slack(envelope);
        return response(200, baml.json.parse("{}"));
    };
    if let feedback: Feedback = slack_feedback(envelope, slack_channel() ?? "") {
        run(feedback);
    };
    response(200, baml.json.parse("{}"))
}

/// For now, every human app mention is a feedback report, including command-like text.
function slack_feedback(envelope: json, channel: string) -> Feedback? {
    if (
        channel == ""
            || baml.json.path_or<string>(envelope, ".type", "") != "event_callback"
            || baml.json.path_or<string>(envelope, ".event.type", "") != "app_mention"
            || baml.json.path_or<string>(envelope, ".event.channel", "") != channel
            || baml.json.path_or<string?>(envelope, ".event.bot_id", null) != null
    ) {
        return null;
    };
    let text = baml.json.path<string>(envelope, ".event.text");
    if (text.trim() == "") {
        return null;
    };
    Feedback {
        source: "Slack",
        reported_version: null,
        files: {},
        id: baml.json.path<string>(envelope, ".event_id"),
        description: text,
        source_url: null,
        channel: channel,
        thread: baml.json.path_or<string?>(envelope, ".event.thread_ts", null)
            ?? baml.json.path<string>(envelope, ".event.ts"),
    }
}

function serve() -> never {
    mkdir(data_dir());
    spawn {
        worker_loop() catch_all (error) {
            _ => {
                log.error(error);
                baml.sys.exit(1)
            }
        }
    };
    spawn { posthog_loop() };
    spawn { github_loop() };
    baml.http.Server.bind("0.0.0.0:8080").serve(
        http,
        max_body_size = 65536,
        max_connections = 32,
        header_read_timeout = baml.time.Duration.from_seconds(5),
    )
}

function shepherd_map(config: string) -> map<string, string> {
    let result: map<string, string> = {};
    for (let entry in config.split(",")) {
        if (entry.trim() == "") {
            continue;
        };
        let pair = entry.trim().split(":");
        let login = (pair.at(0) ?? "").trim().to_lower_case();
        let user = (pair.at(1) ?? "").trim();
        require(
            pair.length() == 2
                && login != ""
                && login.chars().every((c) -> {
                    c.is_ascii_alphanumeric() || c == "-"
                })
                && slack_user(user)
                && !result.has(login),
            "Invalid shepherd mapping",
        );
        result.set(login, user);
    }
    result
}

function shepherd(subsystem: Subsystem, mapping: map<string, string>) -> string? {
    let owners: string[] = match (subsystem) {
        Subsystem.Syntax => ["aaronvg", "codeshaunted", "2kai2kai2"],
        Subsystem.Compiler => ["codeshaunted", "antoniosarosi", "2kai2kai2"],
        Subsystem.Runtime => ["antoniosarosi", "2kai2kai2", "sxlijin"],
        Subsystem.StdLibrary => ["2kai2kai2", "antoniosarosi", "codeshaunted"],
        Subsystem.Tooling => ["sxlijin", "codeshaunted", "2kai2kai2"],
        Subsystem.Unknown => [],
    };
    owners.concat(["hellovai", "aaronvg"]).concat(mapping.keys()).find((login) -> {
        mapping.has(login)
    })
}

function slack_user(id: string) -> bool {
    (id.starts_with("U") || id.starts_with("W"))
        && id.length() >= 9
        && id.length() <= 20
        && id.chars().every((c) -> {
            c.is_ascii_alphanumeric()
        })
}

function announce_issue(issue: Issue) -> Issue {
    let mapping = shepherd_map(baml.env.get("ATB2_SHEPHERDS") ?? "");
    let owner = shepherd(issue.classification.subsystem, mapping);
    let user = mapping.get(owner ?? "");
    issue.feedback.channel = slack_channel() ?? issue.feedback.channel;
    issue.feedback.thread = null;
    let row = load_issue_row(issue.id);
    let pr = published_pr() ?? baml.json.path_or<string?>(row, ".status.pr", null);
    let text = (
        if (user != null) {
            "<@" + user + "> "
        } else {
            ""
        }
    )
        + slack_text(issue.ticket.title)
        + " <"
        + issue_url(issue.id)
        + "|View issue>"
        + if (issue.classification.kind == Kind.Feature && issue.repro == null) {
            " Feature request ready for review."
        } else {
            fix_notice(pr)
        };
    let ts = post_slack(issue.feedback, text);
    if (ts != null) {
        issue.feedback.thread = ts;
    };
    patch_issue(
        issue.id,
        {
            "shepherd": owner,
            "slack_channel": issue.feedback.channel,
            "slack_ts": issue.feedback.thread,
        }
            .to_json(),
    );
    issue
}

function slack_text(value: string) -> string {
    value.replace_all("&", "&amp;").replace_all("<", "&lt;").replace_all(">", "&gt;")
}

function cancel_from_slack(envelope: json) -> void {
    if (
        !["x", "heavy_multiplication_x"].includes(
            baml.json.path_or<string>(envelope, ".event.reaction", ""),
        )
    ) {
        return;
    };
    let actor = baml.json.path_or<string>(envelope, ".event.user", "");
    let mapping = shepherd_map(baml.env.get("ATB2_SHEPHERDS") ?? "");
    if (
        !mapping.keys().some((login) -> {
            mapping.get(login) == actor
        })
    ) {
        return;
    };
    let channel = baml.json.path_or<string>(envelope, ".event.item.channel", "");
    let ts = baml.json.path_or<string>(envelope, ".event.item.ts", "");
    if (channel != slack_channel()) {
        return;
    };
    require(
        channel.chars().every((c) -> {
            c.is_ascii_alphanumeric()
        })
            && ts != ""
            && ts.chars().every((c) -> {
                "0123456789.".includes(c)
            }),
        "Invalid Slack message",
    );
    let rows = baml.json.from_json<json[]>(
        store(
            "GET",
            "issues?dataset=eq."
                + dataset()
                + "&slack_channel=eq."
                + channel
                + "&slack_ts=eq."
                + ts
                + "&limit=2",
            baml.json.parse("{}"),
        ),
    );
    if (rows.length() != 1) {
        return;
    };
    let row = rows.at(0) ?? baml.sys.panic("Missing issue");
    let id = baml.json.path<string>(row, ".id");
    if (
        ["cancelled", "merged", "shipped"].includes(
            baml.json.path_or<string>(row, ".status.state", ""),
        )
    ) {
        return;
    };
    if (!valid_issue_id(id)) {
        return;
    };
    patch_issue(
        id,
        {
            "status": {
                "state": "cancelled",
                "reason": "Closed from Slack",
                "by": baml.json.path<string>(envelope, ".event.user"),
            },
        }
            .to_json(),
    );
    store(
        "POST",
        "events",
        {
            "kind": "cancelled",
            "issue_id": id,
            "dataset": dataset(),
            "payload": { "source": "Slack", "actor": baml.json.path<string>(envelope, ".event.user") },
        }
            .to_json(),
    );
}

function slack_channel() -> string? {
    baml.env.get("ATB2_SLACK_CHANNEL") ?? baml.env.get("ATB_SLACK_FIX_CHANNEL")
}

function fix_notice(pr: string?) -> string {
    if let url: string = pr {
        " Here is my fix: <" + slack_text(url) + "|Draft PR>"
    } else {
        " Automatic fix did not produce a PR; see the issue timeline for details."
    }
}
