function _application_tool(
    tool: root.ai.tools.Tool,
    strict: bool,
) -> json throws baml.errors.Unsupported {
    {
        "name": tool.name,
        "description": tool.description,
        "input_schema": _tool_input_schema(tool.input_schema, strict),
        "strict": strict,
    }
}

function _application_tools(
    tools: root.ai.tools.Tool[],
    strict: bool,
) -> json[] throws baml.errors.Unsupported {
    tools.map((tool: root.ai.tools.Tool) -> json {
        _application_tool(tool, strict)
    })
}

function _application_calls(
    raw_calls: json[],
    raw_output: string,
) -> root.ai.tools.ToolCalls throws root.ai.ParseFailed {
    root.ai.tools.ToolCalls {
        calls: raw_calls.map((raw: json) -> root.ai.tools.ToolCall {
            let id = baml.json.path_or(raw, ".id", "");
            let name = baml.json.path_or(raw, ".name", "");
            let args = baml.json.path_or<json>(raw, ".input", null);
            if (id.length() == 0 || name.length() == 0) {
                throw root.ai.ParseFailed {
                    provider: "anthropic",
                    raw_output: raw_output,
                };
            }
            root.ai.tools.ToolCall {
                id: id,
                name: name,
                args: args,
            }
        }),
    }
}
