test "guide 08.07: Claude Code JSONL becomes typed output and normalized events" {
    let run = root
        .claude_code
        .parse_claude_code_cli_output<ClaudeCodePingResult>(
            root.claude_code.claude_code_cli(),
            claude_code_cli_fixture(),
        );
    assert.equal(run.response.value.answer, "PONG");
    assert.equal(run.response.metadata.provider, "claude-code-cli");
    assert.equal(run.session_id, "fixture-session");
    assert.equal(run.event_delivery, "streaming_jsonl");
    assert
        .equal(
            run.events.map((event: ai.observe.AgentEvent) -> string {
                event.kind()
            }),
            ["model_started", "text_delta", "usage_updated", "run_finished"],
        )
}

test "guide 08.07: Claude Code is a literal local-process agent provider" {
    let provider = root.claude_code.claude_code_cli();
    let task = ClaudeCodePing@task().with_provider(provider);
    let schema = baml.schema.json_schema(reflect.type_of<ClaudeCodePingResult>());
    let args = provider.command_args(task.prompt.text(), baml.json.stringify(schema));
    assert.equal(provider.name(), "claude-code-cli");
    assert.is_true(args.includes("-p"));
    assert.is_true(args.includes("stream-json"));
    assert.is_true(args.includes("--json-schema"));
    assert.is_true(args.includes("--no-session-persistence"));
    assert.is_true(args.includes(task.prompt.text()))
}

test "guide 08.07: Claude Code keeps interactive control on a separate HarnessSession" {
    let harness = root.claude_code.claude_code_cli();
    let session = harness
        .open(
            ai.harness.HarnessOptions {
                cwd: "/tmp",
                permission_mode: "dontAsk",
                sandbox: "safe-mode",
                attributes: {},
            },
        );
    harness.steer(session, "Inspect the requested source before answering.");
    let token = harness.save_session(session);
    let restored = harness.restore_session(token);
    assert.equal(restored.id(), session.id());
    harness.interrupt(restored);
    assert.is_true(restored.stopped())
}

testset "integ-test-guide-08-claude-code-cli" {
    test "guide 08.07 live Claude Code: one Harness runner emits events and returns typed output" {
        let provider = live_claude_code_with_web_fetch();
        let callback_kinds: string[] = [];
        //# Run the real Claude Code harness with a per-run event listener
        let run = ClaudeCodeWebFetch@task().with_provider(provider)
            .run(
                runner = ai.run.Harness<HackerNewsTopStory>.new(provider, cwd = "/tmp", permission_mode = "dontAsk", sandbox = "safe-mode", on_event = (
                    event: ai.observe.AgentEvent,
                ) -> null {
                    callback_kinds.push(event.kind());
                    log.info("Claude Code harness event: " + event.kind());
                    null
                }),
            );
        let kinds = run.events.map((event: ai.observe.AgentEvent) -> string {
            event.kind()
        });
        log.info(run.value);

        assert.is_true(run.value.title.length() > 0);
        assert.is_true(run.value.url.length() > 0);
        assert.equal(run.token.provider, "claude-code-cli");
        assert.equal(run.token.runtime, "claude-code-cli");
        assert.is_true(run.token.session_id.length() > 0);
        let restored = provider.restore_session(run.token);
        assert.equal(restored.id(), run.token.session_id);
        assert.equal(callback_kinds, kinds);
        assert.is_true(kinds.includes("model_started"));
        assert.is_true(kinds.includes("text_delta"));
        assert.equal(kinds[kinds.length() - 1], "run_finished");
        assert.is_true(kinds.includes("tool_call_proposed"))
    }

    test "guide 08.07 live Claude Code: direct call composes native WebFetch with one BAML tool" {
        //# Run the LLM function through its direct-call stand-in
        let story = ClaudeCodeWebFetchWithBamlTool@task().with_provider(live_claude_code_with_web_fetch()).complete(runner = ai.run.Agent<HackerNewsTopStoryWithBamlTool>.new());

        log.info("Claude Code plus BAML tool completed: " + story.to_string());
        assert.is_true(story.title.length() > 0);
        assert.is_true(story.url.length() > 0);
        assert.equal(story.baml_receipt, "baml-tool-receipt-7f3c")
    }
}
