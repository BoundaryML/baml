class Tool {
    name: string,
    description: string,
    input_schema: json,
    handler: baml.AnyFunction<Returns = unknown, Throws = ai.Failure>,
    handoff: bool,

    function as_handoff(self) -> Tool throws never {
        Tool {
            name: self.name,
            description: self.description,
            input_schema: self.input_schema,
            handler: self.handler,
            handoff: true,
        }
    }
}

// Public tool inputs accept ordinary BAML functions. Tool is the normalized
// representation used after configuration crosses into the runner/provider
// machinery.
type ToolInput = Tool | baml.AnyFunction<Returns = unknown, Throws = ai.Failure>;

function _coarse_schema(type_name: string) -> json throws never {
    let schema: json = { "type": type_name };
    schema
}

function _schema_for_function(handler: baml.AnyFunction) -> json throws never {
    let signature = reflect.signature(handler);
    let properties: map<string, json> = {};
    let required: string[] = [];
    //# Derive required positional parameters
    for (let arg in signature.args) {
        let schema = baml.schema.json_schema(arg.type) catch (e) {
            _ => _coarse_schema(arg.type.to_string()),
        };
        let _ = properties.set(arg.name, schema);
        required.push(arg.name);
    }
    //# Derive defaulted/named parameters without marking them required
    for (let name in signature.opts.keys()) {
        match (signature.opts.get(name)) {
            let arg: reflect.Arg => {
                let schema = baml.schema.json_schema(arg.type) catch (e) {
                    _ => _coarse_schema(arg.type.to_string()),
                };
                let _ = properties.set(name, schema);
            },
            null => {},
        }
    }

    {
        "type": "object",
        "properties": properties,
        "required": baml.json.to_json(required) catch (e) {
            _ => null
        },
        "additionalProperties": false,
    }
}

// Function-backed application tool. AnyFunction preserves the callable's
// declared name, docstring, and signature. Explicit metadata is only needed
// for anonymous callables or when the external tool contract intentionally
// differs from the underlying function declaration. reflect.call_any performs
// checked named argument dispatch, including ordinary BAML default arguments.
function tool(
    handler: baml.AnyFunction<Returns = unknown, Throws = ai.Failure>,
    name: string? = null,
    description: string? = null,
) -> Tool throws never {
    let signature = reflect.signature(handler);
    Tool {
        name: name ?? signature.name ?? "",
        description: description ?? signature.docstring ?? "",
        input_schema: _schema_for_function(handler),
        handler: handler,
        handoff: false,
    }
}

function normalize_tool(input: ToolInput) -> Tool throws never {
    match (input) {
        let existing: Tool => existing,
        let handler: baml.AnyFunction<Returns = unknown, Throws = ai.Failure> => {
            tool(handler)
        },
    }
}

function normalize_tools(inputs: ToolInput[]) -> Tool[] throws never {
    inputs.map((input: ToolInput) -> Tool {
        normalize_tool(input)
    })
}

// Arrays are invariant, so normalized internals widen element-by-element when
// they need to re-enter a public ToolInput boundary.
function _tool_inputs_from_tools(tools: Tool[]) -> ToolInput[] throws never {
    tools.map((tool: Tool) -> ToolInput {
        tool
    })
}

// Escape hatch for MCP and other dynamically described tools. Runtime
// argument validation is still performed by the supplied handler.
function tool_from_json_schema(
    name: string,
    description: string,
    input_schema: json,
    handler: baml.AnyFunction<Returns = unknown, Throws = ai.Failure>,
) -> Tool throws never {
    Tool {
        name: name,
        description: description,
        input_schema: input_schema,
        handler: handler,
        handoff: false,
    }
}

// A provider-owned tool is configured and executed by the provider adapter.
// It is deliberately not an application Tool and has no application handler.
class ProviderTool {
    name: string,
    kind: string,
    config: json,
}

class ToolCall {
    id: string,
    name: string,
    args: json,
}

class ToolResult {
    id: string,
    output: json,
    is_error: bool,

    function ok(call: ToolCall, output: json) -> ToolResult throws never {
        ToolResult { id: call.id, output: output, is_error: false }
    }

    function error(call: ToolCall, message: string) -> ToolResult throws never {
        ToolResult { id: call.id, output: { "error": message }, is_error: true }
    }
}

function invoke_tool(tool: Tool, call: ToolCall) -> ToolResult throws never {
    //# Require a JSON object so named arguments can be checked by call_any
    let args = baml.json.from_json<map<string, unknown>>(call.args) catch (e) {
        _ => {
            return ToolResult.error(call, "tool arguments must be an object");
        },
    };
    //# Invoke the original BAML function with signature-aware named arguments
    let output = reflect.call_any(tool.handler, args) catch (e) {
        let invalid: reflect.InvalidArgumentError => {
            return ToolResult
                .error(
                    call,
                    `invalid arguments: expected ${invalid.expected}, received ${invalid.got}`,
                );
        },
        let failure: ai.Failure => {
            return ToolResult.error(call, `tool failed: ${failure}`);
        },
    };
    //# Normalize arbitrary typed function results at the provider boundary
    let encoded = baml.json.to_json(output) catch (e) {
        _ => {
            return ToolResult.error(call, "tool result could not be serialized");
        },
    };
    ToolResult.ok(call, encoded)
}

class ToolCalls {
    calls: ToolCall[],
}

class ToolRegistryError {
    name: string,
    message: string,
}

implements ai.Failure for ToolRegistryError {
  function is_transient(self) -> bool throws never { false }
  function effects(self) -> ai.Effects throws never { ai.Effects.None }
}

class ToolRegistry {
    roster: Tool[],
    version: int,

    function new(tools: ToolInput[]) -> ToolRegistry throws ai.Failure {
        let registry = ToolRegistry { roster: [], version: 0 };
        //# Seed the registry through validated insertion
        for (let input in tools) {
            registry.add(input);
        }
        registry
    }

    function add(self, input: ToolInput) -> null throws ai.Failure {
        let tool = normalize_tool(input);
        //# Reject duplicate tool names
        for (let existing in self.roster) {
            if (existing.name == tool.name) {
                throw ToolRegistryError {
          name: tool.name,
          message: "tool name is already registered: " + tool.name,
        };
            }
        }
        self.roster.push(tool);
        self.version = self.version + 1;
        null
    }

    function add_all(self, tools: ToolInput[]) -> null throws ai.Failure {
        let combined: ToolInput[] = [];
        for (let existing in self.roster) {
            combined.push(existing);
        }
        //# Stage the combined tool roster
        for (let input in tools) {
            combined.push(input);
        }
        //# Validate before replacing live registry state
        let replacement = ToolRegistry.new(combined);
        self.roster = replacement.roster;
        self.version = self.version + 1;
        null
    }

    function contains(self, name: string) -> bool throws never {
        self.roster
            .filter((tool: Tool) -> bool {
                tool.name == name
            })
            .length()
            == 1
    }

    function get(self, name: string) -> Tool? throws never {
        self.roster.find((tool: Tool) -> bool {
            tool.name == name
        })
    }

    function replace(self, input: ToolInput) -> bool throws never {
        let tool = normalize_tool(input);
        let replaced = false;
        //# Replace the matching tool without reordering
        self.roster
            = self.roster.map((existing: Tool) -> Tool {
                if (existing.name == tool.name) {
                    replaced = true;
                    tool
                } else {
                    existing
                }
            });
        if (replaced) {
            self.version = self.version + 1;
        }
        replaced
    }

    function replace_all(self, tools: ToolInput[]) -> null throws ai.Failure {
        //# Validate the complete replacement roster
        let replacement = ToolRegistry.new(tools);
        self.roster = replacement.roster;
        self.version = self.version + 1;
        null
    }

    function remove(self, name: string) -> bool throws never {
        let before = self.roster.length();
        self.roster
            = self.roster.filter((tool: Tool) -> bool {
                tool.name != name
            });
        let removed = self.roster.length() != before;
        if (removed) {
            self.version = self.version + 1;
        }
        removed
    }

    function clear(self) -> null throws never {
        if (self.roster.length() > 0) {
            self.roster = [];
            self.version = self.version + 1;
        }
        null
    }

    function snapshot(self) -> Tool[] throws never {
        self.roster.slice(0, self.roster.length())
    }
}
