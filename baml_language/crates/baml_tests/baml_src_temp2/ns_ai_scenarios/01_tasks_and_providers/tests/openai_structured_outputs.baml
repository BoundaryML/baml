// Verifies OpenAI's three bounded-output modes without making model calls:
// native function output by default, strict function output, and explicit SAP.

class OpenAIStructuredFixture {
    answer: string,
    note: string?,
}

class OpenAIRecursiveFixture {
    label: string,
    child: OpenAIRecursiveFixture?,
}

test "OpenAI defaults to a non-strict native output tool" {
    let provider = root.openai.responses();
    assert.equal(provider.output_mode, root.openai.OutputMode.Native);
    assert.equal(provider.tool_mode, root.openai.ToolMode.Native);

    let tool = root.openai.internal._output_tool(reflect.type_of<OpenAIStructuredFixture>(), false);
    assert.equal(baml.json.path<string>(tool, ".name"), "__baml_return_output");
    assert.equal(baml.json.path<bool>(tool, ".strict"), false);
    assert.equal(baml.json.path<string>(tool, ".parameters.properties.value.type"), "object")
}

test "OpenAI strict mode closes objects and requires nullable fields" {
    let tool = root.openai.internal._output_tool(reflect.type_of<OpenAIStructuredFixture>(), true);
    let required = baml.json.path<string[]>(tool, ".parameters.properties.value.required");
    assert.equal(baml.json.path<bool>(tool, ".strict"), true);
    assert
        .equal(
            baml.json.path<bool>(tool, ".parameters.properties.value.additionalProperties"),
            false,
        );
    assert.is_true(required.includes("answer"));
    assert.is_true(required.includes("note"))
}

test "OpenAI wraps primitive return types in the result tool" {
    let tool = root.openai.internal._output_tool(reflect.type_of<string>(), true);
    assert.equal(baml.json.path<string>(tool, ".parameters.properties.value.type"), "string");
    assert.equal(baml.json.path<string[]>(tool, ".parameters.required"), ["value"])
}

test "OpenAI array returns stay inside one result tool" {
    let tool = root.openai.internal._output_tool(reflect.type_of<OpenAIStructuredFixture[]>(), true);
    assert.equal(baml.json.path<string>(tool, ".parameters.properties.value.type"), "array")
}

test "OpenAI array results still exact-decode one call" {
    let call: json = {
        "type": "function_call",
        "name": "__baml_return_output",
        "arguments": `{"value":[{"answer":"one","note":null},{"answer":"two","note":"ok"}]}`,
    };
    let value = root
        .openai
        .internal
        ._parse_output_call<OpenAIStructuredFixture[]>(call, baml.json.stringify(call));
    assert.equal(value.length(), 2);
    assert.equal(value[1].answer, "two")
}

test "OpenAI lifts recursive definitions to the tool parameter root" {
    let tool = root.openai.internal._output_tool(reflect.type_of<OpenAIRecursiveFixture>(), true);
    let parameters = baml.json.path<map<string, json>>(tool, ".parameters");
    let value_schema = baml.json.path<map<string, json>>(tool, ".parameters.properties.value");
    assert.is_true(parameters.has("$defs"));
    assert.equal(value_schema.has("$defs"), false)
}

test "OpenAI result tools exact-decode the wrapped value" {
    let call: json = {
        "type": "function_call",
        "name": "__baml_return_output",
        "arguments": `{"value":{"answer":"done","note":null}}`,
    };
    let value = root
        .openai
        .internal
        ._parse_output_call<OpenAIStructuredFixture>(call, baml.json.stringify(call));
    assert.equal(value.answer, "done");
    assert.equal(value.note, null)
}

test "OpenAI strict mode rejects map-like output schemas" {
    let output: map<string, json>? = root.openai.internal._output_tool(reflect.type_of<map<string, string>>(), true) catch (e) {
        baml.errors.Unsupported => null
    };
    assert.equal(output, null)
}

test "OpenAI prompt tools require SAP output" {
    let rejected = false;
    let provider = root.openai.responses(tool_mode = root.openai.ToolMode.Prompt);
    root.openai.internal._validate_output_config(provider) catch (e) {
        root.ai.InvalidRequest => {
            rejected = true;
            null
        },
    };
    assert.is_true(rejected)
}

test "OpenAI prompt tool mode selects the text adapter internally" {
    let provider = root
        .openai
        .responses(
            output_mode = root.openai.OutputMode.Sap,
            tool_mode = root.openai.ToolMode.Prompt,
        );
    let conversation = provider.begin<Resolution>(ResolveTicket_task(sample_ticket(), provider));
    match (conversation) {
        let value: root.openai.internal.PromptConversation => {
            assert.equal(value.owner.name(), "openai")
        },
        _ => assert.is_true(false),
    }
}

test "OpenAI agent mode adds the result tool unless SAP is explicit" {
    let native = root.openai.internal._agent_tools(reflect.type_of<OpenAIStructuredFixture>(), [], false, false);
    let sap = root.openai.internal._agent_tools(reflect.type_of<OpenAIStructuredFixture>(), [], false, true);
    assert.equal(native.length(), 1);
    assert.equal(baml.json.path<string>(native[0], ".name"), "__baml_return_output");
    assert.equal(sap.length(), 0)
}

test "OpenAI parallel calls depend on app tools and strictness, not output shape" {
    let non_strict = root.openai.responses(parallel_tool_calls = true);
    let strict = root.openai.responses(output_mode = root.openai.OutputMode.Strict, parallel_tool_calls = true);
    assert.equal(root.openai.internal._allow_parallel_tool_calls(non_strict, 1), false);
    assert.equal(root.openai.internal._allow_parallel_tool_calls(non_strict, 2), true);
    assert.equal(root.openai.internal._allow_parallel_tool_calls(strict, 2), false)
}

test "OpenAI agent requests preserve extras but own model and transcript" {
    let provider = root
        .openai
        .responses(
            model = "expected-model",
            api_key = "secret",
            extra_headers = { "X-Test": "yes" },
            extra_body = { "model": "must-not-win", "temperature": 0.25 },
        );
    let input: json[] = [{ "role": "user", "content": "hello" }];
    let body = root.openai.internal._request_body(provider, input);
    let headers = root.openai.internal._request_headers(provider);
    assert.equal(baml.json.path<string>(body, ".model"), "expected-model");
    assert.equal(baml.json.path<float>(body, ".temperature"), 0.25);
    assert.equal(baml.json.path<json[]>(body, ".input"), input);
    assert.equal(headers.get("Authorization"), "Bearer secret");
    assert.equal(headers.get("X-Test"), "yes")
}

test "OpenAI bounded native request forces one Responses result tool" {
    let provider = root
        .openai
        .responses(
            model = "request-model",
            api_key = "unused",
            base_url = "https://example.com/v1",
            output_mode = root.openai.OutputMode.Strict,
            parallel_tool_calls = true,
        );
    let request = root
        .openai
        .internal
        ._responses_built_request(provider, ResolveTicket_task(sample_ticket(), provider), false);
    let body = baml.json.parse(request.body);
    assert.equal(request.url, "https://example.com/v1/responses");
    assert.equal(baml.json.path<string>(body, ".tool_choice.name"), "__baml_return_output");
    assert.equal(baml.json.path<bool>(body, ".parallel_tool_calls"), false);
    assert.equal(baml.json.path<string>(body, ".tools[0].type"), "function");
    assert.equal(baml.json.path<string>(body, ".tools[0].name"), "__baml_return_output")
}

test "OpenAI SAP request omits the synthetic result protocol" {
    let provider = root
        .openai
        .responses(
            model = "request-model",
            api_key = "unused",
            base_url = "https://example.com/v1",
            output_mode = root.openai.OutputMode.Sap,
        );
    let request = root
        .openai
        .internal
        ._responses_built_request(provider, ResolveTicket_task(sample_ticket(), provider), false);
    let body = baml.json.parse(request.body);
    assert.equal(baml.json.path_or<json?>(body, ".tools", null), null);
    assert.equal(baml.json.path_or<json?>(body, ".tool_choice", null), null);
    assert.equal(baml.json.path_or<json?>(body, ".parallel_tool_calls", null), null)
}

test "OpenAI SAP reads Responses output_text" {
    let raw: json = {
        "id": "resp-sap",
        "status": "completed",
        "model": "text-model",
        "output": [
            {
                "type": "message",
                "content": [{ "type": "output_text", "text": `{"answer":"from text","note":null}` }],
            },
        ],
    };
    let value = root
        .openai
        .internal
        ._parse_response_value<OpenAIStructuredFixture>(raw, baml.json.stringify(raw), true);
    assert.equal(value.answer, "from text")
}

function _openai_array_task(
    provider: root.ai.Provider,
) -> root.ai.Task<OpenAIStructuredFixture[]> throws never {
    root.ai.task_named<OpenAIStructuredFixture[]>(provider, "OpenAIStructuredArray", {}, [], (
        ctx: baml.llm.Context,
    ) -> baml.llm.PromptAst {
        baml.llm
            .assemble_prompt_ast(
                ["", "\n", ""],
                [
                    "Return exactly two items. Use answer `one` and `two`; set note to null.",
                    ctx.output_format,
                ],
            )
    })
}

testset "integ-test-openai-responses-configurations" {
    test "OpenAI Responses live default uses one non-strict result call" {
        let provider = root
            .openai
            .responses(model = "gpt-5.6-luna", api_key = baml.env.get_or_panic("OPENAI_API_KEY"));
        let response = root
            .ai
            .internal
            ._response_from_done(
                root.ai
                    .internal
                    ._require_done(
                        ResolveTicket_task(sample_ticket(), provider)
                            .run(runner = root.ai.run.Agent<Resolution>.new()),
                    ),
            );
        let calls = root.openai.internal._response_function_calls(response.metadata.raw);
        assert.equal(calls.length(), 1);
        assert.equal(baml.json.path<string>(calls[0], ".name"), "__baml_return_output");
        assert.is_true(response.value.reply.length() > 0)
    }

    test "OpenAI Responses live strict accepts the lowered class schema" {
        let provider = root
            .openai
            .responses(
                model = "gpt-5.6-luna",
                api_key = baml.env.get_or_panic("OPENAI_API_KEY"),
                output_mode = root.openai.OutputMode.Strict,
            );
        let response = root
            .ai
            .internal
            ._response_from_done(
                root.ai
                    .internal
                    ._require_done(
                        ResolveTicket_task(sample_ticket(), provider)
                            .run(runner = root.ai.run.Agent<Resolution>.new()),
                    ),
            );
        let calls = root.openai.internal._response_function_calls(response.metadata.raw);
        assert.equal(calls.length(), 1);
        assert.equal(baml.json.path<string>(calls[0], ".name"), "__baml_return_output");
        assert.is_true(response.value.summary.length() > 0)
    }

    test "OpenAI Responses live strict array returns one array-valued call" {
        let provider = root
            .openai
            .responses(
                model = "gpt-5.6-luna",
                api_key = baml.env.get_or_panic("OPENAI_API_KEY"),
                output_mode = root.openai.OutputMode.Strict,
                parallel_tool_calls = true,
            );
        let response = root
            .ai
            .internal
            ._response_from_done(
                root.ai
                    .internal
                    ._require_done(
                        _openai_array_task(provider)
                            .run(runner = root.ai.run.Agent<OpenAIStructuredFixture[]>.new()),
                    ),
            );
        let calls = root.openai.internal._response_function_calls(response.metadata.raw);
        assert.equal(calls.length(), 1);
        assert.equal(response.value.length(), 2);
        assert.equal(response.value[0].answer, "one");
        assert.equal(response.value[1].answer, "two")
    }

    test "OpenAI Responses live SAP parses output_text explicitly" {
        let provider = root
            .openai
            .responses(
                model = "gpt-5.6-luna",
                api_key = baml.env.get_or_panic("OPENAI_API_KEY"),
                output_mode = root.openai.OutputMode.Sap,
            );
        let response = root
            .ai
            .internal
            ._response_from_done(
                root.ai
                    .internal
                    ._require_done(
                        ResolveTicket_task(sample_ticket(), provider)
                            .run(runner = root.ai.run.Agent<Resolution>.new()),
                    ),
            );
        assert.is_true(root.openai.internal._response_output_text(response.metadata.raw) != null);
        assert.is_true(response.value.reply.length() > 0)
    }

    test "OpenAI Responses live strict agent runs an application tool then returns T" {
        let provider = root
            .openai
            .responses(
                model = "gpt-5.6-luna",
                api_key = baml.env.get_or_panic("OPENAI_API_KEY"),
                output_mode = root.openai.OutputMode.Strict,
                parallel_tool_calls = true,
            );
        let task = ToolInstructions_task(
            provider,
            [search_knowledge],
            `Call search_knowledge with query "duplicate charge". After receiving its result, return the final resolution.`,
        );
        let outcome = task.run(runner = root.ai.run.Agent<Resolution>.new(tools = [search_knowledge]));
        match (outcome) {
            let done: root.ai.Done<Resolution> => {
                assert.is_true(done.value.reply.length() > 0)
            },
            _ => assert.is_true(false),
        }
    }
}
