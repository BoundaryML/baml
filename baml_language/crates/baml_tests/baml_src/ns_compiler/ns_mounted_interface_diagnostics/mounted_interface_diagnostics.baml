function webhooks_package() -> reflect.Package throws never {
    root.compiler.CompileFixturePackage({ "main.baml": `class WebhookNotifier { url: string }` })
}

test "local interface implementation for a foreign receiver dispatches" {
    let webhooks = webhooks_package();
    let app = reflect.Package.compile(
        {
            "main.baml": `
                interface Notifier {
                    function send(self, message: string) -> string throws never
                }
                implements Notifier for webhooks.WebhookNotifier {
                    function send(self, message: string) -> string {
                        "webhook to " + self.url + ": " + message
                    }
                }
                function notify(notifier: Notifier, message: string) -> string throws never {
                    notifier.send(message)
                }
                function main() -> string throws never {
                    notify(
                        webhooks.WebhookNotifier { url: "https://example.com/hooks/deploy" },
                        "Build passed",
                    )
                }
            `,
        },
        packages = { "webhooks": webhooks },
    );
    let main = app.get_function<() -> string>("main") ?? throw "missing main";
    assert.equal(main(), "webhook to https://example.com/hooks/deploy: Build passed")
}

test "foreign interface and foreign receiver violate local ownership" {
    let webhooks = webhooks_package();
    let diagnostics = root.compiler.CompileDiagnostics(
        {
            "main.baml": `
                implements baml.ToString for webhooks.WebhookNotifier {
                    function to_string(self) -> string { self.url }
                }
            `,
        },
        packages = { "webhooks": webhooks },
    );
    assert.equal(
        diagnostics
            .filter((diagnostic) -> bool {
                diagnostic.code == "E0139"
            })
            .length(),
        1,
    );

    // The runtime compiler adds source stubs for linkability. Those stubs
    // must retain the semantic metadata from the mounted package interface:
    // transitive requirements still reject an incomplete implementor, while
    // an associated-type default keeps its binding optional.
    let contracts = root.compiler.CompileFixturePackage(
        {
            "contracts.baml": `
                interface Base {}
                interface Child requires Base {}
                interface Grandchild requires Child {}
                interface Named {}
                interface Defaults { type Item = int }
                interface Bounded { type Item extends Named }
                class Box<T extends Named> { value T }
            `,
        },
    );
    let transitive_diagnostics = root.compiler.CompileDiagnostics(
        {
            "missing_requirement.baml": `
                class MissingRequirement { implements contracts.Grandchild {} }
            `,
        },
        packages = { "contracts": contracts },
    );
    assert.is_true(
        transitive_diagnostics.some((diagnostic) -> bool {
            diagnostic.code == "E0125"
        }),
    );
    assert.is_true(
        transitive_diagnostics.some((diagnostic) -> bool {
            diagnostic.message.includes("contracts.Base")
        }),
    );
    root.compiler.AssertCompiles(
        {
            "uses_default.baml": `
                class UsesDefault { implements contracts.Defaults {} }
            `,
        },
        packages = { "contracts": contracts },
    );
    root.compiler.AssertHasMessage(
        {
            "bad_associated_bound.baml": `
                class BadAssociatedBound {
                    implements contracts.Bounded { type Item = int }
                }
            `,
        },
        "does not implement bound `contracts.Named`",
        packages = { "contracts": contracts },
    );
    root.compiler.AssertCompiles(
        {
            "good_associated_bound.baml": `
                class NamedItem { implements contracts.Named {} }
                class GoodAssociatedBound {
                    implements contracts.Bounded { type Item = NamedItem }
                }
            `,
        },
        packages = { "contracts": contracts },
    );
    root.compiler.AssertHasMessage(
        {
            "bad_class_generic_bound.baml": `
                function rejects(value: contracts.Box<int>) -> int { 0 }
            `,
        },
        "expected `contracts.Named`, found `int`",
        packages = { "contracts": contracts },
    )
}

test "foreign field mapping requires the receiver owner" {
    let webhooks = webhooks_package();
    root.compiler.AssertHasCode(
        {
            "main.baml": `
                interface Destination { destination: string }
                implements Destination for webhooks.WebhookNotifier { destination as url }
            `,
        },
        "E0126",
        packages = { "webhooks": webhooks },
    )
}

// A mounted impl-provided method referenced as a value registers its own
// bounds exactly as the source lane does: an argument type that does not
// implement the bound is rejected at the reference, never left to fail at
// dispatch.
test "mounted bounded impl method as a value rejects an unbounded argument" {
    let sizing = root.compiler.CompileFixturePackage(
        {
            "sizing.baml": `
                interface Measured {
                    function measure(self) -> int throws never
                }
                interface Sizing {
                    function of<T extends Measured>(self, item: T) -> int throws never
                }
                class Rock {
                    weight int
                }
                class Widget {
                    x int

                    implements Sizing {
                        function of<T extends Measured>(self, item: T) -> int throws never {
                            self.x + item.measure()
                        }
                    }
                }
            `,
        },
    );
    root.compiler.AssertHasCode(
        {
            "main.baml": `
                function unbounded() -> int throws never {
                    let f: (sizing.Widget, sizing.Rock) -> int throws never = sizing.Widget.of;
                    f(sizing.Widget { x: 1 }, sizing.Rock { weight: 2 })
                }
            `,
        },
        "E0001",
        packages = { "sizing": sizing },
    )
}
