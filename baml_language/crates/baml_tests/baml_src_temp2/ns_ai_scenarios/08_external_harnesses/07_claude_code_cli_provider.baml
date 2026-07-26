class ClaudeCodePing {
    answer: string,
}

class HackerNewsTopStory {
    title: string @description("Title of the current number-one Hacker News story"),
    url: string @description("Absolute destination URL of the story"),
}

function ClaudeCodePing_task(
    provider: root.ai.Provider,
) -> root.ai.Task<ClaudeCodePing> throws never {
    root.ai.task_named<ClaudeCodePing>(provider, "ClaudeCodePing", {}, [], (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm
            .assemble_prompt_ast(
                ["", "\n", ""],
                ["Set answer to the exact string PONG.", ctx.output_format],
            )
    })
}

function ClaudeCodeWebFetch_task(
    provider: root.ai.Provider,
) -> root.ai.Task<HackerNewsTopStory> throws never {
    root.ai.task_named<HackerNewsTopStory>(provider, "ClaudeCodeWebFetch", {}, [], (
        ctx: baml.llm.Context,
    ) -> baml.llm.PromptAst {
        baml.llm
            .assemble_prompt_ast(
                ["", "\n", ""],
                [
                    "Use WebFetch to inspect https://news.ycombinator.com/ and return the current number-one story. You must use WebFetch before answering. Return its title and absolute destination URL.",
                    ctx.output_format,
                ],
            )
    })
}

function claude_code_cli_fixture() -> string throws never {
    [
        baml.json
            .stringify(
                {
                    "type": "system",
                    "subtype": "init",
                    "session_id": "fixture-session",
                    "model": "claude-haiku-fixture",
                },
            ),
        baml.json
            .stringify(
                {
                    "type": "stream_event",
                    "session_id": "fixture-session",
                    "event": {
                        "type": "content_block_delta",
                        "delta": { "type": "input_json_delta", "partial_json": "{\"answer\":\"PONG\"}" },
                    },
                },
            ),
        baml.json
            .stringify(
                {
                    "type": "result",
                    "session_id": "fixture-session",
                    "uuid": "fixture-result",
                    "is_error": false,
                    "stop_reason": "tool_use",
                    "terminal_reason": "completed",
                    "result": "{\"answer\":\"PONG\"}",
                    "structured_output": { "answer": "PONG" },
                    "total_cost_usd": 0.001,
                    "usage": { "input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 0 },
                },
            ),
    ]
    .join("\n")
}

test "guide 08.07: Claude Code JSONL becomes typed output and normalized events" {
    let run = root
        .ai
        .parse_claude_code_cli_output<ClaudeCodePing>(
            root.ai.claude_code_cli(),
            claude_code_cli_fixture(),
        );
    assert.equal(run.response.value.answer, "PONG");
    assert.equal(run.response.meta.provider, "claude-code-cli");
    assert.equal(run.session_id, "fixture-session");
    assert.equal(run.event_delivery, "streaming_jsonl");
    assert
        .equal(
            run.events.map((event: root.ai.AgentEvent) -> string {
                event.kind()
            }),
            ["model_started", "text_delta", "usage_updated", "run_finished"],
        )
}

test "guide 08.07: Claude Code is a literal local-process completion provider" {
    let provider = root.ai.claude_code_cli();
    let task = ClaudeCodePing_task(provider);
    let schema = baml.schema.json_schema(reflect.type_of<ClaudeCodePing>());
    let args = provider.command_args(task.prompt.text(), baml.json.stringify(schema));
    assert.equal(provider.name(), "claude-code-cli");
    assert.is_true(args.includes("-p"));
    assert.is_true(args.includes("stream-json"));
    assert.is_true(args.includes("--json-schema"));
    assert.is_true(args.includes("--no-session-persistence"));
    assert.is_true(args.includes(task.prompt.text()))
}

testset "integ-test-guide-08-claude-code-cli" {
    test "guide 08.07 live Claude Code: fetch Hacker News with WebFetch and return typed output" {
        let provider = root.ai.ClaudeCodeCli {
            executable: baml.env.get("CLAUDE_CODE_CLI") catch (e) {
                _ => null
            }
                ?? "claude",
            model: "haiku",
            cwd: "/tmp",
            timeout_ms: 60000,
            max_turns: 3,
            max_budget_usd: "0.10",
            permission_mode: "dontAsk",
            tools: ["WebFetch"],
            allowed_tools: ["WebFetch"],
            safe_mode: true,
            persist_session: false,
        };
        let run = provider.complete_with_events<HackerNewsTopStory>(ClaudeCodeWebFetch_task(provider), on_event = (
            event: json,
        ) -> void {
            log.info(event)
        });
        let kinds = run.events.map((event: root.ai.AgentEvent) -> string {
            event.kind()
        });
        log.info(run.response.value);

        assert.is_true(run.response.value.title.length() > 0);
        assert.is_true(run.response.value.url.length() > 0);
        assert.equal(run.response.meta.provider, "claude-code-cli");
        assert.is_true(run.session_id.length() > 0);
        assert.is_true(kinds.includes("model_started"));
        assert.is_true(kinds.includes("text_delta"));
        assert.equal(kinds[kinds.length() - 1], "run_finished");
        assert.is_true(baml.json.stringify(run.raw_events).includes("WebFetch"))
    }
}
