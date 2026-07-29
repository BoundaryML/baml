function _append_test_task(provider: root.ai.Provider) -> root.ai.Task<string> throws never {
    root.ai.task<string>(provider, (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm.assemble_prompt_ast(["Return done.\n", ""], [ctx.output_format])
    })
}

function _append_test_provider(
    name: string = "append-test",
    calls: root.ai.tools.ToolCall[] = [],
) -> root.ai.testing.FakeToolProvider throws never {
    root.ai.testing.FakeToolProvider {
        provider_name: name,
        tool_calls: calls,
        final_output: `"done"`,
        provider_tools: [],
    }
}

test "conversation append preserves output type and resumes through Agent" {
    let provider = _append_test_provider();
    let task = _append_test_task(provider);
    let first = root.ai.internal._require_done(task.run(runner = root.ai.run.Agent<string>.new()));
    let fingerprint = first.conversation.output_type_fingerprint();
    let continued = first.conversation.append_message(root.ai.ChatMessage.user("one more question"));
    assert.equal(continued.output_type_fingerprint(), fingerprint);
    assert.equal(continued.provider().name(), "append-test");
    assert.equal(continued.messages().items().length(), 2);
    let second = root
        .ai
        .internal
        ._require_done(task.run(runner = root.ai.run.Agent<string>.new(conversation = continued)));
    assert.equal(second.value, "done");
    assert.equal(second.conversation.messages().items().length(), 2)
}

test "conversation append validates the whole batch before mutation" {
    let provider = _append_test_provider();
    let state = match (provider.begin<string>(_append_test_task(provider))) {
        let value: root.ai.testing.FakeToolConversation => value,
        _ => baml.sys.panic("expected fake tool conversation"),
    };
    let batch = root.ai.MessageHistory {
        entries: [root.ai.ChatMessage.user("valid first"), root.ai.ChatMessage.assistant("invalid second")],
    };
    let rejected = false;
    let _ = provider.append_messages(state, batch) catch (e) {
        let invalid: root.ai.InvalidRequest => {
            rejected = true;
            state
        },
        _ => baml.sys.panic("unexpected append validation error"),
    };
    assert.is_true(rejected);
    assert.equal(state.history.entries.length(), 1)
}

test "conversation append reports Unsupported when the owner lacks the capability" {
    let provider = root.ai.testing.fake_output_provider(`"done"`);
    let conversation = provider.begin<string>(_append_test_task(provider));
    let rejected = false;
    let _ = conversation
        .append_messages(
            root.ai.MessageHistory { entries: [root.ai.ChatMessage.user("follow up")] },
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
    let provider = _append_test_provider(calls = [root.ai.tools.ToolCall { id: "pending", name: "lookup", args: {} }]);
    let state = match (provider.begin<string>(_append_test_task(provider))) {
        let value: root.ai.testing.FakeToolConversation => value,
        _ => baml.sys.panic("expected fake tool conversation"),
    };
    let _ = provider.step<string>(state, []);
    let rejected = false;
    let _ = provider.append_message(state, root.ai.ChatMessage.user("too soon")) catch (e) {
        let invalid: root.ai.InvalidRequest => {
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
    let provider = root.ai.retry(inner, 2);
    let state = match (provider.begin<string>(_append_test_task(provider))) {
        let value: root.ai.RetryConversation => value,
        _ => baml.sys.panic("expected retry conversation"),
    };
    let fingerprint = state.output_type_fingerprint();
    let appended = provider.append_message(state, root.ai.ChatMessage.user("retry follow up"));
    match (appended) {
        let exact: root.ai.RetryConversation => {
            assert.equal(exact.output_type_fingerprint(), fingerprint);
            assert.equal(exact.provider().name(), "retry(retry-member)");
            assert.equal(root.ai.message_text(exact.messages().items()[1]), "retry follow up")
        },
        _ => assert.is_true(false),
    }
}

test "fallback pins the exact member after append" {
    let inner = _append_test_provider(name = "fallback-member");
    let provider = root.ai.fallback([inner]);
    let state = match (provider.begin<string>(_append_test_task(provider))) {
        let value: root.ai.FallbackConversation => value,
        _ => baml.sys.panic("expected fallback conversation"),
    };
    assert.equal(state.progressed, false);
    let appended = provider.append_message(state, root.ai.ChatMessage.user("fallback follow up"));
    match (appended) {
        let exact: root.ai.FallbackConversation => {
            assert.equal(exact.progressed, true);
            assert.equal(exact.member_index, 0);
            assert.equal(exact.provider().name(), "fallback");
            assert.equal(root.ai.message_text(exact.messages().items()[1]), "fallback follow up")
        },
        _ => assert.is_true(false),
    }
}
