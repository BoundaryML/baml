// Regression coverage for appending messages to provider-owned conversations.
function _append_test_task(provider: ai.Provider) -> ai.Task<string> throws never {
    ai.task<string>(provider, (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm.assemble_prompt_ast(["Return done.\n", ""], [ctx.output_format])
    })
}

function _append_test_provider(
    name: string = "append-test",
    calls: ai.tools.ToolCall[] = [],
) -> ai.testing.FakeToolProvider throws never {
    ai.testing.FakeToolProvider {
        provider_name: name,
        tool_calls: calls,
        final_output: `"done"`,
        provider_tools: [],
    }
}

test "conversation append preserves output type and resumes through Agent" {
    let provider = _append_test_provider();
    let task = _append_test_task(provider);
    let first = ai.internal._require_done(task.run(runner = ai.run.Agent<string>.new()));
    let fingerprint = first.conversation.output_type_fingerprint();
    let continued = first.conversation.append_message(ai.ChatMessage.user("one more question"));
    assert.equal(continued.output_type_fingerprint(), fingerprint);
    assert.equal(continued.provider().name(), "append-test");
    assert.equal(continued.messages().items().length(), 2);
    let second = ai
        .internal
        ._require_done(task.run(runner = ai.run.Agent<string>.new(conversation = continued)));
    assert.equal(second.value, "done");
    assert.equal(second.conversation.messages().items().length(), 2)
}

test "conversation append validates the whole batch before mutation" {
    let provider = _append_test_provider();
    let state = match (provider.begin<string>(_append_test_task(provider))) {
        let value: ai.testing.FakeToolConversation => value,
        _ => baml.sys.panic("expected fake tool conversation"),
    };
    let batch = ai.MessageHistory {
        entries: [ai.ChatMessage.user("valid first"), ai.ChatMessage.assistant("invalid second")],
    };
    let rejected = false;
    let _ = provider.append_messages(state, batch) catch (e) {
        let invalid: ai.InvalidRequest => {
            rejected = true;
            state
        },
        _ => baml.sys.panic("unexpected append validation error"),
    };
    assert.is_true(rejected);
    assert.equal(state.history.entries.length(), 1)
}

test "conversation append reports Unsupported when the owner lacks the capability" {
    let provider = ai.testing.fake_output_provider(`"done"`);
    let conversation = provider.begin<string>(_append_test_task(provider));
    let rejected = false;
    let _ = conversation
        .append_messages(
            ai.MessageHistory { entries: [ai.ChatMessage.user("follow up")] },
        ) catch (e) {
        let unsupported: baml.errors.Unsupported => {
            rejected = true;
            conversation
        },
        _ => baml.sys.panic("unexpected unsupported append error"),
    };
    assert.is_true(rejected);
    assert.equal(conversation.messages().items().length(), 1)
}

test "conversation append rejects unresolved fake tool calls" {
    let provider = _append_test_provider(calls = [ai.tools.ToolCall { id: "pending", name: "lookup", args: {} }]);
    let state = match (provider.begin<string>(_append_test_task(provider))) {
        let value: ai.testing.FakeToolConversation => value,
        _ => baml.sys.panic("expected fake tool conversation"),
    };
    let _ = provider.step<string>(state, []);
    let rejected = false;
    let _ = provider.append_message(state, ai.ChatMessage.user("too soon")) catch (e) {
        let invalid: ai.InvalidRequest => {
            rejected = true;
            state
        },
        _ => baml.sys.panic("unexpected pending append error"),
    };
    assert.is_true(rejected);
    assert.equal(state.history.entries.length(), 1)
}

test "retry preserves exact append through its conversation wrapper" {
    let inner = _append_test_provider(name = "retry-member");
    let provider = ai.retry(inner, 2);
    let state = match (provider.begin<string>(_append_test_task(provider))) {
        let value: ai.RetryConversation => value,
        _ => baml.sys.panic("expected retry conversation"),
    };
    let fingerprint = state.output_type_fingerprint();
    let appended = provider.append_message(state, ai.ChatMessage.user("retry follow up"));
    match (appended) {
        let exact: ai.RetryConversation => {
            assert.equal(exact.output_type_fingerprint(), fingerprint);
            assert.equal(exact.provider().name(), "retry(retry-member)");
            assert.equal(ai.message_text(exact.messages().items()[1]), "retry follow up")
        },
        _ => assert.is_true(false),
    }
}

test "fallback pins the exact member after append" {
    let inner = _append_test_provider(name = "fallback-member");
    let provider = ai.fallback([inner]);
    let state = match (provider.begin<string>(_append_test_task(provider))) {
        let value: ai.FallbackConversation => value,
        _ => baml.sys.panic("expected fallback conversation"),
    };
    assert.equal(state.progressed, false);
    let appended = provider.append_message(state, ai.ChatMessage.user("fallback follow up"));
    match (appended) {
        let exact: ai.FallbackConversation => {
            assert.equal(exact.progressed, true);
            assert.equal(exact.member_index, 0);
            assert.equal(exact.provider().name(), "fallback");
            assert.equal(ai.message_text(exact.messages().items()[1]), "fallback follow up")
        },
        _ => assert.is_true(false),
    }
}
