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

class ToolRegistry {
  roster: Tool[],

  function new(tools: Tool[]) -> ToolRegistry throws never {
    ToolRegistry { roster: tools }
  }

  function add(self, tool: Tool) -> null throws never {
    for (let existing in self.roster) {
      if (existing.name == tool.name) { return null; }
    }
    self.roster.push(tool);
    null
  }

  function remove(self, name: string) -> bool throws never {
    let before = self.roster.length();
    self.roster = self.roster.filter((tool: Tool) -> bool { tool.name != name });
    self.roster.length() != before
  }

  function clear(self) -> null throws never {
    self.roster = [];
    null
  }

  function snapshot(self) -> Tool[] throws never {
    self.roster.slice(0, self.roster.length())
  }
}
