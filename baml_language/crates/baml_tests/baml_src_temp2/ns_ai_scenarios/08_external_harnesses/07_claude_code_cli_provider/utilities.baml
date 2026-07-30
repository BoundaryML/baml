class ClaudeCodePingResult {
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

function ClaudeCodePing() -> ClaudeCodePingResult {
    provider: ai.testing.fake_output_provider(`{"answer":"PONG"}`)
    prompt: `
        Set answer to the exact string PONG.
        ${ctx.output_format}
    `
}

function ClaudeCodeWebFetch() -> HackerNewsTopStory {
    provider: ai.testing.fake_output_provider(
        `{"title":"fixture","url":"https://example.com"}`
    )
    prompt: `
        Use WebFetch to inspect https://news.ycombinator.com/ and return the
        current number-one story. You must use WebFetch before answering.
        Return its title and absolute destination URL.
        ${ctx.output_format}
    `
}

function ClaudeCodeWebFetchWithBamlTool() -> HackerNewsTopStoryWithBamlTool {
    provider: ai.testing.fake_output_provider(
        `{"title":"fixture","url":"https://example.com","baml_receipt":"baml-tool-receipt-7f3c"}`
    )
    prompt: `
        Use your native WebFetch tool to inspect https://news.ycombinator.com/
        and find the current number-one story. Then call the BAML application
        tool record_claude_code_story exactly once with that story's title and
        absolute destination URL. After receiving its result, return the same
        title and URL and copy the tool result exactly into baml_receipt. Do not
        invent the receipt.
        ${ctx.output_format}
    `
    tools: [record_claude_code_story]
}

// The provider keeps its native WebFetch tool while the task captures the BAML
// application tool. The Agent runner owns the bounded loop needed to finish the
// complete task as T.
function live_claude_code_with_web_fetch() -> root.claude_code.ClaudeCodeCli throws never {
    root.claude_code.ClaudeCodeCli {
        _instance_id: baml.id.new(),
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
