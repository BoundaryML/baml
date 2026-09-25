/// A mounted enum is served by identity, whatever its variants are called:
/// the consumer's compile reaches the declaration through the mount's
/// interface row and the load binds it through the mount, so a variant the
/// consumer cannot spell (`bad-variant` is no identifier) only means that
/// variant is unreachable from source. There is no stub whose spelling could
/// fail, so nothing refuses the enum.
function non_identifier_variant() -> string {
    let color = reflect.enum.new("Color", ["Red", "bad-variant"]);
    let base = reflect.Package.compile({ "lib.baml": `function noop() -> int throws never { 0 }` });
    let app = base.with_types({ "Color": color });
    let consumer = reflect.Package.compile(
        { "main.baml": `function go() -> unknown throws never { app.Color.Red }` },
        packages = { "app": app },
    );
    let go = consumer.get_function<() -> unknown>("root.go") ?? throw "the consumer has no `go`";
    go().to_string()
}

/// The control: every variant spellable, the same shape mounts the same way.
function identifier_variants() -> string {
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

test "mount_serves_an_enum_with_a_non_identifier_variant" {
    assert.equal(non_identifier_variant(), "Red");
    assert.equal(identifier_variants(), "Red")
}
