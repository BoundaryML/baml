class Tool {
  name: string,
  description: string,
  parameters: type,
  // Runtime JSON Schema supplied by sources such as MCP. Null means derive the
  // schema from `parameters`.
  input_schema: json,
  handoff: bool?,
  _strict_validator: ((string) -> bool throws never)?,

  /// Runtime-type constructor for dynamic tool catalogs. Validation uses SAP
  /// recovery because only the runtime `type` is available.
  function from_type(name: string, description: string, parameters: type) -> Tool throws never {
    Tool {
      name: name,
      description: description,
      parameters: parameters,
      input_schema: null,
      handoff: null,
      _strict_validator: null,
    }
  }

  /// Preferred constructor when the argument type is lexical. Tool-call args
  /// are provider JSON protocol data, so validate them with strict JSON rather
  /// than SAP's intentionally forgiving LLM-text coercions.
  function typed<A>(name: string, description: string) -> Tool throws never {
    Tool {
      name: name,
      description: description,
      parameters: reflect.type_of<A>(),
      input_schema: null,
      handoff: null,
      _strict_validator: (text: string) -> bool throws never {
        let decoded: A = baml.json.from_string<A>(text) catch (e) {
          _ => return false,
        };
        true
      },
    }
  }

  function from_json_schema(
    name: string,
    description: string,
    input_schema: json,
  ) -> Tool throws never {
    Tool {
      name: name,
      description: description,
      parameters: reflect.type_of<json>(),
      input_schema: input_schema,
      handoff: null,
      _strict_validator: null,
    }
  }

  function validate_args(self, call: ToolCall) -> bool throws never {
    match (self._strict_validator) {
      let validate: (string) -> bool throws never => return validate(call.args),
      null => {},
    }
    if (self.input_schema != null) {
      let decoded = baml.json.parse(call.args) catch (e) { _ => { return false; } };
      return true;
    }
    baml.sap.parse_type(self.parameters, call.args) catch (e) {
      _ => return false,
    };
    true
  }

  function as_handoff(self) -> Tool throws never {
    Tool {
      name: self.name,
      description: self.description,
      parameters: self.parameters,
      input_schema: self.input_schema,
      handoff: true,
      _strict_validator: self._strict_validator,
    }
  }
}

class ToolCall {
  id: string,
  name: string,
  args: string,
}

class ToolResult {
  id: string,
  output: string,

  function ok(call: ToolCall, output: string) -> ToolResult throws never {
    ToolResult { id: call.id, output: output }
  }
  function error(call: ToolCall, message: string) -> ToolResult throws never {
    ToolResult { id: call.id, output: "error: " + message }
  }
}

class ToolCalls {
  calls: ToolCall[],
}

// Application-owned roster that may grow between model turns. Provider
// adapters remain responsible for translating each snapshot to their wire
// format through Tools.sync_tools.
class ToolRegistry {
  tools: Tool[],

  function new(tools: Tool[]) -> ToolRegistry throws never {
    ToolRegistry { tools: tools }
  }

  function add(self, tool: Tool) -> null throws never {
    for (let existing in self.tools) {
      if (existing.name == tool.name) { return null; }
    }
    self.tools.push(tool);
    null
  }

  function add_all(self, tools: Tool[]) -> null throws never {
    for (let tool in tools) { self.add(tool); }
    null
  }

  function snapshot(self) -> Tool[] throws never {
    self.tools.slice(0, self.tools.length())
  }
}

class ToolStepMeta {
  blocks: ModelBlock[],
  meta: Meta,
}
