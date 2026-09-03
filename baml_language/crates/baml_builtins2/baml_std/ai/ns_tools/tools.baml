// Tools are plain functions. Schemas come from reflection; execution is
// reflect.call_any inside the runner, never inside a client.
// Mirrors _planv2/pages/02_guides/01_functions/02_tools.md.

enum ErrorMode {
    Report, // the failure becomes the call's result; the model adapts
    Raise, // the failure is journaled, then the run throws root.errors.ToolFailedError
}

class Tool {
    name: string,
    description: string,
    input_schema: map<string, unknown>, // JSON Schema for the arguments object
    handler: reflect.AnyFunction?, // signature-derived dispatch; null for raw tools
    // Dynamic-source dispatch: the handler receives the validated-by-schema
    // argument map as one value. Set by raw_tool() when the schema does not
    // derive from a BAML signature (MCP servers, OpenAPI specs).
    raw_handler: ((map<string, unknown>) -> string throws unknown)?,
    on_error: ErrorMode?, // null: inherit the run's tool_errors mode

    /// The total boundary around one call. A throw here is the tool's
    /// failure, not the run's.
    ///
    /// A raw tool (`raw_tool`) hands the argument map straight to its
    /// `raw_handler` and returns that string as-is. A schema-derived tool
    /// validates, dispatches through reflection, and JSON-serializes the
    /// result; that reflection boundary widens exactly representable integral
    /// arguments into float parameters (see reflect.call_any).
    function call(self, args: map<string, unknown>) -> string {
        if let rh: (map<string, unknown>) -> string throws unknown = self.raw_handler {
            return rh(args);
        }
        if let h: reflect.AnyFunction = self.handler {
            let value = reflect.call_any(h, args) catch (e) {
                let bad: reflect.InvalidArgumentError => {
                    throw baml.errors.InvalidArgument { message: `invalid arguments for ${self.name}: ${bad.to_string()}` }
                },
            };
            baml.json.stringify(baml.json.to_json(value))
        } else {
            throw baml.errors.InvalidArgument { message: `tool ${self.name} has no handler` }
        }
    }
}

/// The explicit constructor. `tools: [f]` normalizes through this.
function tool(
    handler: reflect.AnyFunction,
    name: string? = null,
    description: string? = null,
    on_error: ErrorMode? = null,
) -> Tool {
    let sig = reflect.signature(handler);
    let resolved_name = name ?? sig.name;
    if let n: string = resolved_name {
        if (n == "__baml_return_output") {
            throw baml.errors.InvalidArgument { message: "the tool name __baml_return_output is reserved" }
        }
        Tool {
            name: n,
            description: description ?? sig.docstring ?? "",
            input_schema: root.internal._schema_for_signature(handler),
            handler: handler,
            raw_handler: null,
            on_error: on_error,
        }
    } else {
        throw baml.errors.InvalidArgument { message: "anonymous tool needs a name" }
    }
}

/// The constructor for tools whose schema does not derive from a BAML
/// signature: the caller supplies the JSON Schema and a handler over the raw
/// argument map. This is how dynamic tool sources (MCP servers) become
/// ordinary tools.
function raw_tool(
    name: string,
    description: string,
    input_schema: map<string, unknown>,
    handler: (map<string, unknown>) -> string throws unknown,
    on_error: ErrorMode? = null,
) -> Tool {
    if (name == "__baml_return_output") {
        throw baml.errors.InvalidArgument { message: "the tool name __baml_return_output is reserved" }
    }
    Tool {
        name: name,
        description: description,
        input_schema: input_schema,
        handler: null,
        raw_handler: handler,
        on_error: on_error,
    }
}

class Toolbox {
    entries: Tool[],

    /// Rejects duplicate names, so a toolbox is well-formed on construction.
    function new(tools: Tool[]) -> Toolbox {
        let names: string[] = [];
        for (let t in tools) {
            if (names.includes(t.name)) {
                throw baml.errors.InvalidArgument { message: `duplicate tool name: ${t.name}` }
            }
            names.push(t.name);
        }
        Toolbox { entries: tools }
    }

    function list(self) -> Tool[] {
        self.entries
    }

    function get(self, name: string) -> Tool? {
        self.entries.find((t) -> {
            t.name == name
        })
    }

    function is_empty(self) -> bool {
        self.entries.length() == 0
    }
}

