function github_get(path: string) -> json[] {
    require(path.starts_with("issues?"), "Invalid GitHub intake path");
    let headers = {
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "miniatb",
    };
    headers.set("Authorization", "Bearer " + github_token("read"));
    let result = baml.http.send(
        baml.http.Request {
            method: "GET",
            url: "https://api.github.com/repos/BoundaryML/baml/" + path,
            headers: headers,
            body: "",
        },
        timeout = baml.time.Duration.from_seconds(30),
    );
    require(result.ok(), "GitHub intake failed: " + result.status_code.to_string());
    baml.json.from_json<json[]>(baml.json.parse(result.text()))
}

function github_feedback(row: json) -> Feedback? {
    if (baml.json.path_or<string>(row, ".state", "open") != "open") {
        return null;
    };
    if (baml.json.path_or<json?>(row, ".pull_request", null) != null) {
        return null;
    };
    let number = baml.json.path<int>(row, ".number");
    require(number > 0, "Invalid GitHub issue number");
    Feedback {
        source: "Github",
        id: "GHI-" + number.to_string(),
        description: baml.json.path<string>(row, ".title")
            + "\n\n"
            + (baml.json.path_or<string?>(row, ".body", null) ?? ""),
        source_url: "https://github.com/BoundaryML/baml/issues/" + number.to_string(),
        reported_version: null,
        files: {},
        channel: null,
        thread: null,
    }
}

function poll_github() -> void {
    let path = data_dir() + "/github.json";
    let now = baml.time.Instant.now();
    let state = if (baml.fs.exists(path)) {
        read_json<json>(path)
    } else {
        {
            "since": (
                baml.env.get("MINIATB_GITHUB_SINCE")
                    ?? (now - baml.time.Duration.from_seconds(3600)).to_string()
            ),
            "page": 1,
            "until": now.to_string(),
        }
            .to_json()
    };
    let since = baml.json.path<string>(state, ".since");
    baml.time.Instant.parse(since);
    let page = baml.json.path<int>(state, ".page");
    require(page > 0, "Invalid GitHub page");
    let rows = github_get(
        "issues?state=all&sort=updated&direction=asc&per_page=100&page="
            + page.to_string()
            + "&since="
            + since.replace("+", "%2B"),
    );
    for (let row in rows) {
        if let feedback: Feedback = github_feedback(row) {
            run(feedback);
        };
    }
    write_json(
        path,
        if (rows.length() == 100) {
            { "since": since, "page": page + 1, "until": baml.json.path<string>(state, ".until") }
                .to_json()
        } else {
            {
                "since": (
                    baml.time.Instant.parse(baml.json.path<string>(state, ".until"))
                        - baml.time.Duration.from_seconds(300)
                )
                    .to_string(),
                "page": 1,
                "until": now.to_string(),
            }
                .to_json()
        },
    );
}

function github_loop() -> null {
    if (baml.env.get("MINIATB_GITHUB_INGRESS") != "1") {
        return null;
    };
    while (true) {
        poll_github() catch_all (_) {
            _ => {
                log.warn("GitHub intake failed; retrying without advancing cursor");
            }
        };
        baml.sys.sleep(baml.time.Duration.from_seconds(60));
    }
}

/// Credentials stay in the controller, outside agent sandboxes.
function github_token(mode: string) -> string {
    checked(
        "node",
        [app_dir() + "/deployment/github-token.mjs", mode],
        options = baml.sys.ProcessOptions {
            env: {
                "PATH": "/usr/local/bin:/usr/bin:/bin",
                "MINIATB_PUBLISH_REPO": baml.env.get("MINIATB_PUBLISH_REPO") ?? "",
                "BAMMY_GITHUB_APP_CLIENT_ID": baml.env.get("BAMMY_GITHUB_APP_CLIENT_ID") ?? "",
                "BAMMY_GITHUB_APP_PRIVATE_KEY": baml.env.get("BAMMY_GITHUB_APP_PRIVATE_KEY") ?? "",
            },
            timeout_ms: 50000,
        },
    )
}
