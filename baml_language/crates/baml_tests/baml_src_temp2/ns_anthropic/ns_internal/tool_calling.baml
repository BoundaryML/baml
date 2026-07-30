function _application_tool(
    tool: ai.tools.Tool,
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
    tools: ai.tools.Tool[],
    strict: bool,
) -> json[] throws baml.errors.Unsupported {
    tools.map((tool: ai.tools.Tool) -> json {
        _application_tool(tool, strict)
    })
}

function _application_calls(
    raw_calls: json[],
    raw_output: string,
) -> ai.tools.ToolCalls throws ai.ParseFailed {
    ai.tools.ToolCalls {
        calls: raw_calls.map((raw: json) -> ai.tools.ToolCall {
            let id = baml.json.path_or(raw, ".id", "");
            let name = baml.json.path_or(raw, ".name", "");
            let args = baml.json.path_or<json>(raw, ".input", null);
            if (id.length() == 0 || name.length() == 0) {
                throw ai.ParseFailed {
                    provider: "anthropic",
                    raw_output: raw_output,
                };
            }
            ai.tools.ToolCall { id: id, name: name, args: args }
        }),
    }
}

function _tool_use_id(raw: json) -> string? throws never {
    match (raw) {
        let object: map<string, json> => {
            match (object.get("id")) {
                let id: string => id,
                _ => null,
            }
        },
        _ => null,
    }
}

function _is_tool_use(raw: json) -> bool throws never {
    match (raw) {
        let object: map<string, json> => {
            match (object.get("type")) {
                "tool_use" => true,
                _ => false,
            }
        },
        _ => false,
    }
}
