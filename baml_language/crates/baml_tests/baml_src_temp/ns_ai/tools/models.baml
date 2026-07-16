class Tool {
  name: string,
  description: string,
  parameters: type,
  input_schema: json,
  handoff: bool,

  function typed<A>(name: string, description: string) -> Tool throws never {
    Tool {
      name: name,
      description: description,
      parameters: reflect.type_of<A>(),
      input_schema: null,
      handoff: false,
    }
  }

  function from_json_schema(name: string, description: string, input_schema: json) -> Tool throws never {
    Tool {
      name: name,
      description: description,
      parameters: reflect.type_of<json>(),
      input_schema: input_schema,
      handoff: false,
    }
  }

  function as_handoff(self) -> Tool throws never {
    Tool {
      name: self.name,
      description: self.description,
      parameters: self.parameters,
      input_schema: self.input_schema,
      handoff: true,
    }
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
    for (let tool in tools) { registry.add(tool); }
    registry
  }

  function add(self, tool: Tool) -> null throws baml.errors.ToolError {
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
    for (let tool in tools) { combined.push(tool); }
    let replacement = ToolRegistry.new(combined);
    self.roster = replacement.roster;
    self.version = self.version + 1;
    null
  }

  function contains(self, name: string) -> bool throws never {
    self.roster.filter((tool: Tool) -> bool { tool.name == name }).length() == 1
  }

  function replace(self, tool: Tool) -> bool throws never {
    let replaced = false;
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
