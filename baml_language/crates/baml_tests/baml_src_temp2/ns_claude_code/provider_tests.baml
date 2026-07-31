class ClaudeCodeProviderFixture {
    answer: string,
}

function _claude_code_provider_fixture_task(
    provider: ClaudeCodeCli,
) -> ai.Task<ClaudeCodeProviderFixture> throws ai.InvalidRequest {
    ai.task<ClaudeCodeProviderFixture>(provider, (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm.assemble_prompt_ast(["ORIGINAL_CLAUDE_CODE_PROMPT\n", ""], [ctx.output_format])
    })
}

function _claude_code_provider_fixture_state(
    provider: ClaudeCodeCli,
) -> ClaudeCodeAgentConversation throws ai.InvalidRequest {
    match (provider.begin<ClaudeCodeProviderFixture>(_claude_code_provider_fixture_task(provider))) {
        let state: ClaudeCodeAgentConversation => state,
        _ => baml.sys.panic("expected Claude Code Agent conversation"),
    }
}

function _claude_code_provider_fixture_run(
    value: ClaudeCodeProviderFixture,
    assistant_text: string,
) -> ClaudeCodeCliRun<ClaudeCodeProviderFixture> throws never {
    ClaudeCodeCliRun<ClaudeCodeProviderFixture> {
        response: ai.ResponseWithMetadata<ClaudeCodeProviderFixture> {
            value: value,
            metadata: ai.ResponseMetadata {
                provider: "claude-code-cli",
                model: "test",
                request_id: "test-request",
                finish_reason: "completed",
                usage: null,
                attributes: {},
                raw: { "result": assistant_text },
            },
            conversation: null,
        },
        events: [],
        raw_events: [],
        session_id: "test-session",
        event_delivery: "fixture",
    }
}

function _claude_code_wrong_owner_step(
    provider: ClaudeCodeCli,
    conversation: ai.Conversation,
) -> string throws never {
    provider.step<ClaudeCodeProviderFixture>(conversation, []) catch (e) {
        let invalid: ai.InvalidRequest => {
            return "invalid";
        },
        _ => {
            return "other";
        },
    };
    "accepted"
}

function _claude_code_wrong_owner_submit(
    provider: ClaudeCodeCli,
    conversation: ai.Conversation,
) -> string throws never {
    provider.submit(conversation, []) catch (e) {
        let invalid: ai.InvalidRequest => {
            return "invalid";
        },
        _ => {
            return "other";
        },
    };
    "accepted"
}

test "Claude Code conversations require the exact provider instance" {
    let owner = claude_code_cli();
    let other = claude_code_cli();
    let state = _claude_code_provider_fixture_state(owner);
    assert.is_true(owner._instance_id != other._instance_id);
    assert.equal(_claude_code_wrong_owner_step(other, state), "invalid");
    assert.equal(_claude_code_wrong_owner_submit(other, state), "invalid")
}

test "Claude Code portable history includes the task and one final assistant message" {
    let provider = claude_code_cli();
    let task = _claude_code_provider_fixture_task(provider);
    let state = _claude_code_provider_fixture_state(provider);
    let original = task.messages().to_history();
    assert
        .equal(
            state.history.entries.map((message: ai.Message) -> string {
                ai.message_text(message)
            }),
            original.entries.map((message: ai.Message) -> string {
                ai.message_text(message)
            }),
        );
    let request = _claude_code_agent_turn<ClaudeCodeProviderFixture>(provider, state, "");
    assert.contains(request.prompt.text(), "ORIGINAL_CLAUDE_CODE_PROMPT");
    assert.is_true(!request.prompt.text().includes("Conversation so far:"));

    _claude_code_record_final(
        state,
        _claude_code_provider_fixture_run(ClaudeCodeProviderFixture { answer: "done" }, "FINAL_CLAUDE_CODE_ASSISTANT_TEXT"),
    );
    assert.equal(state.history.entries.length(), original.entries.length() + 1);
    assert
        .contains(
            ai.message_text(state.history.entries[state.history.entries.length() - 1]),
            "FINAL_CLAUDE_CODE_ASSISTANT_TEXT",
        )
}

test "Claude Code submit is atomic when a later result ID is invalid" {
    let provider = claude_code_cli();
    let state = _claude_code_provider_fixture_state(provider);
    state.pending_calls
        = [
            ai.tools.ToolCall { id: "first", name: "first_tool", args: {} },
            ai.tools.ToolCall { id: "second", name: "second_tool", args: {} },
        ];
    let original_history_length = state.history.entries.length();
    let rejected = false;
    let _ = provider
        .submit(
            state,
            [
                ai.tools.ToolOk { id: "first", output: { "value": 1 } },
                ai.tools.ToolOk { id: "unknown", output: { "value": 2 } },
            ],
        ) catch (e) {
        let invalid: ai.InvalidRequest => {
            rejected = true;
            state
        },
    };
    assert.is_true(rejected);
    assert.equal(state.history.entries.length(), original_history_length);
    assert.equal(state.pending_calls.length(), 2)
}

test "Claude Code submit records valid parallel results in provider call order" {
    let provider = claude_code_cli();
    let state = _claude_code_provider_fixture_state(provider);
    state.pending_calls
        = [
            ai.tools.ToolCall { id: "first", name: "first_tool", args: {} },
            ai.tools.ToolCall { id: "second", name: "second_tool", args: {} },
        ];
    let original_history_length = state.history.entries.length();
    let _ = provider
        .submit(
            state,
            [
                ai.tools.ToolOk { id: "second", output: { "value": 2 } },
                ai.tools.ToolOk { id: "first", output: { "value": 1 } },
            ],
        );
    assert.equal(state.pending_calls.length(), 0);
    assert.equal(state.history.entries.length(), original_history_length + 2);
    assert
        .contains(
            ai.message_text(state.history.entries[original_history_length]),
            "\"id\":\"first\"",
        );
    assert
        .contains(
            ai.message_text(state.history.entries[original_history_length + 1]),
            "\"id\":\"second\"",
        )
}
