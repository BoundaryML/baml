// Where reports come from.
//
// `baml feedback` (crates/baml_cli/src/feedback_command.rs) posts one PostHog
// event, `baml_feedback`, with title / description / files / report_id and,
// when the reporter is logged in, their email. The intake reads those
// events through PostHog's events API and turns each into a Feedback
// (source BamlFeedback). Slack reports come through slack.baml.
//
//   ATB2_POSTHOG_API_KEY      a personal API key with events:read
//   ATB2_POSTHOG_PROJECT_ID   the project the CLI posts to
//   ATB2_POSTHOG_HOST         API host; default https://us.posthog.com
//
// Each falls back to the ATB_POSTHOG_* name already in Infisical
// (boundary-tools, prod).
//
// The cursor is the timestamp of the newest event seen (`after=` on the
// next call). Event -> Feedback is a pure function with token-free tests.

function posthog_configured() -> bool {
    posthog_env("API_KEY") != null && posthog_env("PROJECT_ID") != null
}

/// ATB2_POSTHOG_<name>, else the old ATB_POSTHOG_<name>.
function posthog_env(name: string) -> string? {
    baml.env.get("ATB2_POSTHOG_" + name) ?? baml.env.get("ATB_POSTHOG_" + name)
}

function posthog_host() -> string {
    let h = (posthog_env("HOST") ?? "https://us.posthog.com").trim();
    if (h.ends_with("/")) {
        h.slice(0, h.length() - 1)
    } else {
        h
    }
}

class PosthogFeedback {
    feedback: Feedback[],
    /// The cursor for the next call: the newest event timestamp seen.
    newest: string?,
}

/// `baml_feedback` events after `after` (ISO timestamp, or null for the
/// last 100), as Feedback, oldest first.
function posthog_feedback(after: string?) -> PosthogFeedback {
    if (!posthog_configured()) {
        return PosthogFeedback { feedback: [], newest: after };
    };
    let key = posthog_env("API_KEY") ?? "";
    let project = posthog_env("PROJECT_ID") ?? "";
    let url = posthog_host()
        + "/api/projects/"
        + project
        + "/events/?event=baml_feedback&limit=100"
        + (
            match (after) {
                null => "",
                let a: string => "&after=" + url_encode(a),
            }
        );
    let res = baml.http.send(
        baml.http.Request {
            method: "GET",
            url: url,
            headers: { "Authorization": "Bearer " + key, "Accept": "application/json" },
            body: "",
        },
        timeout = baml.time.Duration.from_seconds(30),
    ) catch (_) {
        _ => baml.sys.panic("posthog: request failed"),
    };
    let text = res.text() catch (_) {
        _ => "",
    };
    if (!res.ok()) {
        baml.sys.panic("posthog: " + res.status_code.to_string() + ": " + tail(text, 2));
    };
    let out = baml.json.parse(text) catch (_) {
        _ => baml.json.parse("{}"),
    };
    let events = baml.json.path_or<json[]>(out, ".results", []);
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
    PosthogFeedback { feedback: feedback, newest: newest }
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
        assert.equal(slack_feedback(null).feedback.length(), 0);
    }
}
