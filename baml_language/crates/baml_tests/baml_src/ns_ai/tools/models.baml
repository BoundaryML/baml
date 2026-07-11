class Tool {
  name: string,
  description: string,
  parameters: type,
  handoff: bool?,
  _strict_validator: ((string) -> bool throws never)?,

  /// Runtime-type constructor for dynamic tool catalogs. Validation uses SAP
  /// recovery because only the runtime `type` is available.
  function from_type(name: string, description: string, parameters: type) -> Tool throws never {
    Tool {
      name: name,
      description: description,
      parameters: parameters,
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
      handoff: null,
      _strict_validator: (text: string) -> bool throws never {
        let decoded: A = baml.json.from_string<A>(text) catch (e) {
          _ => return false,
        };
        true
      },
    }
  }

  function validate_args(self, call: ToolCall) -> bool throws never {
    match (self._strict_validator) {
      let validate: (string) -> bool => return validate(call.args),
      null => {},
    }
    baml.sap.parse_type(self.parameters, call.args) catch (e) {
      _ => return false,
    };
    true
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
}

class ToolCalls {
  calls: ToolCall[],
}

class ToolStepMeta {
  blocks: ModelBlock[],
  meta: Meta,
}
