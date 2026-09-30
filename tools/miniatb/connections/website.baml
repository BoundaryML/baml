function trace_stages() -> string[] {
    ["reproduce", "investigate", "ticket", "deduplicate", "fix"]
}

function transcript_event(event: ai.events.Event) -> json {
    let role = "assistant";
    let kind = "text";
    let name: string? = null;
    let text = "";
    match (event) {
        let e: ai.events.UserMessage => {
            role = "user";
            text = e.text();
        },
        let e: ai.events.AssistantMessage => {
            for (let block in e.content) {
                match (block) {
                    let t: ai.content.Text => {
                        text += t.text;
                    },
                    _ => {},
                };
            }
        },
        let e: ai.events.ToolRequested => {
            kind = "tool_use";
            name = e.name;
            text = baml.json.stringify(e.args.to_json());
        },
        let e: ai.events.ToolCompleted => {
            role = "user";
            kind = "tool_result";
            text = e.text();
        },
        let e: ai.events.ToolFailed => {
            role = "user";
            kind = "tool_result";
            text = e.message;
        },
        let e: ai.events.FinalProduced => {
            name = "StructuredOutput";
            text = e.value_json;
        },
        _ => {
            kind = "metadata";
            text = baml.json.stringify(event.to_json());
        },
    };
    { "role": role, "type": kind, "name": name, "text": text, "continuation": false }.to_json()
}

function trace_rows(id: string, directory: string? = null) -> json[] {
    require(valid_run_id(id), "Invalid run ID");
    let base = (directory ?? data_dir()) + "/runs";
    let folder = base + "/" + id;
    let run = baml.fs.read_dir(base).find((e) -> {
        e.name == id
    });
    require(run != null && !(run?.is_symlink ?? true), "Run unavailable");
    let path = folder + "/timeline.jsonl";
    let entry = baml.fs.read_dir(folder).find((e) -> {
        e.name == "timeline.jsonl"
    });
    require(entry != null && !(entry?.is_symlink ?? true), "Transcript unavailable");
    require(baml.fs.size(path) < 32000000, "Transcript exceeds read limit");
    let lines = baml.fs.read(path).split("\n");
    let rows: json[] = [];
    for (let line in lines.slice(0, lines.length() - 1)) {
        if (line.trim() != "") {
            rows.push(baml.json.parse(line));
        };
    }
    rows
}

function trace_meta(id: string, stage: string, rows: json[]) -> json? {
    let selected = rows.filter((r) -> {
        baml.json.path_or<string>(r, ".stage", "") == stage
    });
    if (selected.length() == 0) {
        return null;
    };
    let first = selected.at(0) ?? baml.sys.panic("No trace start");
    let terminal = selected.filter((r) -> {
        ["completed", "failed", "blocked"].includes(baml.json.path_or<string>(r, ".status", ""))
    });
    let job = read_json<Job>(data_dir() + "/queue/" + id + ".json");
    let status = if (terminal.length() > 0) {
        baml.json.path<string>(terminal.at(terminal.length() - 1) ?? first, ".status")
    } else if (job.status == "running") {
        "running"
    } else {
        "interrupted"
    };
    let issues: string[] = [];
    if let linked: string = trace_issue(id) {
        issues.push(linked);
    };
    {
        "id": id + "-" + stage,
        "stage": stage,
        "status": status,
        "reason": null,
        "started_at": baml.time.Instant.parse(baml.json.path<string>(first, ".at")).to_timestamp_seconds(),
        "issue_ids": issues.to_json(),
        "workspace": id,
    }
        .to_json()
}

function traces(body: json) -> json {
    let identity = baml.json.path_or<string>(body, ".id", "");
    if (identity == "") {
        let result: json[] = [];
        if (baml.json.path_or<string>(body, ".pr", "") != "") {
            return result.to_json();
        };
        let requested = baml.json.path_or<string>(body, ".issue_id", "");
        let folder = data_dir() + "/runs";
        if (!baml.fs.exists(folder)) {
            return result.to_json();
        };
        for (let entry in baml.fs.read_dir(folder)) {
            if (entry.is_symlink || !valid_run_id(entry.name)) {
                continue;
            };
            if (requested != "" && requested != trace_issue(entry.name)) {
                continue;
            };
            if (!baml.fs.exists(folder + "/" + entry.name + "/timeline.jsonl")) {
                continue;
            };
            let rows = trace_rows(entry.name);
            for (let stage in trace_stages()) {
                if let meta: json = trace_meta(entry.name, stage, rows) {
                    result.push(meta);
                };
            }
        }
        return result.to_json();
    };
    let parts = identity.split("-");
    let id = parts.at(0) ?? "";
    let stage = parts.at(1) ?? "";
    require(
        parts.length() == 2 && valid_run_id(id) && trace_stages().includes(stage),
        "Invalid transcript ID",
    );
    let rows = trace_rows(id);
    let events = rows
        .filter((r) -> {
            baml.json.path_or<string>(r, ".stage", "") == stage
                && baml.json.path_or<string>(r, ".status", "") == "event"
        })
        .map((r) -> {
            baml.json.path<json>(r, ".detail")
        });
    let offset = baml.json.path_or<int>(body, ".offset", 0);
    require(offset >= 0 && offset <= events.length(), "Invalid offset");
    let end = if (offset + 20 < events.length()) {
        offset + 20
    } else {
        events.length()
    };
    {
        "meta": trace_meta(id, stage, rows),
        "events": events.slice(offset, end).to_json(),
        "offset": end,
        "more": end < events.length(),
        "artifact": trace_artifact(id, stage),
    }
        .to_json()
}

function website_http(req: baml.http.Request) -> baml.http.Response {
    let secret = baml.env.get("ATB2_UI_RUNNER_SECRET") ?? "";
    let signed = baml.http.Request {
        method: req.method,
        url: req.url,
        body: req.body,
        headers: {
            "x-slack-request-timestamp": slack_header(req, "x-atb2-timestamp") ?? "",
            "x-slack-signature": slack_header(req, "x-atb2-signature") ?? "",
        },
    };
    if (
        req.method != "POST"
            || secret.length() < 32
            || !slack_signature_ok(signed, secret, baml.time.Instant.now().to_timestamp_seconds())
    ) {
        return response(403, { "error": "Unauthorized" }.to_json());
    };
    let body = baml.json.parse(req.body);
    require(baml.json.path_or<string>(body, ".dataset", "live") == dataset(), "Wrong dataset");
    let author = baml.json.path_or<string>(body, ".author", "");
    require(
        author != ""
            && author.length() <= 39
            && author.chars().every((c) -> {
                c.is_ascii_alphanumeric() || c == "-"
            }),
        "Invalid author",
    );
    let operation = baml.json.path<string>(body, ".operation");
    if (operation == "traces") {
        return response(200, traces(body));
    };
    if (operation == "export") {
        return response(200, export_linear(baml.json.path<string>(body, ".id")));
    };
    if (operation == "comment") {
        return response(
            200,
            add_comment(baml.json.path<string>(body, ".id"), author, baml.json.path<string>(body, ".body")),
        );
    };
    return response(400, { "error": "Unsupported operation" }.to_json());
}

function trace_issue(id: string) -> string? {
    let folder = run_path(id);
    if (baml.fs.exists(folder + "/linked-issue.json")) {
        return baml.json.path<string>(read_json<json>(folder + "/linked-issue.json"), ".id");
    };
    if (baml.fs.exists(folder + "/issue.json")) {
        return read_json<Issue>(folder + "/issue.json").id;
    };
    null
}

function trace_artifact(id: string, stage: string) -> json? {
    let issue = trace_issue(id);
    if (issue == null) {
        return null;
    };
    let path = run_path(id) + "/fix.json";
    if (stage == "fix" && baml.fs.exists(path)) {
        let fix = read_json<Fix>(path);
        return {
            "title": "Fix result",
            "body": fix.summary + "\n\nTests: " + fix.test_command,
            "status": if (fix.blocker != null) {
                "blocked"
            } else {
                "proposed"
            },
            "url": "/issues/" + issue,
        }
            .to_json();
    };
    null
}
