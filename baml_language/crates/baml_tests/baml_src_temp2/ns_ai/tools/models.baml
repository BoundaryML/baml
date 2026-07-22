class Tool {
  name: string,
  description: string,
  input_schema: json,
  handler: baml.AnyFunction<Returns = unknown, Throws = baml.errors.ToolError>,
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

function coarse_schema(type_name: string) -> json throws never {
  let schema: json = { "type": type_name };
  schema
}

function schema_for_function(
  handler: baml.AnyFunction,
) -> json throws never {
  let signature = reflect.signature(handler);
  let properties: map<string, json> = {};
  let required: string[] = [];

  //# Derive required positional parameters
  for (let arg in signature.args) {
    let schema = baml.schema.json_schema(arg.type) catch (e) {
      _ => coarse_schema(arg.type.to_string()),
    };
    let _ = properties.set(arg.name, schema);
    required.push(arg.name);
  }

  //# Derive defaulted/named parameters without marking them required
  for (let name in signature.opts.keys()) {
    match (signature.opts.get(name)) {
      let arg: reflect.Arg => {
        let schema = baml.schema.json_schema(arg.type) catch (e) {
          _ => coarse_schema(arg.type.to_string()),
        };
        let _ = properties.set(name, schema);
      },
      null => {},
    }
  }

  {
    "type": "object",
    "properties": properties,
    "required": baml.json.to_json(required) catch (e) { _ => null },
    "additionalProperties": false,
  }
}

// Function-backed application tool. AnyFunction preserves the callable's
// signature for schema derivation and reflect.call_any performs checked named
// argument dispatch, including ordinary BAML default arguments.
function tool(
  name: string,
  description: string,
  handler: baml.AnyFunction<Returns = unknown, Throws = baml.errors.ToolError>,
) -> Tool throws never {
  let signature = reflect.signature(handler);
  Tool {
    name: name,
    description: if (description.length() > 0) {
      description
    } else {
      signature.docstring ?? ""
    },
    input_schema: schema_for_function(handler),
    handler: handler,
    handoff: false,
  }
}

// Escape hatch for MCP and other dynamically described tools. Runtime
// argument validation is still performed by the supplied handler.
function tool_from_json_schema(
  name: string,
  description: string,
  input_schema: json,
  handler: baml.AnyFunction<Returns = unknown, Throws = baml.errors.ToolError>,
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
    _ => return ToolResult.error(call, "tool arguments must be an object"),
  };

  //# Invoke the original BAML function with signature-aware named arguments
  let output = reflect.call_any(tool.handler, args) catch (e) {
    let invalid: reflect.InvalidArgumentError => return ToolResult.error(
      call,
      `invalid arguments: expected ${invalid.expected}, received ${invalid.got}`,
    ),
    let failure: baml.errors.ToolError => return ToolResult.error(
      call,
      `tool failed: ${failure}`,
    ),
  };

  //# Normalize arbitrary typed function results at the provider boundary
  let encoded = baml.json.to_json(output) catch (e) {
    _ => return ToolResult.error(call, "tool result could not be serialized"),
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

implements baml.errors.Failure for ToolRegistryError {
  function is_retryable(self) -> bool throws never { false }
  function is_effectful(self) -> bool throws never { false }
  function is_policy_refusal(self) -> bool throws never { false }
  function is_resumable(self) -> bool throws never { false }
  function is_unsupported(self) -> bool throws never { false }
}

implements baml.errors.ToolError for ToolRegistryError {
  function is_network_error(self) -> bool throws never { false }
}

class ToolRegistry {
  roster: Tool[],
  version: int,

  function new(tools: Tool[]) -> ToolRegistry throws baml.errors.ToolError {
    let registry = ToolRegistry { roster: [], version: 0 };
    //# Seed the registry through validated insertion
    for (let tool in tools) { registry.add(tool); }
    registry
  }

  function add(self, tool: Tool) -> null throws baml.errors.ToolError {
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

  function add_all(self, tools: Tool[]) -> null throws baml.errors.ToolError {
    let combined = self.snapshot();
    //# Stage the combined tool roster
    for (let tool in tools) { combined.push(tool); }
    //# Validate before replacing live registry state
    let replacement = ToolRegistry.new(combined);
    self.roster = replacement.roster;
    self.version = self.version + 1;
    null
  }

  function contains(self, name: string) -> bool throws never {
    self.roster.filter((tool: Tool) -> bool { tool.name == name }).length() == 1
  }

  function get(self, name: string) -> Tool? throws never {
    self.roster.find((tool: Tool) -> bool { tool.name == name })
  }

  function replace(self, tool: Tool) -> bool throws never {
    let replaced = false;
    //# Replace the matching tool without reordering
    self.roster = self.roster.map((existing: Tool) -> Tool {
      if (existing.name == tool.name) {
        replaced = true;
        tool
      } else {
        existing
      }
    });
    if (replaced) { self.version = self.version + 1; }
    replaced
  }

  function replace_all(self, tools: Tool[]) -> null throws baml.errors.ToolError {
    //# Validate the complete replacement roster
    let replacement = ToolRegistry.new(tools);
    self.roster = replacement.roster;
    self.version = self.version + 1;
    null
  }

  function remove(self, name: string) -> bool throws never {
    let before = self.roster.length();
    self.roster = self.roster.filter((tool: Tool) -> bool { tool.name != name });
    let removed = self.roster.length() != before;
    if (removed) { self.version = self.version + 1; }
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
