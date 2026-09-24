/// A mounted enum the link stub cannot spell in full is refused at the mount
/// boundary. The enum's own name is a BAML identifier, so a consumer writes
/// `app.Color.Red` and the served interface — the semantic authority for a
/// mounted package — resolves it; without a stub there is no declaration
/// behind that reference for emit to reach. A partial stub is not an option:
/// the variant list is the enum's ABI.
function refused_variant() -> string {
    let color = reflect.enum.new("Color", ["Red", "bad-variant"]);
    let base = reflect.Package.compile({ "lib.baml": `function noop() -> int throws never { 0 }` });
    let app = base.with_types({ "Color": color });
    let outcome = reflect.Package.compile(
        { "main.baml": `function go() -> unknown throws never { app.Color.Red }` },
        packages = { "app": app },
    ) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code
    };
    if (outcome is string) {
        outcome
    } else {
        "the unstubbable enum was served"
    }
}

/// The control: every variant spellable, so the same shape mounts and the
/// consumer's reference reaches the live declaration.
function served_variant() -> string {
    let color = reflect.enum.new("Color", ["Red", "Green"]);
    let base = reflect.Package.compile({ "lib.baml": `function noop() -> int throws never { 0 }` });
    let app = base.with_types({ "Color": color });
    let consumer = reflect.Package.compile(
        { "main.baml": `function go() -> unknown throws never { app.Color.Red }` },
        packages = { "app": app },
    );
    let go = consumer.get_function<() -> unknown>("root.go") ?? throw "the consumer has no `go`";
    go().to_string()
}

test "mount_refuses_an_unstubbable_enum_variant" {
    assert.equal(refused_variant(), "E_RUNTIME_INTERFACE");
    assert.equal(served_variant(), "Red")
}
