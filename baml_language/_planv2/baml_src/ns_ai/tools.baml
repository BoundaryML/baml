// Tools are plain functions. Schemas come from reflection; execution is
// reflect.call_any inside the runner, never inside a client.
// Mirrors _planv2/pages/02_guides/01_functions/02_tools.md.

enum ToolErrorMode {
    Report, // the failure becomes the call's result; the model adapts
    Raise, // the failure is journaled, then the run throws root.ai.errors.ToolFailedError
}

class Tool {
    name: string,
    description: string,
    input_schema: map<string, unknown>, // JSON Schema for the arguments object
    handler: baml.AnyFunction,
    on_error: ToolErrorMode?, // null: inherit the run's tool_errors mode

    // The total boundary around one call: validate, dispatch through
    // reflection, serialize. A throw here is the tool's failure, not the
    // run's. The reflection boundary widens exactly representable integral
    // arguments into float parameters (see reflect.call_any).
    function call(self, args: map<string, unknown>) -> string {
        let value = reflect.call_any(self.handler, args) catch (e) {
            let bad: reflect.InvalidArgumentError => {
                throw baml.errors.InvalidArgument { message: `invalid arguments for ${self.name}: ${bad.to_string()}` }
            },
        };
        baml.json.stringify(baml.json.to_json(value))
    }
}

// The explicit constructor. `tools: [f]` normalizes through this.
function tool(
    handler: baml.AnyFunction,
    name: string? = null,
    description: string? = null,
    on_error: ToolErrorMode? = null,
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
            input_schema: root.ai.internal._schema_for_signature(handler),
            handler: handler,
            on_error: on_error,
        }
    } else {
        throw baml.errors.InvalidArgument { message: "anonymous tool needs a name" }
    }
}

class Toolbox {
    entries: Tool[],

    // Rejects duplicate names, so a toolbox is well-formed on construction.
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


