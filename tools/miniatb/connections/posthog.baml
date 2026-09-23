class PosthogCursor {
    after: string,
    uuid: string,
    until: string,
}

function posthog_env(name: string) -> string {
    baml.env.get("ATB2_POSTHOG_" + name) ?? baml.env.get("ATB_POSTHOG_" + name) ?? ""
}

function posthog_request(
    host: string,
    project: string,
    key: string,
    cursor: PosthogCursor,
) -> baml.http.Request {
    if (
        !["https://us.posthog.com", "https://eu.posthog.com"].includes(host)
            || project == ""
            || project.chars()
                .filter((c: string) -> bool {
                    !"0123456789".includes(c)
                })
                .length()
                > 0
    ) {
        throw baml.errors.InvalidArgument { message: "Invalid PostHog host or project" };
    };
    baml.http.Request {
        method: "POST",
        url: host + "/api/projects/" + project + "/query/",
        headers: { "Authorization": "Bearer " + key, "Content-Type": "application/json" },
        body: baml.json.stringify(
            {
                "refresh": "force_blocking",
                "query": {
                    "kind": "HogQLQuery",
                    "query": "SELECT toString(uuid), toString(timestamp), properties FROM events WHERE event = 'baml_feedback' AND timestamp < parseDateTimeBestEffort({until}, 'UTC') AND (timestamp > parseDateTimeBestEffort({after}, 'UTC') OR (timestamp = parseDateTimeBestEffort({after}, 'UTC') AND toString(uuid) > {uuid})) ORDER BY timestamp, toString(uuid) LIMIT 100",
                    "values": cursor.to_json(),
                },
            }
                .to_json(),
        ),
    }
}

function posthog_feedback(row: json[], project: string) -> Feedback? {
    let raw = row.at(2) ?? baml.sys.panic("Missing PostHog properties");
    let props = baml.json.parse(baml.json.from_json<string>(raw)) catch_all (_) {
        _ => raw
    };
    let title = baml.json.path_or<string>(props, ".title", "").trim();
    let description = baml.json.path_or<string>(props, ".description", "").trim();
    if (title == "" && description == "") {
        return null;
    };
    let id = baml.json.path_or<string>(props, ".report_id", "").trim();
    if (id == "") {
        id = baml.json.from_json<string>(row.at(0) ?? baml.sys.panic("Missing event ID"));
    };
    let files: map<string, string> = {};
    for (let file in baml.json.path_or<json[]>(props, ".files", [])) {
        let name = baml.json.path<string>(file, ".name");
        let encoded = baml.json.path<string>(file, ".content_base64");
        let content = uint8array.from_base64(encoded).to_string() catch_all (_) {
            _ => "[Attachment could not be decoded]",
        };
        files.set(name, content);
    }
    Feedback {
        id: "PH-" + project + "-" + id,
        source: "BamlFeedback",
        description: if (title == "" || title == description) {
            description
        } else if (description == "") {
            title
        } else {
            title + "\n\n" + description
        },
        reported_version: baml.json.path_or<string?>(props, ".cli_version", null),
        files: files,
        source_url: null,
        channel: null,
        thread: null,
    }
}

/// Advances the cursor only after every report on the page is durably queued.
function posthog_page(
    rows: json[][],
    project: string,
    cursor: PosthogCursor,
    enqueue: (Feedback) -> null,
) -> json {
    for (let row in rows) {
        if (row.length() != 3) {
            throw baml.errors.InvalidArgument { message: "Malformed PostHog result" };
        };
        let feedback = posthog_feedback(row, project);
        if (feedback != null) {
            enqueue(feedback);
        };
    }
    if (rows.length() < 100) {
        return { "until": cursor.until, "complete": true }.to_json();
    };
    let last = rows.at(rows.length() - 1) ?? baml.sys.panic("Missing PostHog row");
    {
        "after": baml.json.from_json<string>(last.at(1) ?? baml.sys.panic("Missing timestamp")),
        "uuid": baml.json.from_json<string>(last.at(0) ?? baml.sys.panic("Missing event ID")),
        "until": cursor.until,
        "complete": false,
    }
        .to_json()
}

function poll_posthog() -> null {
    let project = posthog_env("PROJECT_ID");
    let host = if (posthog_env("HOST") == "") {
        "https://us.posthog.com"
    } else {
        posthog_env("HOST")
    };
    let cursor = posthog_state(project, host);
    let response = baml.http.send(
        posthog_request(host, project, posthog_env("API_KEY"), cursor),
        timeout = baml.time.Duration.from_seconds(30),
    );
    if (!response.ok()) {
        throw baml.errors.InvalidArgument { message: "PostHog query failed: " + response.status_code.to_string() };
    };
    let rows = baml.json.path<json[][]>(baml.json.parse(response.text()), ".results");
    let checkpoint = posthog_page(rows, project, cursor, root.run);
    posthog_checkpoint(checkpoint);
    null
}

function posthog_loop() -> null {
    if (posthog_env("API_KEY") == "" || posthog_env("PROJECT_ID") == "") {
        return null;
    };
    while (true) {
        poll_posthog() catch_all (_) {
            _ => {
                log.warn("PostHog intake failed; cursor retained for retry");
                null
            }
        };
        baml.sys.sleep(baml.time.Duration.from_seconds(60));
    }
}
