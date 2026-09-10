// Private website-to-runner API. The website verifies organization membership;
// the HMAC binds its identity and operation to the exact request body.
function website_http(req: baml.http.Request) -> baml.http.Response {
    let secret = baml.env.get("ATB2_UI_RUNNER_SECRET") ?? "";
    let signed = baml.http.Request {
        method: req.method,
        url: req.url,
        body: req.body,
        headers: {
            "X-Slack-Request-Timestamp": slack_header(req, "x-atb2-timestamp") ?? "",
            "X-Slack-Signature": slack_header(req, "x-atb2-signature") ?? "",
        },
    };
    if (
        req.method != "POST"
            || secret.length() < 32
            || !slack_signature_ok(signed, secret, baml.time.Instant.now().to_timestamp_seconds())
    ) {
        return intake_response(403, { "error": "unauthorized" }.to_json());
    };
    let body = baml.json.parse(req.body) catch (_) {
        _ => {
            return intake_response(400, baml.json.parse("{}"));
        }
    };
    let author = baml.json.path_or<string>(body, ".author", "");
    if (
        author.length() == 0
            || author.length() > 39
            || author.chars()
                .filter((c: string) -> bool {
                    !c.is_ascii_alphanumeric() && c != "-"
                })
                .length()
                > 0
    ) {
        return intake_response(400, baml.json.parse("{}"));
    };
    if (baml.json.path_or<string>(body, ".dataset", "live") != dataset()) {
        return intake_response(400, baml.json.parse("{}"));
    };
    let operation = baml.json.path_or<string>(body, ".operation", "");
    let id = baml.json.path_or<string>(body, ".id", "");
    if (operation == "traces") {
        let trace_body = baml.json.from_json<map<string, json>>(body);
        let issue_id = baml.json.path_or<string>(body, ".issue_id", "");
        if (issue_id.length() > 0) {
            let issue = load_issue(issue_id);
            trace_body.set("feedback_ids", (issue?.feedback_ids ?? []).to_json());
        };
        let result = baml.sys.exec(
            "/usr/bin/python3",
            ["-I", "/usr/local/lib/atb2/traces.py", "read"],
            baml.sys.ProcessOptions {
                cwd: atb2_home(),
                env: session_environment(),
                timeout_ms: 10000,
                stdin: baml.json.stringify(trace_body.to_json()),
            },
        );
        if (result.exit_code != 0) {
            return intake_response(503, { "error": "Transcript unavailable" }.to_json());
        };
        let traced = baml.json.parse(result.stdout.to_string());
        if (id.length() == 0) {
            return intake_response(200, traced);
        };
        let enriched = baml.json.from_json<map<string, json>>(traced);
        let proposal = baml.json.path_or<string?>(traced, ".meta.proposal_id", null);
        if (proposal != null) {
            let rows: json[] = select(
                "babysit_proposals",
                "select=id,summary,status&dataset="
                    + eq(dataset())
                    + "&id="
                    + eq(proposal ?? "")
                    + "&limit=1",
            ) catch (_) {
                StoreError => []
            };
            if (rows.length() == 0) {
                let path = automatic_fix_path(proposal ?? "");
                if (baml.fs.exists(path)) {
                    let saved = baml.json.parse(baml.fs.read(path));
                    if (baml.json.path_or<string>(saved, ".dataset", "") != dataset()) {
                        return intake_response(404, baml.json.parse("{}"));
                    };
                    enriched.set(
                        "artifact",
                        {
                            "title": "Fix plan",
                            "body": baml.json.path_or<string>(saved, ".fix.summary", ""),
                            "status": baml.json.path_or<string>(saved, ".fix.status", ""),
                            "url": "/proposals/" + (proposal ?? ""),
                        }
                            .to_json(),
                    );
                };
            };
            if (rows.length() > 0) {
                let row = rows.at(0) ?? baml.json.parse("{}");
                enriched.set(
                    "artifact",
                    {
                        "title": "Proposed fix",
                        "body": baml.json.path_or<string>(row, ".summary", ""),
                        "status": baml.json.path_or<string>(row, ".status", "pending"),
                        "url": "/proposals/" + (proposal ?? ""),
                    }
                        .to_json(),
                );
            };
        } else {
            let issues = baml.json.path_or<string[]>(traced, ".meta.issue_ids", []);
            if (issues.length() == 0) {
                let feedback_ids = baml.json.path_or<string[]>(traced, ".meta.feedback_ids", []);
                for (
                    let fb_id in feedback_ids.slice(
                        0,
                        if (feedback_ids.length() < 10) {
                            feedback_ids.length()
                        } else {
                            10
                        },
                    )
                ) {
                    for (
                        let row in select(
                            "feedback",
                            "select=issue_ids&dataset="
                                + eq(dataset())
                                + "&id="
                                + eq(fb_id)
                                + "&limit=1",
                        )
                    ) {
                        for (let issue_id in baml.json.path_or<string[]>(row, ".issue_ids", [])) {
                            issues.push(issue_id);
                        }
                    }
                }
            };
            if (issues.length() > 0) {
                let issue = load_issue(issues.at(0) ?? "");
                if (issue != null && issue?.resolution_plan != null) {
                    enriched.set(
                        "artifact",
                        {
                            "title": "Proposed resolution",
                            "body": issue.resolution_plan ?? "",
                            "status": baml.json.path_or<string>(issue.status.to_json(), ".state", ""),
                            "url": "/issues/" + issue.id,
                        }
                            .to_json(),
                    );
                };
            };
        };
        return intake_response(200, enriched.to_json());
    };
    if (
        !id.starts_with("ISSUE-")
            || id.length() > 126
            || id.chars()
                .filter((c: string) -> bool {
                    !c.is_ascii_alphanumeric() && c != "-" && c != "_"
                })
                .length()
                > 0
    ) {
        return intake_response(400, baml.json.parse("{}"));
    };
    let issue = load_issue(id);
    if (issue == null) {
        return intake_response(404, baml.json.parse("{}"));
    };
    if (operation == "export") {
        let exported = export_issue_to_linear(issue ?? baml.sys.panic("checked issue"));
        return intake_response(200, exported.to_json());
    };
    if (operation == "comment") {
        let text = baml.json.path_or<string>(body, ".body", "").trim();
        if (text.length() == 0 || text.length() > 5000) {
            return intake_response(400, baml.json.parse("{}"));
        };
        let rows = select("issues", "select=comments&id=" + eq(id) + "&dataset=" + eq(dataset()) + "&limit=1");
        let old = baml.json.path_or<json[]>(rows.at(0) ?? baml.json.parse("{}"), ".comments", []);
        if (
            old
                .filter((c: json) -> bool {
                    baml.json.path_or<string>(c, ".author", "") == author
                        && baml.json.path_or<string>(c, ".body", "") == text
                })
                .length()
                > 0
        ) {
            return intake_response(200, { "ok": true }.to_json());
        };
        let next = old.concat(
            [
                { "author": author, "body": text, "at": baml.time.Instant.now().to_string() }
                    .to_json(),
            ],
        );
        let changed = rows_of(
            supabase_send(
                "PATCH",
                "issues?id="
                    + eq(id)
                    + "&dataset="
                    + eq(dataset())
                    + "&comments="
                    + eq(baml.json.stringify(old.to_json())),
                baml.json.stringify({ "comments": next }.to_json()),
                "return=representation",
            ),
        );
        return intake_response(
            if (changed.length() == 1) {
                200
            } else {
                409
            },
            { "ok": changed.length() == 1 }.to_json(),
        );
    };
    intake_response(400, baml.json.parse("{}"))
}
