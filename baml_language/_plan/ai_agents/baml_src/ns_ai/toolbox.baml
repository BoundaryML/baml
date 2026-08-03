// Tools are plain functions. Schemas come from reflection (BEP-062);
// validation is reflect.call_any. Mirrors _plan/pages/02_guides/05_tools.md.

class Tool {
    name: string,
    f: baml.AnyFunction,
}

//# closures are tools too — they just need an explicit name

function tool(f: baml.AnyFunction, name: string? = null) -> Tool {
    let sig = reflect.signature(f);
    if let n: string = (name ?? sig.name) {
        Tool { name: n, f: f }
    } else {
        throw baml.errors.InvalidArgument { message: "anonymous tool needs a name" }
    }
}

class Toolbox {
    entries: Tool[],

    // prompt-side: schemas rendered straight from reflection
    function render(self) -> string {
        self.entries
            .map((t) -> {
                let sig = reflect.signature(t.f);
                let params = sig
                    .args
                    .map((a) -> {
                        `${a.name}: ${a.type.to_string()}`
                    })
                    .join(", ");
                `- ${t.name}(${params}) -> ${sig.returns.to_string()}\n  ${sig.docstring ?? ""}`
            })
            .join("\n")
    }

    function find(self, name: string) -> Tool? {
        self.entries.find((t) -> {
            t.name == name
        })
    }

    function names(self) -> string[] {
        self.entries.map((t) -> {
            t.name
        })
    }

    // call-side: reflect.call_any validates args against the real signature;
    // failures come back as an error result the model can read.
    function call(self, name: string, args: map<string, unknown>) -> string {
        if let t: Tool = self.find(name) {
            // to_json (not to_string): the only serializer that walks the
            // dynamic type of an `unknown` — see _plan/reference_notes.md
            baml.json.stringify(baml.json.to_json(reflect.call_any(t.f, args))) catch_all (e) {
                let bad: reflect.InvalidArgumentError => {
                    `{"error": "bad arguments: ${bad.to_string()}"}`
                },
                _ => `{"error": "${e.to_string()}"}`,
            }
        } else {
            `{"error": "no such tool: ${name}"}`
        }
    }
}
