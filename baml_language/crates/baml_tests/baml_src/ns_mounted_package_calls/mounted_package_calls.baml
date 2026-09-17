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

                interface Makeable {
                    function make(x: int) -> Self throws never
                }

                interface Measured {
                    function measure(self) -> int throws never
                }

                interface Sizing {
                    function tag_as<T>(self, tag: T) -> string throws never
                    function of<T extends Measured>(self, item: T) -> int throws never
                }

                interface Greeter {
                    function greet(self) -> string throws never
                    function greet_twice(self) -> string throws never {
                        self.greet() + self.greet()
                    }
                }

                class Pebble {
                    weight int

                    implements Measured {
                        function measure(self) -> int throws never {
                            self.weight
                        }
                    }
                }

                interface Labeled {
                    label string
                }

                class Widget {
                    x int
                    label string

                    function get_x(self) -> int throws never {
                        self.x
                    }

                    function origin() -> Widget throws never {
                        Widget { x: 0, label: "origin" }
                    }

                    implements Labeled {}

                    implements Describable {
                        function describe(self) -> string throws never {
                            self.label
                        }
                    }

                    implements Makeable {
                        function make(x: int) -> Widget throws never {
                            Widget { x: x, label: "made" }
                        }
                    }

                    implements Sizing {
                        function tag_as<T>(self, tag: T) -> string throws never {
                            if reflect.Type.of<T>() == reflect.Type.of<int>() {
                                self.label + ":int"
                            } else {
                                self.label + ":other"
                            }
                        }
                        function of<T extends Measured>(self, item: T) -> int throws never {
                            self.x + item.measure()
                        }
                    }

                    implements Greeter {
                        function greet(self) -> string throws never {
                            "hi " + self.label
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

// A mounted `self`-less impl method called through its class resolves
// type-keyed on the class (`app.Widget.make(3)`), and a mounted impl method
// referenced as a value is a virtual function bound to the class's impl,
// taking the receiver as its first argument — the roads a source static and
// `Widget.describe` take, never a bare constant naming the interface slot.
test "mounted impl statics and method values take the source roads" {
    let app = mpc_library();
    let consumer = reflect.Package.compile(
        {
            "main.baml": `
                function static_call() -> int throws never {
                    app.Widget.make(3).x
                }

                function value_reference() -> string throws never {
                    let f = app.Widget.describe;
                    f(app.Widget { x: 1, label: "v" })
                }
            `,
        },
        packages = { "app": app },
    );
    assert.equal(consumer.diagnostics().length(), 0);
    let static_call = consumer.get_function<() -> int>("root.static_call") ?? throw "missing root.static_call";
    assert.equal(static_call(), 3);
    let value_reference = consumer.get_function<() -> string>("root.value_reference") ?? throw "missing root.value_reference";
    assert.equal(value_reference(), "v")
}

// Type-rooted references to a mounted class's impl-provided methods take the
// source lane's projection tier: a generic method as a value carries its
// frame (annotated and turbofish alike), a bounded one registers its bound,
// and the interface-qualified and written-qualifier spellings against a
// mounted interface resolve — its default method included.
test "mounted impl methods as values and through qualified spellings" {
    let app = mpc_library();
    let consumer = reflect.Package.compile(
        {
            "main.baml": `
                function annotated_value() -> string throws never {
                    let f: (app.Widget, int) -> string throws never = app.Widget.tag_as;
                    f(app.Widget { x: 1, label: "w" }, 7)
                }

                function turbofish_value() -> string throws never {
                    let f = app.Widget.tag_as<string>;
                    f(app.Widget { x: 2, label: "t" }, "eight")
                }

                function bounded_value() -> int throws never {
                    let f: (app.Widget, app.Pebble) -> int throws never = app.Widget.of;
                    f(app.Widget { x: 40, label: "b" }, app.Pebble { weight: 2 })
                }

                function interface_qualified_call() -> string throws never {
                    app.Greeter.greet(app.Widget { x: 1, label: "q" })
                }

                function written_qualifier_default() -> string throws never {
                    (app.Widget as app.Greeter).greet_twice(app.Widget { x: 1, label: "d" })
                }
            `,
        },
        packages = { "app": app },
    );
    assert.equal(consumer.diagnostics().length(), 0);
    let annotated_value = consumer.get_function<() -> string>("root.annotated_value") ?? throw "missing root.annotated_value";
    assert.equal(annotated_value(), "w:int");
    let turbofish_value = consumer.get_function<() -> string>("root.turbofish_value") ?? throw "missing root.turbofish_value";
    assert.equal(turbofish_value(), "t:other");
    let bounded_value = consumer.get_function<() -> int>("root.bounded_value") ?? throw "missing root.bounded_value";
    assert.equal(bounded_value(), 42);
    let interface_qualified_call = consumer.get_function<() -> string>("root.interface_qualified_call") ?? throw "missing root.interface_qualified_call";
    assert.equal(interface_qualified_call(), "hi q");
    let written_qualifier_default = consumer.get_function<() -> string>("root.written_qualifier_default") ?? throw "missing root.written_qualifier_default";
    assert.equal(written_qualifier_default(), "hi dhi d")
}

// A mounted INTERFACE FIELD is read through an existential at runtime (the
// virtual field road keyed on the row), and a mounted class's INHERENT
// static resolves type-keyed on the class — neither goes through an impl.
test "mounted interface fields read through an existential and inherent statics resolve" {
    let app = mpc_library();
    let consumer = reflect.Package.compile(
        {
            "main.baml": `
                function read_label(l: app.Labeled) -> string throws never {
                    l.label
                }

                function origin_x() -> int throws never {
                    app.Widget.origin().x
                }

                function main() -> string throws never {
                    read_label(app.Widget { x: 1, label: "lbl" }) + ":" + origin_x().to_string()
                }
            `,
        },
        packages = { "app": app },
    );
    assert.equal(consumer.diagnostics().length(), 0);
    let main = consumer.get_function<() -> string>("root.main") ?? throw "missing root.main";
    assert.equal(main(), "lbl:0")
}
