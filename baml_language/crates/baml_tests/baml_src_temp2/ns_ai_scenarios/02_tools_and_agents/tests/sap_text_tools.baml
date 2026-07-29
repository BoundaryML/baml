test "guide 02.11: full SAP chooses between T and ToolCalls as text" {
    let provider = root
        .openai
        .responses(
            api_key = "unused",
            output_mode = root.openai.OutputMode.Sap,
            tool_mode = root.openai.ToolMode.Prompt,
        );
    let task = _sap_resolution_task(provider, [search_knowledge]);
    let conversation = provider.begin<Resolution>(task);
    let state = match (conversation) {
        let value: root.openai.internal.PromptConversation => value,
        _ => baml.sys.panic("expected OpenAI prompt-tool conversation"),
    };
    let tools = root.ai.tools.normalize_tools([search_knowledge]);
    let rendered = root
        .openai
        .internal
        ._prompt_turn_task<Resolution | root.ai.tools.ToolCalls>(
            provider,
            state,
            root.openai.internal._prompt_tool_output_suffix(tools),
        )
        .prompt
        .text();
    assert.contains(rendered, "Available tools:");
    assert.contains(rendered, "search_knowledge");
    assert.contains(rendered, "calls")
}

test "guide 02.11: a union T remains distinct from the ToolCalls branch" {
    let provider = root
        .openai
        .responses(
            api_key = "unused",
            output_mode = root.openai.OutputMode.Sap,
            tool_mode = root.openai.ToolMode.Prompt,
        );
    let task = _sap_union_task(provider, [search_knowledge]);
    let conversation = provider.begin<SapSupportDecision>(task);
    let state = match (conversation) {
        let value: root.openai.internal.PromptConversation => value,
        _ => baml.sys.panic("expected OpenAI prompt-tool conversation"),
    };
    let tools = root.ai.tools.normalize_tools([search_knowledge]);
    let rendered = root
        .openai
        .internal
        ._prompt_turn_task<SapSupportDecision | root.ai.tools.ToolCalls>(
            provider,
            state,
            root.openai.internal._prompt_tool_output_suffix(tools),
        )
        .prompt
        .text();
    assert.contains(rendered, "answer");
    assert.contains(rendered, "escalation");
    assert.contains(rendered, "calls")
}

testset "integ-test-guide-02-sap-text-tools" {
    test "guide 02.11 live OpenAI: no tool API uses text for ToolCalls or union T" {
        let provider = root
            .openai
            .responses(
                model = "gpt-5.6-luna",
                api_key = baml.env.get_or_panic("OPENAI_API_KEY"),
                output_mode = root.openai.OutputMode.Sap,
                tool_mode = root.openai.ToolMode.Prompt,
            );
        // OpenAI selects its prompt-tool adapter internally. Every turn is
        // output_text parsed as T | ToolCalls.
        let task = _sap_union_task(provider, [search_knowledge]);
        let outcome = task.run(runner = root.ai.run.Agent<SapSupportDecision>.new(tools = [search_knowledge]));
        match (outcome) {
            let done: root.ai.Done<SapSupportDecision> => {
                match (done.value) {
                    let answer: SapAnswer => assert.is_true(answer.message.length() > 0),
                    let escalation: SapEscalation => assert.is_true(false),
                }
            },
            _ => assert.is_true(false),
        }
    }

    test "guide 02.11 live OpenAI: provider SAP is hybrid by default" {
        let provider = root
            .openai
            .responses(
                model = "gpt-5.6-luna",
                api_key = baml.env.get_or_panic("OPENAI_API_KEY"),
                output_mode = root.openai.OutputMode.Sap,
            );
        let task = _sap_union_task(provider, [search_knowledge]);
        let outcome = task.run(runner = root.ai.run.Agent<SapSupportDecision>.new(tools = [search_knowledge]));
        match (outcome) {
            let done: root.ai.Done<SapSupportDecision> => {
                let state = match (done.conversation) {
                    let value: root.openai.internal.Conversation => value,
                    _ => baml.sys.panic("expected OpenAI Responses conversation"),
                };
                assert.equal(state.active_tools.length(), 1);
                assert
                    .equal(
                        baml.json.path<string>(state.active_tools[0], ".name"),
                        "search_knowledge",
                    );
                match (done.value) {
                    let answer: SapAnswer => assert.is_true(answer.message.length() > 0),
                    let escalation: SapEscalation => assert.is_true(false),
                }
            },
            _ => assert.is_true(false),
        }
    }
}
