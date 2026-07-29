class OpenAIEmptyStrictFixture {
}

function _openai_state_machine_task(
    provider: root.ai.Provider,
) -> root.ai.Task<string> throws never {
    root.ai.task<string>(provider, (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm.assemble_prompt_ast(["Return ok.\n", ""], [ctx.output_format])
    })
}

test "OpenAI strict schema accepts an empty class" {
    let tool = _output_tool(reflect.type_of<OpenAIEmptyStrictFixture>(), true);
    let value_schema = baml.json.path<json>(tool, ".parameters.properties.value");
    let properties = baml.json.path<map<string, json>>(value_schema, ".properties");
    assert.equal(properties.keys().length(), 0);
    assert.equal(baml.json.path<bool>(value_schema, ".additionalProperties"), false)
}

test "OpenAI tool results are correlated and ordered by pending call ID" {
    let ordered = _correlate_tool_results(
        ["call-a", "call-b"],
        [
            root.ai.tools.ToolResult { id: "call-b", output: { "value": "b" }, is_error: false },
            root.ai.tools.ToolResult { id: "call-a", output: { "value": "a" }, is_error: false },
        ],
    );
    assert.equal(ordered[0].id, "call-a");
    assert.equal(ordered[1].id, "call-b")
}

test "OpenAI prompt mode requires pending results before another step" {
    let provider = root
        .openai
        .responses(
            api_key = "unused",
            output_mode = root.openai.OutputMode.Sap,
            tool_mode = root.openai.ToolMode.Prompt,
        );
    let state = match (provider.begin<string>(_openai_state_machine_task(provider))) {
        let value: PromptConversation => value,
        _ => baml.sys.panic("expected OpenAI prompt conversation"),
    };
    state.pending_call_ids = ["call-a"];
    let rejected = false;
    let _ = _prompt_step<string>(provider, state, []) catch (e) {
        let invalid: root.ai.InvalidRequest => {
            rejected = true;
            root.ai.ModelStep<string> {
                outcome: "unreachable",
                metadata: root.ai.ResponseMetadata {
                    provider: "test",
                    model: null,
                    request_id: null,
                    finish_reason: null,
                    usage: null,
                    attributes: {},
                    raw: null,
                },
            }
        },
        _ => baml.sys.panic("unexpected OpenAI prompt error"),
    };
    assert.is_true(rejected)
}

test "OpenAI prompt submit rejects duplicate result IDs" {
    let provider = root
        .openai
        .responses(
            api_key = "unused",
            output_mode = root.openai.OutputMode.Sap,
            tool_mode = root.openai.ToolMode.Prompt,
        );
    let state = match (provider.begin<string>(_openai_state_machine_task(provider))) {
        let value: PromptConversation => value,
        _ => baml.sys.panic("expected OpenAI prompt conversation"),
    };
    state.pending_call_ids = ["call-a", "call-b"];
    let rejected = false;
    let _ = _prompt_submit(
        provider,
        state,
        [
            root.ai.tools.ToolResult { id: "call-a", output: "a", is_error: false },
            root.ai.tools.ToolResult { id: "call-a", output: "duplicate", is_error: false },
        ],
    ) catch (e) {
        let invalid: root.ai.InvalidRequest => {
            rejected = true;
            state
        },
        _ => baml.sys.panic("unexpected OpenAI prompt submit error"),
    };
    assert.is_true(rejected)
}
