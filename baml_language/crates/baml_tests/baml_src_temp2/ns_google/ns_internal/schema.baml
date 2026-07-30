// Gemini-native schemas and function declarations. Provider-neutral JSON
// Schema is produced by baml.schema; this file only adds Gemini's wire wrapper.

function _output_tool_name() -> string throws never {
    "__baml_return_output"
}

function _output_value_schema(output_type: type) -> json throws baml.errors.Unsupported {
    baml.schema.json_schema(output_type)
}

// Gemini function declarations require an object at the parameter root. The
// value wrapper keeps primitive, array, union, and nullable task outputs legal.
// Hoist $defs because JSON Schema references remain rooted at the complete
// parameters document.
function _output_parameters(output_type: type) -> map<string, json> throws baml.errors.Unsupported {
    let value_schema = _output_value_schema(output_type);
    let schema_object = match (value_schema) {
        let object: map<string, json> => object,
        _ => {
            throw baml.errors.Unsupported {
            message: "Gemini output schema must be a JSON Schema object",
        }
        },
    };
    let has_definitions = schema_object.has("$defs");
    let definitions = schema_object.get("$defs");
    if (has_definitions) {
        schema_object.delete("$defs");
    }
    let parameters: map<string, json> = {
        "type": "object",
        "properties": { "value": schema_object },
        "required": ["value"],
        "additionalProperties": false,
    };
    if (has_definitions) {
        parameters.set("$defs", definitions);
    }
    parameters
}

function _output_declaration(output_type: type) -> json throws baml.errors.Unsupported {
    {
        "name": _output_tool_name(),
        "description": "Return the final typed result. Call this only when the task is complete.",
        "parametersJsonSchema": _output_parameters(output_type),
    }
}

function _application_declaration(tool: ai.tools.Tool) -> json throws never {
    {
        "name": tool.name,
        "description": tool.description,
        "parametersJsonSchema": tool.input_schema,
    }
}

function _validate_application_tool_names(
    tools: ai.tools.Tool[],
    provider_name: string,
) -> null throws ai.InvalidRequest {
    if (
        tools.some((tool: ai.tools.Tool) -> bool {
            tool.name == _output_tool_name()
        })
    ) {
        throw ai.InvalidRequest {
            provider: provider_name,
            status_code: null,
            detail: "Gemini reserves the tool name " + _output_tool_name(),
        };
    }
    null
}

function _agent_declarations(
    output_type: type,
    tools: ai.tools.Tool[],
    use_sap: bool,
    provider_name: string,
) -> json[] throws baml.errors.Unsupported | ai.InvalidRequest {
    // Reserve the result name in every output mode. This keeps an application
    // tool from changing meaning when a provider switches between Native/SAP.
    _validate_application_tool_names(tools, provider_name);
    let declarations = tools.map((tool: ai.tools.Tool) -> json {
        _application_declaration(tool)
    });
    if (!use_sap) {
        declarations.push(_output_declaration(output_type));
    }
    declarations
}
