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
    /// decodes each argument into its parameter's type
    /// (`_typed_arguments`), dispatches through reflection, which validates
    /// them, and JSON-serializes the result.
    function call(self, args: map<string, unknown>) -> string {
        if let rh: (map<string, unknown>) -> string throws unknown = self.raw_handler {
            return rh(args);
        }
        if let h: reflect.AnyFunction = self.handler {
            let value = reflect.call_any(h, _typed_arguments(self.name, h, args)) catch (e) {
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

/// (internal) A tool call's arguments, each decoded into the type of the
/// parameter it names.
///
/// A model's arguments are JSON: a class arrives as a map, an enum as a
/// string, a list as a list of json. `reflect.call_any` takes typed values and
/// rejects those. Schema-aligned parsing decodes them, because it reads the
/// names the tool's schema advertises (`@alias` included), where
/// `baml.json.to` reads declared names only.
///
/// An argument passes through as it arrived in three cases: it names no
/// parameter; the parser cannot model the parameter's type (`unknown`); or it
/// is already a value of the parameter's type, which is what BAML code passes
/// when it calls a tool itself. Decoding that one would write it as JSON under
/// its declared names and then fail to read an `@alias` field back.
/// `reflect.call_any` judges every argument that passed through.
function _typed_arguments(
    tool: string,
    handler: reflect.AnyFunction,
    args: map<string, unknown>,
) -> map<string, unknown> throws baml.errors.InvalidArgument {
    let sig = reflect.signature(handler);
    let params = sig.args.concat(sig.opts.values());
    let typed: map<string, unknown> = {};
    for (let name in args.keys()) {
        let _ = typed.set(name, args.get(name));
    }
    for (let param in params) {
        if (
            args.has(param.name)
                && baml.sap.can_parse(param.type)
                && !_is_value_of(param.type, args.get(param.name))
        ) {
            let written = baml.json.to_string(args.get(param.name)) catch_all (e) {
                _ => {
                    throw baml.errors.InvalidArgument { message: `invalid arguments for ${tool}: ${param.name}: ${e.to_string()}` }
                },
            };
            let value = baml.sap.parse_type(param.type, written) catch (e) {
                baml.errors.ParseError => {
                    throw baml.errors.InvalidArgument { message: `invalid arguments for ${tool}: ${param.name}: ${e.message}` }
                },
            };
            let _ = typed.set(param.name, value);
        }
    }
    typed
}

/// (internal) Whether `value` is already a value of the runtime type `t`.
function _is_value_of(t: reflect.Type, value: unknown) -> bool throws never {
    type T = unreflect(t);
    value is T
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

