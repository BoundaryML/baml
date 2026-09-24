function example_ticket() -> Ticket {
    Ticket {
        title: "Expected: Unicode escapes decode correctly",
        description: ["A partial escape loses its last digit.", "All four digits should be consumed."],
        investigation: [
            "The escape reader stops early.",
            "The caller skips one more byte.",
            "The native assertion fails.",
            "Consume all four digits.",
        ],
    }
}

test "pure feature requests never generate or replay a reproduction" {
    let generated = false;
    let replayed = false;
    let evidence = collect_reproduction(
        Kind.Feature,
        () -> {
            generated = true;
            Repro {
                kind: Kind.Bug,
                files: {},
                command: "test",
                explanation: "not called",
                blocker: null,
            }
        },
        (_) -> {
            replayed = true;
            Execution { exit_code: 1, stdout: "", stderr: "not called" }
        },
    );
    assert.is_true(!generated && !replayed);
    assert.equal(evidence.repro, null);
    assert.equal(evidence.execution, null);
    assert.equal(stored_repros(evidence.repro, evidence.execution, "0.20.0"), []);
}

test "bug reports still replay and preserve execution evidence" {
    let sample = Repro {
        kind: Kind.Bug,
        files: { "main.baml": "test" },
        command: "test",
        explanation: "Expected behavior fails",
        blocker: null,
    };
    let replayed = false;
    let evidence = collect_reproduction(
        Kind.Bug,
        () -> {
            sample
        },
        (repro) -> {
            assert.equal(repro, sample);
            replayed = true;
            Execution { exit_code: 1, stdout: "assertion failed", stderr: "" }
        },
    );
    assert.is_true(replayed);
    let rows = stored_repros(evidence.repro, evidence.execution, "0.20.0");
    assert.equal(rows.length(), 1);
    assert.equal(baml.json.path<string>(rows[0], ".result"), "fails");
    assert.equal(baml.json.path<string>(rows[0], ".command"), "baml test");
    assert.equal(baml.json.path<string>(rows[0], ".verified_version"), "0.20.0");
}

test "blocked bugs do not replay or fabricate successful evidence" {
    let replayed = false;
    let evidence = collect_reproduction(
        Kind.Bug,
        () -> {
            Repro {
                kind: Kind.Bug,
                files: {},
                command: "test",
                explanation: "blocked",
                blocker: "Unsupported SDK",
            }
        },
        (_) -> {
            replayed = true;
            Execution { exit_code: 0, stdout: "", stderr: "" }
        },
    );
    assert.is_true(!replayed);
    assert.equal(evidence.repro?.blocker, "Unsupported SDK");
    assert.equal(evidence.execution, null);
    let rejected = false;
    stored_repros(evidence.repro, evidence.execution, "0.20.0") catch_all (_) {
        _ => {
            rejected = true;
        }
    };
    assert.is_true(rejected);
}

test "feature tickets roundtrip without evidence and cannot enter the bug fixer" {
    let issue = Issue {
        id: "ISSUE-feature",
        feedback: runtime_fixture("feature"),
        classification: classification_from(null, Kind.Feature),
        workspace: Workspace { id: "feature", version: "0.20.0", revision: "fixture", branch: "bammy/feature" },
        ticket: example_ticket(),
        repro: null,
        execution: null,
    };
    let saved = baml.json.from_json<Issue>(issue.to_json());
    assert.equal(saved.repro, null);
    assert.equal(saved.execution, null);
    let blocked = false;
    perform_fix(saved) catch_all (error) {
        _ => {
            assert.contains(error.to_string(), "Feature requests without a repro");
            blocked = true;
        }
    };
    assert.is_true(blocked);
}

test "ticket format is concise" {
    assert.is_true(valid_ticket(example_ticket()))
}

test "ticket format rejects verbose descriptions" {
    assert.is_true(
        !valid_ticket(
            Ticket {
                title: "Expected: example",
                description: ["one", "two", "three"],
                investigation: ["a", "b", "c", "d"],
            },
        ),
    )
}

test "Slack HMAC matches standard SHA256 fixture" {
    assert.equal(
        hmac_sha256_hex("secret", "v0:1000:body"),
        "6c96d11f3bef2160b1b50c7d16242485c5f2da466b62a9905056a075b997f97a",
    )
}

function signed_request() -> baml.http.Request {
    baml.http.Request {
        method: "POST",
        url: "/slack/events",
        body: "body",
        headers: {
            "X-Slack-Request-Timestamp": "1000",
            "X-Slack-Signature": "v0=6c96d11f3bef2160b1b50c7d16242485c5f2da466b62a9905056a075b997f97a",
        },
    }
}

test "Slack accepts authentic requests" {
    assert.is_true(slack_signature_ok(signed_request(), "secret", 1001n))
}

test "Slack rejects replayed requests" {
    assert.is_true(!slack_signature_ok(signed_request(), "secret", 1301n))
}

test "Slack rejects wrong signatures" {
    assert.is_true(!slack_signature_ok(signed_request(), "wrong", 1001n))
}

test "Slack fails closed without a signing secret" {
    assert.is_true(!slack_signature_ok(signed_request(), "", 1001n))
}

test "JEV classification builds without credentials" {
    let request = classify@build_request(
        Feedback {
            source: "Slack",
            reported_version: null,
            files: {},
            id: "fixture",
            description: "Unicode escapes lose a digit",
            source_url: null,
            channel: null,
            thread: null,
        },
    );
    assert.equal(request.url, "https://api.typesafe.ai/v1/systemone");
    let body = baml.json.parse(request.body);
    let questions = baml.json.path<map<string, json>>(body, ".questions");
    assert.equal(questions.keys().length(), 2);
    assert.equal(baml.json.path<string>(body, ".questions.subsystem.type"), "choice");
    let criteria = baml.json.path<map<string, string>>(body, ".questions.subsystem.criteria");
    assert.equal(criteria.keys().length(), 6);
    for (let subsystem in ["Syntax", "Compiler", "Runtime", "StdLibrary", "Tooling", "Unknown"]) {
        assert.is_true((criteria.get(subsystem) ?? "").length() > 0);
    }
}

test "missing JEV key leaves quick checks undetermined" {
    let classification = classification_from(null, Kind.Feature);
    assert.equal(classification.difficulty, null);
    assert.equal(classification.subsystem, Subsystem.Unknown);
    assert.equal(classification.kind, Kind.Feature);
    assert.equal(
        baml.json.path<json>(classification.to_json(), ".difficulty"),
        baml.json.parse("null"),
    );
    if (!jev_enabled()) {
        assert.equal(quick_checks(runtime_fixture("no-jev")), null);
    }
}

test "JEV judgments are retained with independently determined report kind" {
    let result = classification_from(QuickChecks { difficulty: Difficulty.Easy, subsystem: Subsystem.Compiler }, Kind.Bug);
    assert.equal(result.difficulty, Difficulty.Easy);
    assert.equal(result.subsystem, Subsystem.Compiler);
    assert.equal(result.kind, Kind.Bug);
}

function posthog_fixture() -> json[] {
    [
        "event-1".to_json(),
        "2026-09-19 12:00:00.000000".to_json(),
        {
            "title": "Unicode escapes",
            "description": "Last digit disappears",
            "report_id": "report-1",
            "cli_version": "0.20.0",
            "files": [{ "name": "main.baml", "content_base64": "aGVsbG8=" }],
        }
            .to_json(),
    ]
}

function posthog_mapping_ok() -> bool {
    let feedback = posthog_feedback(posthog_fixture(), "42") ?? baml.sys.panic("Missing report");
    feedback.id == "PH-42-report-1"
        && feedback.source == "BamlFeedback"
        && feedback.description == "Unicode escapes\n\nLast digit disappears"
        && feedback.reported_version == "0.20.0"
        && feedback.files.get("main.baml") == "hello"
        && feedback.channel == null
        && feedback.thread == null
}

test "PostHog preserves report identity source version and attachments" {
    assert.is_true(posthog_mapping_ok())
}

test "PostHog ignores events without feedback text" {
    assert.equal(
        posthog_feedback(["event-1".to_json(), "time".to_json(), baml.json.parse("{}")], "42"),
        null,
    )
}

function posthog_rejects_host() -> bool {
    posthog_request(
        "https://attacker.example",
        "42",
        "fixture",
        PosthogCursor { after: "start", uuid: "", until: "end" },
    );
    false
}

test "PostHog refuses off-host credentials" {
    assert.is_true(
        posthog_rejects_host() catch_all (_) {
            _ => true
        },
    )
}

function posthog_failed_enqueue() -> bool {
    posthog_page([posthog_fixture()], "42", PosthogCursor { after: "start", uuid: "", until: "end" }, (
        feedback: Feedback,
    ) -> null {
        throw baml.errors.InvalidArgument { message: "Queue unavailable" };
    });
    false
}

test "PostHog cannot checkpoint a failed enqueue" {
    assert.is_true(
        posthog_failed_enqueue() catch_all (_) {
            _ => true
        },
    )
}

function posthog_full_page() -> bool {
    let rows: json[][] = [];
    for (let i = 0; i < 100; i += 1) {
        rows.push(posthog_fixture());
    }
    let checkpoint = posthog_page(rows, "42", PosthogCursor { after: "start", uuid: "", until: "end" }, (
        feedback: Feedback,
    ) -> null {
        null
    });
    !baml.json.path<bool>(checkpoint, ".complete")
        && baml.json.path<string>(checkpoint, ".uuid") == "event-1"
}

test "PostHog full pages retain a timestamp and UUID cursor" {
    assert.is_true(posthog_full_page())
}

test "PostHog empty pages finish the bounded window" {
    assert.is_true(
        baml.json.path<bool>(
            posthog_page([], "42", PosthogCursor { after: "start", uuid: "", until: "end" }, (
                feedback: Feedback,
            ) -> null {
                null
            }),
            ".complete",
        ),
    )
}

test "GitHub intake preserves the complete report and canonical source" {
    let row = {
        "number": 42,
        "title": "Broken escape",
        "body": "First line\n\nFull repro",
        "state": "open",
        "html_url": "https://untrusted.example",
    }
        .to_json();
    let feedback = github_feedback(row) ?? baml.sys.panic("Missing feedback");
    assert.equal(feedback.description, "Broken escape\n\nFirst line\n\nFull repro");
    assert.equal(feedback.source_url, "https://github.com/BoundaryML/baml/issues/42");
    assert.equal(feedback.id, "GHI-42");
    assert.equal(
        github_feedback({ "number": 42, "pull_request": baml.json.parse("{}") }.to_json()),
        null,
    );
    assert.equal(github_feedback({ "number": 42, "state": "closed" }.to_json()), null);
}

test "shepherd mapping retains GitHub identities and validates Slack users" {
    let mapping = shepherd_map("codeshaunted:U0123456789,aaronvg:U9876543210");
    assert.equal(shepherd(Subsystem.Compiler, mapping), "codeshaunted");
    assert.equal(shepherd(Subsystem.Unknown, mapping), "aaronvg");
    for (
        let config in [
            "codeshaunted:D0123456789",
            "aaronvg:U0123456789,aaronvg:U9876543210",
            "bad login:U0123456789",
        ]
    ) {
        let rejected = false;
        shepherd_map(config) catch_all (_) {
            _ => {
                rejected = true;
            }
        };
        assert.is_true(rejected);
    }
    assert.equal(slack_text("<@everyone> & title"), "&lt;@everyone&gt; &amp; title");
    assert.equal(
        slack_text("<a> <b> && <!channel>"),
        "&lt;a&gt; &lt;b&gt; &amp;&amp; &lt;!channel&gt;",
    );
}

test "agent events match the website transcript contract" {
    let result = transcript_event(ai.events.FinalProduced { value_json: "{\"summary\":\"done\"}" });
    assert.equal(baml.json.path<string>(result, ".name"), "StructuredOutput");
    assert.equal(baml.json.path<string>(result, ".role"), "assistant");
    assert.equal(baml.json.path<string>(result, ".text"), "{\"summary\":\"done\"}");
    let tool = transcript_event(ai.events.ToolRequested { id: "tool-1", name: "shell", args: { "command": "baml check" } });
    assert.equal(baml.json.path<string>(tool, ".type"), "tool_use");
    assert.equal(baml.json.path<string>(tool, ".name"), "shell");
}

test "timeline does not claim a ticket or verified repro before persistence and review" {
    assert.equal(timeline_kind("ticket", "completed"), "decision");
    assert.equal(timeline_kind("persist", "completed"), "issue_created");
    assert.equal(timeline_kind("reproduce", "failed"), "decision");
    assert.equal(timeline_kind("replay", "completed"), "decision");
    assert.equal(timeline_kind("verified", "completed"), "repro_verified");
    assert.equal(timeline_kind("fix", "completed"), "decision");
    assert.equal(timeline_kind("publish", "completed"), "pr_opened");
    assert.equal(timeline_kind("pipeline", "not_confirmed"), "no_issue");
}

test "private website endpoint rejects unsigned requests" {
    assert.equal(
        website_http(baml.http.Request { method: "POST", url: "/ui", headers: {}, body: "{}" }).status_code,
        403,
    );
}

test "transcript reads ignore partial lines and reject symlinked runs" {
    let base = test_folder();
    defer { baml.fs.remove_dir_all(base); }
    let id = "0123456789abcdef01234567";
    mkdir(base + "/runs/" + id);
    baml.fs.write(base + "/runs/" + id + "/timeline.jsonl", "{\"stage\":\"ticket\"}\n{\"partial\":");
    assert.equal(trace_rows(id, directory = base).length(), 1);
    let link = "abcdef0123456789abcdef01";
    baml.fs.symlink(id, base + "/runs/" + link);
    let rejected = false;
    trace_rows(link, directory = base) catch_all (_) {
        _ => {
            rejected = true;
        }
    };
    assert.is_true(rejected);
}

test "live CLI reports keep the original resolution lookup UUID" {
    let report = runtime_fixture("PH-42-01234567-89ab-4cde-8fab-0123456789ab");
    report.source = "BamlFeedback";
    assert.equal(cli_report_id(report), "01234567-89ab-4cde-8fab-0123456789ab");
    report.source = "Slack";
    assert.equal(cli_report_id(report), null);
    report.source = "BamlFeedback";
    report.id = "PH-42-invalid-report-id";
    assert.equal(cli_report_id(report), null);
}

function mention_fixture(text: string, thread: string? = null, bot: string? = null) -> json {
    {
        "type": "event_callback",
        "event_id": "Ev-mention-test",
        "event": {
            "type": "app_mention",
            "channel": "C0123456789",
            "ts": "1234.5678",
            "text": text,
            "thread_ts": thread,
            "bot_id": bot,
        },
    }
        .to_json()
}

test "Slack mentions preserve the message and enter the durable feedback queue once" {
    let text = "<@U0123456789> Parsing this value fails.\nHere is the repro.";
    let envelope = mention_fixture(text);
    let feedback = slack_feedback(envelope, "C0123456789") ?? baml.sys.panic("Missing feedback");
    assert.equal(feedback.source, "Slack");
    assert.equal(feedback.description, text);
    assert.equal(feedback.channel, "C0123456789");
    assert.equal(feedback.thread, "1234.5678");
    let folder = test_folder();
    defer { baml.fs.remove_dir_all(folder); }
    enqueue(feedback, directory = folder);
    enqueue(feedback, directory = folder);
    assert.equal(baml.fs.read_dir(folder + "/queue").length(), 1);
    assert.equal(
        read_json<Job>(folder + "/queue/" + feedback_id(feedback) + ".json").feedback.description,
        text,
    );
}

test "Slack thread mentions and command-like messages are ordinary feedback" {
    for (
        let message in [
            "<@U0123456789> babysit this PR",
            "<@U0123456789> try this",
            "<@U0123456789> shirt",
        ]
    ) {
        let feedback = slack_feedback(mention_fixture(message, thread = "1111.2222"), "C0123456789")
            ?? baml.sys.panic("Missing feedback");
        assert.equal(feedback.description, message);
        assert.equal(feedback.thread, "1111.2222");
    }
}

test "Slack ignores bot mentions empty messages and unconfigured channels" {
    assert.equal(
        slack_feedback(mention_fixture("report", bot = "B0123456789"), "C0123456789"),
        null,
    );
    assert.equal(slack_feedback(mention_fixture("  "), "C0123456789"), null);
    assert.equal(slack_feedback(mention_fixture("report"), "C9876543210"), null);
    assert.equal(slack_feedback(mention_fixture("report"), ""), null);
    assert.equal(slack_feedback(baml.json.parse("{}"), "C0123456789"), null);
}

test "optional difficulty serializes without optional enum dispatch" {
    assert.equal(difficulty_text(Difficulty.Easy), "Easy");
    assert.equal(difficulty_text(Difficulty.Hard), "Hard");
    assert.equal(difficulty_text(null), null);
}

test "autofix reports configuration blockers before starting an agent" {
    assert.is_true(autofix_blocker(null, "client", "key") != null);
    assert.is_true(autofix_blocker("0", "client", "key") != null);
    assert.is_true(autofix_blocker("1", null, "key") != null);
    assert.is_true(autofix_blocker("1", "client", "") != null);
    assert.is_true(autofix_blocker("1", "client", "key") != null);
    for (
        let repository in [
            "",
            "BoundaryML/baml",
            "boundaryml/other",
            "BOUNDARYML/baml",
            "../baml",
            "example/baml?x",
            "example/",
            "/baml",
            "example/baml/extra",
            "-owner/baml",
            "example/.repo",
            "example/baml\n",
        ]
    ) {
        assert.is_true(autofix_blocker("1", "client", "key", publish_repo = repository) != null);
    }
    assert.equal(autofix_blocker("1", "client", "key", publish_repo = "example/baml"), null);
    assert.equal(
        autofix_blocker("1", "client", "key", publish_repo = "Example-Owner/Baml_fork.test"),
        null,
    );
}

test "first issue announcement includes the draft or reports no PR" {
    assert.equal(
        fix_notice("https://github.com/BoundaryML/baml/pull/123"),
        " Here is my fix: <https://github.com/BoundaryML/baml/pull/123|Draft PR>",
    );
    assert.equal(
        fix_notice(null),
        " Automatic fix did not produce a PR; see the issue timeline for details.",
    );
}

test "dedup retains issues during and after a blocked fix" {
    assert.is_true(dedup_states().includes("in_progress"));
    assert.is_true(dedup_states().includes("deferred"));
    assert.is_true(!dedup_states().includes("cancelled"));
    assert.is_true(!dedup_states().includes("merged"));
}

test "malformed attachments cannot block the PostHog checkpoint" {
    let row = [
        "event-2".to_json(),
        "time".to_json(),
        baml.json.parse(
            `
                {"title":"Report", "files":[{}, null, {"name":"bad.baml", "content_base64":{}},
                  {"name":"empty.baml"}, {"name":"main.baml", "content_base64":"aGVsbG8="}]}
            `,
        ),
    ];
    let seen: Feedback[] = [];
    let checkpoint = posthog_page([row, posthog_fixture()], "42", PosthogCursor { after: "start", uuid: "", until: "end" }, (
        feedback: Feedback,
    ) -> null {
        seen.push(feedback);
        null
    });
    assert.equal(seen.length(), 2);
    assert.equal(seen[0].files.get("main.baml"), "hello");
    assert.equal(seen[0].files.get("empty.baml"), "[Attachment could not be decoded]");
    assert.is_true(baml.json.path<bool>(checkpoint, ".complete"));
}
