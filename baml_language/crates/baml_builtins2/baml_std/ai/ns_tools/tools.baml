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
    /// aligns a model's arguments to its handler's parameters
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

/// (internal) The arguments of a tool call as `reflect.call_any` takes them.
///
/// A provider hands over a model's arguments as parsed JSON: a class is a
/// map, an enum a string, a list a list of json. `reflect.call_any` takes
/// values of the parameter types, and the only JSON values that are such
/// values are the scalars. So the parameters that were sent JSON are read as
/// the fields of one class, the JSON is aligned to that class as any other
/// model output is (`baml.sap.coerce`), and the fields of the result are
/// the arguments.
///
/// An argument passes as it arrived when it names no parameter, and when it is
/// already a value of its parameter's type: that is what BAML code passes when
/// it calls a tool itself, and it is every value of an `unknown` parameter.
/// `reflect.call_any` judges every argument that passed.
function _typed_arguments(
    tool: string,
    handler: reflect.AnyFunction,
    args: map<string, unknown>,
) -> map<string, unknown> throws baml.errors.InvalidArgument {
    let sig = reflect.signature(handler);
    let fields: map<string, reflect.Type | reflect.WithMeta<reflect.Type>> = {};
    let sent: map<string, baml.json.json> = {};
    for (let param in sig.args.concat(sig.opts.values())) {
        if (!args.has(param.name)) {
            continue;
        }
        if let value: baml.json.json = args.get(param.name) {
            type Declared = unreflect(param.type);
            if (!(value is Declared)) {
                let _ = fields.set(param.name, param.type);
                let _ = sent.set(param.name, value);
            }
        }
    }
    if (fields.length() == 0) {
        return args;
    }
    let parameters = reflect.class.new("Arguments", fields) catch (e) {
        reflect.errors.CompilationError => {
            throw baml.errors.InvalidArgument { message: `invalid arguments for ${tool}: ${e.message}` }
        },
    };
    type Arguments = unreflect(parameters.as_type());
    let aligned = baml.sap.coerce<Arguments>(sent) catch (e) {
        baml.errors.ParseError => {
            // The text goes back to the model, and it quotes the value that
            // did not align, whatever its size.
            throw baml.errors.InvalidArgument { message: `invalid arguments for ${tool}: ${e.message.slice(0, 1000)}` }
        },
    };
    let typed: map<string, unknown> = {};
    for (let name in args.keys()) {
        let _ = typed.set(name, args.get(name));
    }
    for (let name in fields.keys()) {
        let value = reflect.class.get_field<unknown>(aligned, name) catch (e) {
            reflect.errors.CompilationError => {
                baml.sys.panic(`the aligned arguments of ${tool} have no field ${name}`)
            },
        };
        let _ = typed.set(name, value);
    }
    typed
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

