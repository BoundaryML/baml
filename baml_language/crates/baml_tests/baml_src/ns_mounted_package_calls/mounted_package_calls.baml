// End-to-end calls through source-less runtime package artifacts. The old
// Rust suite rebuilt the same library and consumer once per assertion; the
// main matrix aggregates all but the generic-return inference regression,
// which stays isolated below so unrelated object layout cannot mask it.

function mpc_library() -> reflect.Package {
    reflect.Package.compile(
        {
            "lib.baml": `
                interface Describable {
                    function describe(self) -> string throws never
                }

                interface Mergeable {
                    function merge(self, other: Self) -> Self throws never
                }

                interface Tagged {
                    function tag(self) -> string throws never
                }

                class Widget {
                    x int
                    label string

                    function get_x(self) -> int throws never {
                        self.x
                    }

                    implements Describable {
                        function describe(self) -> string throws never {
                            self.label
                        }
                    }
                }

                class Bag {
                    n int

                    implements Mergeable {
                        function merge(self, other: Self) -> Self throws never {
                            Bag { n: self.n + other.n }
                        }
                    }
                }

                class Plain {
                    text string
                }

                implement Describable for Plain {
                    function describe(self) -> string throws never {
                        self.text
                    }
                }

                implement<T> Tagged for T {
                    function tag(self) -> string throws never {
                        "any"
                    }
                }

                class ParseError {
                    message string
                }

                function add(a: int, b: int) -> int throws never {
                    a + b
                }

                function apply_callback(cb: (x: int) -> int) -> int {
                    cb(41)
                }

                function widget_x(w: Widget) -> int throws never {
                    w.x
                }

                function merge_both<T extends Mergeable>(a: T, b: T) -> T throws never {
                    a.merge(b)
                }

                function tag_of<T extends Tagged>(value: T) -> string throws never {
                    value.tag()
                }

                function parse_positive(n: int) -> int throws ParseError {
                    if n < 0 {
                        throw ParseError { message: "negative" }
                    }
                    n
                }
            `,
        },
    )
}

function mpc_consumer() -> reflect.Package {
    let app = mpc_library();
    reflect.Package.compile(
        {
            "main.baml": `
                client Dummy = openai.ResponsesClient.new(
                    model = "gpt-4o",
                    api_key = "test",
                )

                class Mine {
                    v int
                }

                function apply(f: (a: int, b: int) -> int, a: int, b: int) -> int {
                    f(a, b)
                }

                function plus_one(x: int) -> int throws never {
                    x + 1
                }

                function classify(n: int) -> string {
                    let value = app.parse_positive(n) catch (error) {
                        let typed: app.ParseError => {
                            return typed.message
                        }
                    }
                    "ok"
                }

                function combine<T extends app.Mergeable>(a: T, b: T) -> T throws never {
                    a.merge(b)
                }

                // Compiling this declaration proves the mounted package exports
                // its canonical PPIR-generated streaming companion.
                function Ask() -> app.Widget {
                    client: Dummy
                    prompt: "Return a widget"
                }

                function exercise() -> string {
                    let direct = app.add(1, 2)
                    let reference = apply(app.add, 20, 22)
                    let callback = app.apply_callback(plus_one)

                    let widget = app.Widget { x: 40, label: "hello" }
                    let class_result = widget.x + app.widget_x(widget) + widget.get_x() - 2 * 40 + 2
                    let class_impl = widget.describe()
                    let out_of_body = app.Plain { text: "out-of-body" }.describe()

                    let merged = app.Bag { n: 40 }.merge(app.Bag { n: 2 }).n
                    let existential: app.Describable = app.Widget { x: 1, label: "via-existential" }
                    let existential_result = existential.describe()

                    let generic_user = app.tag_of(Mine { v: 1 })
                    let blanket_member = Mine { v: 1 }.tag()

                    let caught = classify(7) + ":" + classify(0 - 5)
                    let user_bound = combine(
                        app.Bag { n: 30 },
                        app.Bag { n: 12 },
                    ).n
                    let ufcs = app.Widget.describe(
                        app.Widget { x: 1, label: "l" },
                    )

                    direct.to_string()
                        + "|" + reference.to_string()
                        + "|" + callback.to_string()
                        + "|" + class_result.to_string()
                        + "|" + class_impl
                        + "|" + out_of_body
                        + "|" + merged.to_string()
                        + "|" + existential_result
                        + "|" + generic_user
                        + "|" + blanket_member
                        + "|" + caught
                        + "|" + user_bound.to_string()
                        + "|" + ufcs
                }
            `,
        },
        packages = { "app": app },
    )
}

test "mounted source-less package calls preserve the complete surface" {
    let consumer = mpc_consumer();
    assert.equal(consumer.diagnostics().length(), 0);
    let exercise = consumer.get_function<() -> string>("root.exercise") ?? throw "missing root.exercise";
    assert.equal(
        exercise(),
        "3|42|42|42|hello|out-of-body|42|via-existential|any|any|ok:negative|42|l",
    )
}

// Keep this generic-return inference case isolated from the larger matrix.
// Link-only source stubs intentionally erase argument types to `unknown`; the
// mounted interface row, not that stub, must therefore drive `T = app.Bag` so
// the unannotated `.n` access lowers as a class field rather than map access.
test "mounted generic return retains its concrete class type" {
    let app = mpc_library();
    let consumer = reflect.Package.compile(
        {
            "main.baml": `
                function main() -> int throws never {
                    app.merge_both(
                        app.Bag { n: 2 },
                        app.Bag { n: 40 },
                    ).n
                }
            `,
        },
        packages = { "app": app },
    );
    let main = consumer.get_function<() -> int>("root.main") ?? throw "missing root.main";
    assert.equal(main(), 42)
}

// Regression for B-1752: declaring an alias in a runtime-compiled package
// must not prevent that package from being mounted by a later compile.
test "mounted package supports declared type aliases" {
    let app = reflect.Package.compile(
        {
            "lib.baml": `
                type Name = string

                function greet(name: Name) -> Name throws never {
                    "Hello, " + name
                }
            `,
        },
    );
    let consumer = reflect.Package.compile(
        {
            "main.baml": `
                function main() -> string throws never {
                    app.greet("Ada")
                }
            `,
        },
        packages = { "app": app },
    );
    let main = consumer.get_function<() -> string>("root.main") ?? throw "missing root.main";
    assert.equal(main(), "Hello, Ada")
}
