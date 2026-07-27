class ClaudeCodePing {
    answer: string,
}

class HackerNewsTopStory {
    title: string @description("Title of the current number-one Hacker News story"),
    url: string @description("Absolute destination URL of the story"),
}

class HackerNewsTopStoryWithBamlTool {
    title: string @description("Title of the current number-one Hacker News story"),
    url: string @description("Absolute destination URL of the story"),
    baml_receipt: string @description("Exact receipt returned by record_claude_code_story"),
}

/// Record the story selected by Claude Code and return an application-owned receipt.
function record_claude_code_story(title: string, url: string) -> string throws never {
    log.info("BAML recorded Claude Code story: " + { "title": title, "url": url }.to_string());
    "baml-tool-receipt-7f3c"
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

// Future compiler lowering:
//
//   function ClaudeCodeWebFetchWithBamlTool() -> HackerNewsTopStoryWithBamlTool {
//     client: claude_code_cli
//     prompt: `
//       Use Claude Code's WebFetch tool to inspect Hacker News.
//       Then call record_claude_code_story with the selected story.
//       ${ctx.output_format}
//     `
//     tools: [record_claude_code_story]
//   }
//
// The provider keeps its native WebFetch tool while the task captures the BAML
// application tool. Its CompletionProvider implementation owns the bounded
// strategy needed to finish the complete task as T.
function ClaudeCodeWebFetchWithBamlTool_task(
    provider: root.ai.Provider,
) -> root.ai.Task<HackerNewsTopStoryWithBamlTool> throws never {
    root.ai.task_named<HackerNewsTopStoryWithBamlTool>(provider, "ClaudeCodeWebFetchWithBamlTool", {}, [record_claude_code_story], (
        ctx: baml.llm.Context,
    ) -> baml.llm.PromptAst {
        baml.llm
            .assemble_prompt_ast(
                ["", "\n", ""],
                [
                    "Use your native WebFetch tool to inspect https://news.ycombinator.com/ and find the current number-one story. Then call the BAML application tool record_claude_code_story exactly once with that story's title and absolute destination URL. After receiving its result, return the same title and URL and copy the tool result exactly into baml_receipt. Do not invent the receipt.",
                    ctx.output_format,
                ],
            )
    })
}

// This is the hand-written stand-in for:
//
//   ClaudeCodeWebFetchWithBamlTool()
//
function ClaudeCodeWebFetchWithBamlTool_manual(
    provider: root.claude_code.ClaudeCodeCli,
) -> HackerNewsTopStoryWithBamlTool throws root.ai.CallError | baml.errors.UnknownError | baml.errors.Unsupported {
    //# Direct calls request CompletionProvider, never a guessed runner
    finish_task_direct_manual<HackerNewsTopStoryWithBamlTool>(
            ClaudeCodeWebFetchWithBamlTool_task(provider),
        )
}

function live_claude_code_with_web_fetch() -> root.claude_code.ClaudeCodeCli throws never {
    root.claude_code.ClaudeCodeCli {
        executable: baml.env.get("CLAUDE_CODE_CLI") catch (e) {
            _ => null
        }
            ?? "claude",
        model: "haiku",
        cwd: "/tmp",
        timeout_ms: 60000,
        max_turns: 3,
        max_application_tool_steps: 4,
        max_budget_usd: "0.10",
        permission_mode: "dontAsk",
        tools: ["WebFetch"],
        allowed_tools: ["WebFetch"],
        safe_mode: true,
        persist_session: false,
        harness_sessions: [],
    }
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
        .claude_code
        .parse_claude_code_cli_output<ClaudeCodePing>(
            root.claude_code.claude_code_cli(),
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
    let provider = root.claude_code.claude_code_cli();
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

test "guide 08.07: Claude Code keeps interactive control on a separate HarnessSession" {
    let harness = root.claude_code.claude_code_cli();
    let session = harness
        .open(
            root.ai.HarnessOptions {
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
        let run = ClaudeCodeWebFetch_task(provider)
            .run(
                runner = root.ai.run.Harness<HackerNewsTopStory>.new(provider, cwd = "/tmp", permission_mode = "dontAsk", sandbox = "safe-mode", on_event = (
                    event: root.ai.AgentEvent,
                ) -> null {
                    callback_kinds.push(event.kind());
                    log.info("Claude Code harness event: " + event.kind());
                    null
                }),
            );
        let kinds = run.events.map((event: root.ai.AgentEvent) -> string {
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
        let story = ClaudeCodeWebFetchWithBamlTool_manual(live_claude_code_with_web_fetch());

        log.info("Claude Code plus BAML tool completed: " + story.to_string());
        assert.is_true(story.title.length() > 0);
        assert.is_true(story.url.length() > 0);
        assert.equal(story.baml_receipt, "baml-tool-receipt-7f3c")
    }
}
