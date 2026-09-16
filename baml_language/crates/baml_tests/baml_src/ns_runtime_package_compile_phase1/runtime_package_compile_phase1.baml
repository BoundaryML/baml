// Native coverage for the language-observable parts of runtime package
// compilation. Host logger state and BexExternalValue layout remain Rust tests.

client RpcTestClient = openai.ResponsesClient.new(
    model = "unused-network-free-companions",
    api_key = "unused",
)

function rpc_extract<T>(document: string) -> T {
    client: RpcTestClient
    prompt: `Extract the document using this schema:\n${ctx.output_format()}`
}

function rpc_compile_method_plugin() -> reflect.Package {
    // Mount a deliberately small package. `Package.current()` is the entire
    // native corpus here, unlike the standalone source used by the old Rust
    // wrapper, and exporting that whole corpus would obscure this contract.
    let app = reflect.Package.compile(
        {
            "app.baml": `
                interface Describable {
                    function describe(self) -> string throws never
                    function shout(self) -> string throws never {
                        self.describe().to_upper_case()
                    }
                }

                class Host {
                    name string
                    function greet(self, punct: string) -> string throws never {
                        "hi " + self.name + punct
                    }
                    function make(name: string) -> Host throws never {
                        Host { name: name }
                    }
                    implements Describable {
                        function describe(self) -> string throws never {
                            "host<" + self.name + ">"
                        }
                    }
                }

                class Other { id int }
                implement Describable for Other {
                    function describe(self) -> string throws never {
                        "other#" + self.id.to_string()
                    }
                }

                function describe_all(items: Describable[]) -> string throws never {
                    let out = ""
                    for (let item in items) { out = out + item.describe() + ";" }
                    out
                }
            `,
        },
    );
    reflect.Package.compile(
        {
            "plugin.baml": `
                class Widget { name string }

                implement app.Describable for Widget {
                    function describe(self) -> string throws never {
                        "widget<" + self.name + ">"
                    }
                }

                function make_widget(name: string) -> Widget throws never {
                    Widget { name: name }
                }
                function call_host_inherent(
                    h: app.Host,
                ) -> string throws never {
                    h.greet("!")
                }
                function call_host_static(name: string) -> string throws never {
                    app.Host.make(name).greet("?")
                }
                function call_host_impl_block(
                    h: app.Host,
                ) -> string throws never {
                    h.describe()
                }
                function call_host_default(
                    h: app.Host,
                ) -> string throws never {
                    h.shout()
                }
                function call_other_out_of_body(
                    o: app.Other,
                ) -> string throws never {
                    o.describe()
                }
                function existential(
                    d: app.Describable,
                ) -> string throws never {
                    d.describe()
                }
                function pass_through(
                    items: app.Describable[],
                ) -> string throws never {
                    app.describe_all(items)
                }

                function exercise() -> string throws never {
                    let widget = make_widget("w1")
                    let host = app.Host { name: "h" }
                    let parts = [
                        widget.describe(),
                        widget.shout(),
                        app.describe_all([widget, host, app.Other { id: 7 }]),
                        call_host_inherent(host),
                        call_host_static("s"),
                        call_host_impl_block(host),
                        call_host_default(host),
                        call_other_out_of_body(app.Other { id: 3 }),
                        existential(widget),
                        pass_through([widget, host]),
                    ]
                    parts.join("|")
                }
            `,
        },
        packages = { "app": app },
    )
}

test "method-bearing interface crosses a runtime package mount" {
    let pkg = rpc_compile_method_plugin();
    let exercise = pkg.get_function<() -> string>("root.exercise") ?? throw "missing exercise";
    assert.equal(
        exercise(),
        "widget<w1>|WIDGET<W1>|widget<w1>;host<h>;other#7;|hi h!|hi s?|host<h>|HOST<H>|other#3|widget<w1>|widget<w1>;host<h>;",
    )
}

test "mounted method stubs do not shadow their owner" {
    assert.equal(rpc_compile_method_plugin().diagnostics().length(), 0)
}

test "mounted associated bindings resolve earlier Self witnesses before emit" {
    let app = reflect.Package.compile(
        {
            "app.baml": `
                interface Batch {
                    type Item
                    type Items

                    function all(self) -> Self.Items throws never
                }
            `,
        },
    );
    let consumer = reflect.Package.compile(
        {
            "consumer.baml": `
                class IntBatch {
                    values: int[]

                    implements app.Batch {
                        type Item = int
                        type Items = Self.Item[]

                        function all(self) -> Self.Items throws never {
                            self.values
                        }
                    }
                }

                class QualifiedIntBatch {
                    values: int[]

                    implements app.Batch {
                        type Item = int
                        type Items = (Self as app.Batch).Item[]

                        function all(self) -> Self.Items throws never {
                            self.values
                        }
                    }
                }

                function total(batch: app.Batch<Item = int, Items = int[]>) -> int throws never {
                    let result = 0
                    for (let value in batch.all()) {
                        result += value
                    }
                    result
                }

                function exercise() -> int throws never {
                    total(IntBatch { values: [2, 3, 5] })
                }

                function exercise_qualified() -> int throws never {
                    total(QualifiedIntBatch { values: [7, 11] })
                }
            `,
        },
        packages = { "app": app },
    );
    if (consumer.diagnostics().length() != 0) {
        throw consumer.diagnostics()[0].message
    }
    let exercise = consumer.get_function<() -> int>("root.exercise") ?? throw "missing exercise";
    let exercise_qualified = consumer.get_function<() -> int>("root.exercise_qualified")
        ?? throw "missing qualified exercise";
    assert.equal(exercise(), 10);
    assert.equal(exercise_qualified(), 18)
}

test "mounted interface impl missing a required method reports E0113" {
    let app = reflect.Package.compile(
        {
            "app.baml": `
                interface Required {
                    function required(self) -> string throws never
                }
            `,
        },
    );
    let result = reflect.Package.compile(
        {
            "consumer.baml": `
                class Incomplete {
                    implements app.Required {}
                }
            `,
        },
        packages = { "app": app },
    ) catch (e) {
        reflect.errors.CompilationError => e.diagnostics,
    };
    if (result is reflect.Diagnostic[]) {
        assert.equal(
            result
                .filter((diagnostic) -> bool {
                    diagnostic.code == "E0113" && diagnostic.message.includes("required")
                })
                .length(),
            1,
        )
    } else {
        throw "incomplete mounted impl compiled without E0113"
    }
}

test "runtime-compiled ToString on a reflected type dispatches" {
    let pkg = reflect.Package.compile(
        {
            "main.baml": `
                class C {}
                function render() -> string {
                    reflect.Type.of_value(C {}).to_string()
                }
            `,
        },
    );
    let render = pkg.get_function<() -> string>("root.render") ?? throw "missing render";
    assert.equal(render(), "C")
}

test "Package finish refuses a Session compile artifact" {
    let session = reflect.Session.new();
    let artifact = session._compile<int>(`1`);
    let rejected = false;
    let _ = reflect.Package._finish(artifact, {}) catch (_) {
        _ => {
            rejected = true
        },
    };
    assert.is_true(rejected);
    assert.equal(session.eval<int>(`2`), 2)
}

function rpc_schema_package() -> reflect.Package {
    reflect.Package.compile(
        {
            "schema.baml": `
                class ExtractedRecord {
                    account string
                    amount int
                }
            `,
        },
    )
}

test "runtime schema compiles parses and encodes" {
    let pkg = rpc_schema_package();
    let record_t = pkg.get_class("root.ExtractedRecord") ?? throw "missing class";
    type Record = unreflect(record_t.as_type())
    let record = rpc_extract@parse<Record>(`{"account":"AC-1","amount":42}`);
    assert.equal(json.to_string(record), `{"account":"AC-1","amount":42}`)
}

function rpc_compare_hot<T extends baml.ops.Compare>(value: T, n: int) -> int throws never {
    let count = 0;
    for (let i = 0; i < n; i += 1) {
        if (value <= value) {
            count += 1
        } else {
            count -= 1
        }
    }
    count
}

test "runtime-compiled code dispatches through static stdlib impls" {
    let package = reflect.Package.compile(
        {
            "dispatch.baml": `
                function compare_hot<T extends baml.ops.Compare>(
                    value: T,
                    n: int,
                ) -> int throws never {
                    let count = 0
                    for (let i = 0; i < n; i += 1) {
                        if (value <= value) { count += 1 } else { count -= 1 }
                    }
                    count
                }
                function run(n: int) -> int throws never {
                    compare_hot<int>(7, n)
                }
            `,
        },
    );
    let run = package.get_function<(int) -> int>("root.run") ?? throw "missing run";
    let before_gc = run(100);
    baml.sys.collect_garbage();
    assert.equal(before_gc, 100);
    assert.equal(run(100), rpc_compare_hot<int>(7, 100))
}

class RpcUser {
    name: string,
    age: int,
}

test "exact runtime types preserve static generic JSON calls" {
    let original = RpcUser { name: "Ada", age: 30 };
    let decoded = RpcUser.from_json(original.to_json());
    assert.equal(decoded.name, "Ada")
}

test "render prompt uses the runtime package schema" {
    let record_t = rpc_schema_package().get_class("root.ExtractedRecord") ?? throw "missing class";
    type Record = unreflect(record_t.as_type())
    let prompt = rpc_extract@render_prompt<Record>("sample document").text();
    assert.contains(prompt, "account");
    assert.contains(prompt, "amount")
}

test "runtime package declarations are stable per package and generative per compile" {
    let files = { "schema.baml": `class ExtractedRecord { account string amount int }` };
    let first = reflect.Package.compile(files);
    let second = reflect.Package.compile(files);
    let a = first.get_class("root.ExtractedRecord") ?? throw "missing first";
    let a_again = first.get_class("root.ExtractedRecord") ?? throw "missing first again";
    let b = second.get_class("root.ExtractedRecord") ?? throw "missing second";
    assert.equal(a.as_type(), a_again.as_type());
    assert.is_true(a.as_type() != b.as_type())
}

test "runtime package and declarations survive major collection" {
    let pkg = rpc_schema_package();
    let before = pkg.get_class("root.ExtractedRecord") ?? throw "missing before GC";
    baml.sys.collect_garbage();
    let after = pkg.get_class("root.ExtractedRecord") ?? throw "missing after GC";
    assert.equal(before.as_type(), after.as_type())
}

test "runtime compile honors namespaces and mounted dependencies" {
    let base = reflect.Package.compile({ "ns_models/base.baml": `class Base { id string }` });
    let child = reflect.Package.compile(
        { "child.baml": `class Child { base dep.models.Base }` },
        packages = { "dep": base },
    );
    assert.is_true(base.get_class("root.models.Base") != null);
    assert.is_true(child.get_class("root.Child") != null);
    assert.equal(child.diagnostics().length(), 0)
}

test "mounted runtime types do not leak into phantom stub diagnostics" {
    let runtime_minted = reflect.class.new("RpcRuntimeMinted", { "value": reflect.Type.of<string>() });
    let app = reflect.Package.compile({ "app.baml": `function ready() -> bool { true }` }).with_types(
        { "RpcRuntimeMinted": runtime_minted },
    );
    let dependency = reflect.Package.compile(
        {
            "dependency.baml": `
                interface CarriesRuntimeMint {
                    item app.RpcRuntimeMinted
                }
                function make_runtime_minted() -> app.RpcRuntimeMinted {
                    app.RpcRuntimeMinted { value: "ok" }
                }
            `,
        },
        packages = { "app": app },
    );
    let consumer = reflect.Package.compile(
        { "consumer.baml": `function ready() -> bool { true }` },
        packages = { "dependency": dependency },
    );
    assert.equal(consumer.diagnostics().length(), 0)
}

function rpc_simple_function_package() -> reflect.Package {
    reflect.Package.compile({ "main.baml": `function Present(value: string) -> string { value }` })
}

test "get_function absence is null" {
    assert.is_true(
        rpc_simple_function_package().get_function<(string) -> string>("root.Missing") == null,
    )
}

test "get_function mismatch throws compiler subtyping diagnostic" {
    let result = rpc_simple_function_package().get_function<(int) -> string>("root.Present") catch (e) {
        reflect.errors.CompilationError => e.diagnostics,
    };
    if (result is reflect.Diagnostic[]) {
        assert.is_true(
            result
                .filter((diagnostic) -> bool {
                    diagnostic.code == "E0001" && diagnostic.message.includes("not a subtype")
                })
                .length()
                > 0,
        )
    } else {
        throw "mismatched function contract did not throw"
    }
}

function rpc_generic_package() -> reflect.Package {
    reflect.Package.compile(
        {
            "main.baml": `
                client Dummy = openai.ResponsesClient.new(
                    model = "unused-reflection-only",
                    api_key = "unused",
                )
                function Extract<T>(document: string) -> T {
                    client: Dummy
                    prompt: "Extract document"
                }
            `,
        },
    )
}

test "unspecialized generic get_function reports the reflection limit" {
    let result = rpc_generic_package().get_function<(string) -> string>("root.Extract") catch (e) {
        reflect.errors.CompilationError => e.diagnostics,
    };
    if (result is reflect.Diagnostic[]) {
        assert.is_true(
            result
                .filter((diagnostic) -> bool {
                    diagnostic.code == "E0165"
                        && diagnostic.message.includes(
                            "signature still mentions its own type parameters",
                        )
                })
                .length()
                > 0,
        )
    } else {
        throw "unspecialized generic extraction did not throw"
    }
}

test "function listing omits unspecialized generics" {
    let pkg = reflect.Package.compile(
        {
            "main.baml": `
                function identity<T>(value: T) -> T { value }
                function Present(value: string) -> string { value }
            `,
        },
    );
    let functions = pkg.functions();
    assert.is_true(functions.get("root.identity") == null);
    assert.is_true(functions.get("root.Present") != null)
}

test "generic function companion extraction reports the reflection limit" {
    let result = rpc_generic_package().get_function<(string) -> ai.Prompt>("root.Extract@render_prompt") catch (e) {
        reflect.errors.CompilationError => e.diagnostics,
    };
    if (result is reflect.Diagnostic[]) {
        assert.is_true(
            result
                .filter((diagnostic) -> bool {
                    diagnostic.code == "E0165"
                })
                .length()
                > 0,
        )
    } else {
        throw "generic companion extraction did not throw"
    }
}

test "generic function companion remains in function listing" {
    assert.is_true(rpc_generic_package().functions().get("root.Extract@render_prompt") != null)
}

test "alias maps are order-independent and cannot shadow stdlib" {
    let root_package = reflect.Package.compile(
        {
            "app.baml": `
                class AgentState { goal string history string[] }
                function Plan(state: AgentState) -> string {
                    "planned " + state.goal
                }
            `,
        },
    );
    let generated = reflect.Package.compile(
        {
            "main.baml": `
                function Read(state: app.AgentState) -> string {
                    app.Plan(state)
                        + ":"
                        + baml.Array.length(["o", "k"]).to_string()
                }
                function main() -> string {
                    Read(app.AgentState { goal: "ordered", history: [] })
                }
            `,
        },
        packages = { "z_last": root_package, "app": root_package },
    );
    let main = generated.get_function<() -> string>("root.main") ?? throw "missing root.main";
    assert.equal(main(), "planned ordered:2");

    let rejected = false;
    let _ = reflect.Package.compile(
        { "main.baml": `function main() -> int { 1 }` },
        packages = { "baml": root_package },
    ) catch (e) {
        reflect.errors.CompilationError => {
            rejected = true
        },
    };
    assert.is_true(rejected)
}

test "package tests enumerate invocable zero-argument functions" {
    let pkg = reflect.Package.compile(
        {
            "main.baml": `
                test "enumerated package test" {
                    assert.equal(1, 1)
                }
            `,
        },
    );
    let tests = pkg.tests();
    let run = tests.get("root::enumerated package test") ?? throw "test not enumerated";
    run();
    assert.equal(tests.length(), 1)
}

test "one package under two aliases is one package" {
    let root_package = reflect.Package.compile({ "twin.baml": `class Twin { x int }` });
    let generated = reflect.Package.compile(
        {
            "main.baml": `
                function cross(
                    value: app.Twin,
                ) -> z_last.Twin {
                    let same: z_last.Twin = value
                    same
                }
                function main() -> int {
                    cross(app.Twin { x: 7 }).x
                }
            `,
        },
        packages = { "app": root_package, "z_last": root_package },
    );
    if (generated.diagnostics().length() != 0) {
        throw generated.diagnostics()[0].message
    }
    let main = generated.get_function<() -> int>("root.main") ?? throw "missing root.main";
    assert.equal(main(), 7)
}
