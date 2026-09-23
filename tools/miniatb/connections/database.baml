function upsert(table: string, row: json) -> null {
    let base = baml.env.get("FEEDBACK_SUPABASE_URL") ?? "";
    let key = baml.env.get("FEEDBACK_SUPABASE_KEY") ?? "";
    if (!base.starts_with("https://") || key == "") {
        throw baml.errors.InvalidArgument { message: "Supabase is not configured" };
    };
    let response = baml.http.send(
        baml.http.Request {
            method: "POST",
            url: base + "/rest/v1/" + table + "?on_conflict=id",
            headers: {
                "apikey": key,
                "Authorization": "Bearer " + key,
                "Content-Type": "application/json",
                "Prefer": "resolution=merge-duplicates,return=minimal",
            },
            body: baml.json.stringify([row].to_json()),
        },
        timeout = baml.time.Duration.from_seconds(15),
    );
    if (!response.ok()) {
        throw baml.errors.InvalidArgument { message: "Supabase write failed: " + response.status_code.to_string() };
    };
    null
}

function issue_url(id: string) -> string {
    (baml.env.get("ATB2_UI_URL") ?? "") + "/issues/" + id
}

function difficulty_text(difficulty: Difficulty?) -> string? {
    match (difficulty) {
        let value: Difficulty => value.to_string(),
        null => null,
    }
}

function save_issue(issue: root.Issue) -> null {
    let feedback_id = report_id(issue.feedback);
    upsert(
        "issues",
        {
            "id": issue.id,
            "kind": issue.classification.kind.to_string().to_lower_case(),
            "title": issue.ticket.title,
            "description": issue.ticket.description.join(" ")
                + "\n\n## Investigation\n"
                + issue.ticket.investigation.join(" "),
            "resolution_plan": issue.ticket.investigation.join(" "),
            "subsystem": issue.classification.subsystem.to_string(),
            "version": issue.workspace.version,
            "feedback_ids": [feedback_id],
            "dataset": dataset(),
            "difficulty": difficulty_text(issue.classification.difficulty),
            "status": { "state": "open" },
            "comments": baml.json.parse("[]"),
            "slack_channel": issue.feedback.channel,
            "slack_ts": issue.feedback.thread,
            "repros": [
                {
                    "files": issue.repro.files.to_json(),
                    "command": "baml " + issue.repro.command,
                    "expectation": if (issue.repro.command == "test") {
                        { "check": "requires_inspection", "instructions": issue.repro.explanation }
                            .to_json()
                    } else {
                        { "check": "should_compile" }.to_json()
                    },
                    "result": if (issue.execution.exit_code == 0) {
                        "passes"
                    } else {
                        "fails"
                    },
                    "verified_version": issue.workspace.version,
                    "observed": issue.execution.stdout + "\n" + issue.execution.stderr,
                },
            ],
        }
            .to_json(),
    );
    save_report(issue.feedback, linked = [issue.id]);
    null
}

function dataset() -> string {
    let value = baml.env.get("MINIATB_DATASET") ?? "eval";
    require(["live", "eval"].includes(value), "Invalid dataset");
    value
}

function report_id(feedback: Feedback) -> string {
    if (dataset() == "live") {
        if let id: string = cli_report_id(feedback) {
            return "PH-" + id;
        };
    };
    "FB-" + dataset() + "-mini-" + feedback_id(feedback)
}

function issue_id(id: string) -> string {
    (
        if (dataset() == "eval") {
            "GH-eval-mini-"
        } else {
            "ISSUE-mini-"
        }
    )
        + id
}

function store(method: string, path: string, body: json) -> json {
    let key = baml.env.get("FEEDBACK_SUPABASE_KEY") ?? "";
    let base = baml.env.get("FEEDBACK_SUPABASE_URL") ?? "";
    require(key != "" && base.starts_with("https://"), "Supabase is not configured");
    let response = baml.http.send(
        baml.http.Request {
            method: method,
            url: base + "/rest/v1/" + path,
            headers: {
                "apikey": key,
                "Authorization": "Bearer " + key,
                "Content-Type": "application/json",
                "Prefer": "return=representation",
            },
            body: if (method == "GET") {
                ""
            } else {
                baml.json.stringify(body)
            },
        },
        timeout = baml.time.Duration.from_seconds(15),
    );
    require(response.ok(), "Store request failed: " + response.status_code.to_string());
    baml.json.parse(response.text())
}

function save_report(feedback: Feedback, linked: string[] = []) -> null {
    upsert(
        "feedback",
        {
            "id": report_id(feedback),
            "title": feedback.description.slice(0, 100),
            "body": feedback.description,
            "source": feedback.source,
            "dataset": dataset(),
            "author_device_id": feedback.id,
            "files": feedback.files.to_json(),
            "toolchain": feedback.reported_version,
            "issue_ids": linked.to_json(),
            "origin": feedback.to_json(),
        }
            .to_json(),
    );
    null
}

function timeline_kind(stage: string, status: string) -> string {
    if (stage == "pipeline" && ["blocked", "not_reproduced", "not_confirmed"].includes(status)) {
        return "no_issue";
    };
    if (stage == "reproduce") {
        return if (status == "running") {
            "enrich_started"
        } else if (status == "completed") {
            "enriched"
        } else {
            "decision"
        };
    };
    if (stage == "verified") {
        return "repro_verified";
    };
    if (stage == "investigate") {
        return if (status == "running") {
            "investigation_started"
        } else if (status == "failed") {
            "investigation_failed"
        } else {
            "investigated"
        };
    };
    if (stage == "persist" && status == "completed") {
        return "issue_created";
    };
    if (stage == "notify" && status == "completed") {
        return "slack_notified";
    };
    if (stage == "fix" && status == "running") {
        return "implementation_started";
    };
    if (stage == "publish" && status == "completed") {
        return "pr_opened";
    };
    "decision"
}

function store_stage(stage: string, status: string, detail: json) -> void {
    let input = read_json<RunInput>(run_dir() + "/input.json");
    let saved = run_dir() + "/issue.json";
    let id: string? = if (baml.fs.exists(saved)) {
        read_json<Issue>(saved).id
    } else {
        null
    };
    store(
        "POST",
        "events",
        {
            "kind": timeline_kind(stage, status),
            "feedback_id": report_id(input.feedback),
            "issue_id": id,
            "dataset": dataset(),
            "payload": {
                "step": stage,
                "decision": status,
                "reason": baml.json.path_or<string>(detail, ".reason", status),
                "summary": baml.json.path_or<string>(detail, ".reason", status),
                "evidence": detail,
                "stage": stage,
                "run_id": input.workspace.id,
            },
        }
            .to_json(),
    );
}

function valid_issue_id(id: string) -> bool {
    id == issue_id(id.split("-").at(id.split("-").length() - 1) ?? "")
        && valid_run_id(id.split("-").at(id.split("-").length() - 1) ?? "")
}

function load_issue_row(id: string) -> json {
    require(valid_store_issue_id(id), "Invalid issue ID");
    let rows = baml.json.from_json<json[]>(
        store(
            "GET",
            "issues?id=eq." + id + "&dataset=eq." + dataset() + "&limit=1",
            baml.json.parse("{}"),
        ),
    );
    require(rows.length() == 1, "Issue not found");
    rows.at(0) ?? baml.sys.panic("Missing issue")
}

function add_comment(id: string, author: string, body: string) -> json {
    require(body.trim() != "" && body.length() <= 5000, "Invalid comment");
    let row = load_issue_row(id);
    let comments = baml.json.path_or<json[]>(row, ".comments", []);
    comments.push(
        {
            "author": author,
            "body": body.trim(),
            "source": "website",
            "at": baml.time.Instant.now().to_string(),
        }
            .to_json(),
    );
    // Match updated_at to prevent overwriting a concurrent writer.
    let updated = baml.json.path<string>(row, ".updated_at");
    let rows = baml.json.from_json<json[]>(
        store(
            "PATCH",
            "issues?id=eq."
                + id
                + "&dataset=eq."
                + dataset()
                + "&updated_at=eq."
                + updated.replace("+", "%2B"),
            { "comments": comments.to_json(), "updated_at": baml.time.Instant.now().to_string() }
                .to_json(),
        ),
    );
    require(rows.length() == 1, "Issue changed; refresh and retry");
    { "ok": true }.to_json()
}

function patch_issue(id: string, values: json) -> void {
    require(valid_issue_id(id), "Invalid MiniATB issue ID");
    let rows = baml.json.from_json<json[]>(
        store(
            "PATCH",
            "issues?id=eq."
                + id
                + "&dataset=eq."
                + dataset()
                + "&status->>state=not.in.(cancelled,merged,shipped)",
            values,
        ),
    );
    require(rows.length() == 1, "Issue is missing or terminal; refusing to overwrite");
}

function issue_cancelled(id: string) -> bool {
    baml.json.path_or<string>(load_issue_row(id), ".status.state", "") == "cancelled"
}

class ExistingIssue {
    id: string,
    description: string,
}

function existing_issues() -> ExistingIssue[] {
    baml.json.from_json<ExistingIssue[]>(
        store(
            "GET",
            "issues?dataset=eq."
                + dataset()
                + "&status->>state=in.("
                + dedup_states().join(",")
                + ")&order=updated_at.desc&limit=100&select=id,description",
            baml.json.parse("{}"),
        ),
    )
}

function link_duplicate(issue: Issue, id: string) -> void {
    let row = load_issue_row(id);
    let ids = baml.json.path_or<string[]>(row, ".feedback_ids", []);
    let report = report_id(issue.feedback);
    if (!ids.includes(report)) {
        ids.push(report);
        let repros = baml.json.path_or<json[]>(row, ".repros", []);
        repros.push(
            {
                "files": issue.repro.files.to_json(),
                "command": "baml " + issue.repro.command,
                "expectation": if (issue.repro.command == "test") {
                    { "check": "requires_inspection", "instructions": issue.repro.explanation }
                        .to_json()
                } else {
                    { "check": "should_compile" }.to_json()
                },
                "verified_version": issue.workspace.version,
                "result": if (issue.execution.exit_code == 0) {
                    "passes"
                } else {
                    "fails"
                },
                "observed": issue.execution.stdout + "\n" + issue.execution.stderr,
            }
                .to_json(),
        );
        let updated = baml.json.path<string>(row, ".updated_at");
        let changed = baml.json.from_json<json[]>(
            store(
                "PATCH",
                "issues?id=eq."
                    + id
                    + "&dataset=eq."
                    + dataset()
                    + "&updated_at=eq."
                    + updated.replace("+", "%2B"),
                {
                    "feedback_ids": ids.to_json(),
                    "repros": repros.to_json(),
                    "updated_at": baml.time.Instant.now().to_string(),
                }
                    .to_json(),
            ),
        );
        require(changed.length() == 1, "Duplicate issue changed; refusing to overwrite");
    };
    save_report(issue.feedback, linked = [id]);
}

function valid_store_issue_id(id: string) -> bool {
    (id.starts_with("ISSUE-") || id.starts_with("GH-"))
        && id.length() > 6
        && id.length() <= 126
        && id.chars().every((c) -> {
            c.is_ascii_alphanumeric() || c == "-" || c == "_"
        })
}

function cli_report_id(feedback: Feedback) -> string? {
    if (
        feedback.source != "BamlFeedback"
            || !feedback.id.starts_with("PH-")
            || feedback.id.length() < 39
    ) {
        return null;
    };
    let id = feedback.id.slice(feedback.id.length() - 36, feedback.id.length()).to_lower_case();
    let pieces = id.split("-");
    if (
        pieces.map((part) -> {
            part.length()
        })
            != [8, 4, 4, 4, 12]
            || !pieces.join("").chars().every((c) -> {
                "0123456789abcdef".includes(c)
            })
    ) {
        return null;
    };
    id
}

function dedup_states() -> string[] {
    ["open", "awaiting_approval", "approved", "in_progress", "deferred"]
}
