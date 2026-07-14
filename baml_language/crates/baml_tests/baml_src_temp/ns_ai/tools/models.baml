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

  function snapshot(self) -> Tool[] throws never {
    self.roster.slice(0, self.roster.length())
  }
}

