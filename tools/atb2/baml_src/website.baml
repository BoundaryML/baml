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
    if (operation == "runs") {
        return intake_response(
            200,
            select(
                "runs",
                "select=id,prompt,play_status,report,canary_sha,created_at,feedback_ids&kind=eq.play&dataset="
                    + eq(dataset())
                    + "&order=created_at.desc&limit=100",
            )
                .to_json(),
        );
    };
    if (operation == "run") {
        if (id.length() == 0 || id.length() > 19 || !id.is_ascii_numeric()) {
            return intake_response(400, baml.json.parse("{}"));
        };
        let rows = select(
            "runs",
            "select=id,session_id,prompt,play_status,report,transcript,canary_sha,created_at,feedback_ids,reason&kind=eq.play&id="
                + eq(id)
                + "&dataset="
                + eq(dataset())
                + "&limit=1",
        );
        let enriched: json[] = [];
        for (let row in rows) {
            let ids = baml.json.path_or<string[]>(row, ".feedback_ids", []);
            let reports: json[] = if (ids.length() > 0) {
                select(
                    "feedback",
                    "select=id,title,body,files,issue_ids&dataset="
                        + eq(dataset())
                        + "&id=in."
                        + url_encode(
                            "("
                                + ids
                                    .map((id: string) -> string {
                                        baml.json.stringify(id.to_json())
                                    })
                                    .join(",")
                                + ")",
                        ),
                )
            } else {
                []
            };
            let item = baml.json.from_json<map<string, json>>(row);
            item.set("feedback", reports.to_json());
            enriched.push(item.to_json());
        }
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
