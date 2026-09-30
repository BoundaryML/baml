// `reflect.Package.current()` inside a runtime-compiled package is that
// package: the call site's owner, read off the running function, never the
// static package ordinal the instruction carries.

test "current package inside a compiled package is the compiled package" {
    let pkg = reflect.Package.compile({
        "own.baml": `
            class Own { id int }

            function names() -> string {
                reflect.Package.current().classes().keys().join("|")
            }

            function via_closure() -> string {
                let f = () -> { reflect.Package.current().classes().keys().join("|") };
                f()
            }
        `,
    });
    let names = pkg.get_function<() -> string>("root.names") ?? throw "missing names";
    let via_closure = pkg.get_function<() -> string>("root.via_closure") ?? throw "missing via_closure";
    assert.equal(names(), "root.Own");
    assert.equal(via_closure(), "root.Own");
    // The static caller's own package is a different one.
    assert.is_true(reflect.Package.current().classes().keys().join("|") != "root.Own")
}
