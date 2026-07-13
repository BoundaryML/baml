/// Lower a BAML type to the restricted JSON Schema dialect accepted by
/// OpenAI Structured Outputs and strict function tools.
///
/// `baml.schema.json_schema` intentionally returns ordinary JSON Schema. This
/// provider-local pass closes every class object and marks all of its fields
/// required. Optional BAML fields remain nullable in their property schema;
/// `$defs` and `$ref` are retained for recursive class graphs.
function openai_strict_schema(t: type) -> json throws baml.errors.Unsupported {
  let schema = _openai_strict_schema_node(baml.schema.json_schema(t), "$");
  match (schema) {
    let object: map<string, json> if _json_schema_type(object) == "object" && object.has("properties") => object,
    _ => throw baml.errors.Unsupported {
      message: "OpenAI strict schemas must have an object root with declared properties",
    },
  }
}

function openai_chat_response_format(t: type, name: string) -> map<string, json>
    throws baml.errors.Unsupported {
  {
    "type": "json_schema",
    "json_schema": {
      "name": name,
      "strict": true,
      "schema": openai_strict_schema(t),
    },
  }
}

/// Encode a BAML return type as one forced strict function call. The outer
/// parameters object satisfies OpenAI's root-object rule, so `value` may be a
/// class, primitive, recursive type, or top-level union. `$defs` is lifted to
/// the parameters root so references inside `value` keep resolving correctly.
function openai_strict_output_tool(t: type, name: string) -> map<string, json>
    throws baml.errors.Unsupported {
  let strict = _openai_strict_schema_node(baml.schema.json_schema(t), "$.value");
  let parameters: map<string, json> = match (strict) {
    let object: map<string, json> => {
      let has_definitions = object.has("$defs");
      let definitions = object.get("$defs");
      if (has_definitions) { object.delete("$defs"); }
      let wrapped: map<string, json> = {
        "type": "object",
        "properties": { "value": object },
        "required": ["value"],
        "additionalProperties": false,
      };
      if (has_definitions) { wrapped.set("$defs", definitions); }
      wrapped
    },
    _ => throw baml.errors.Unsupported {
      message: "OpenAI strict output schema must be a JSON Schema object",
    },
  };
  {
    "type": "function",
    "function": {
      "name": name,
      "description": "Return the final typed result. Call this function exactly once.",
      "parameters": parameters,
      "strict": true,
    },
  }
}

function _openai_strict_schema_node(schema: json, path: string) -> json
    throws baml.errors.Unsupported {
  match (schema) {
    let object: map<string, json> => {
      for (let key in object.keys()) {
        object.set(key, _openai_strict_schema_node(object[key], path + "." + key));
      }

      if (_json_schema_type(object) == "object") {
        match (object.get("properties")) {
          let properties: map<string, json> => {
            let required = baml.json.from(properties.keys()) catch (e) {
              _ => throw baml.errors.Unsupported {
                message: "could not encode strict-schema property names at " + path,
              },
            };
            object.set("required", required);
            object.set("additionalProperties", false);
          },
          _ => throw baml.errors.Unsupported {
            message: "OpenAI strict mode cannot represent unconstrained or map-like object at " + path,
          },
        }
      }
      object
    },
    let array: json[] => array.map((item: json) -> json {
      _openai_strict_schema_node(item, path + "[]")
    }),
    _ => schema,
  }
}

function _json_schema_type(schema: map<string, json>) -> string throws never {
  match (schema.get("type")) {
    let value: string => value,
    _ => "",
  }
}
