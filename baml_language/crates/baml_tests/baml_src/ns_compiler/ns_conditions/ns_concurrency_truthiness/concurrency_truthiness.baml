test "interface conditions can contain falsy primitive values" {
    let package = reflect.Package.compile(
        {
            "main.baml": `
                interface Marker {}
                implements Marker for int {}
                function check(value: Marker) -> bool {
                    if (value) { true } else { false }
                }
            `,
        },
    );
    assert.equal(package.diagnostics().length(), 0);
    let check = package.get_function<(int) -> bool>("root.check") ?? baml.sys.panic("missing check");
    assert.is_true(!check(0));
    assert.is_true(check(1));
}

test "captured nullable locals remain runtime conditions" {
    for (
        let mutation in [
            "let worker = spawn { value = null; }; await worker;",
            "let change = () -> void { value = null; }; change();",
        ]
    ) {
        let diagnostics = root.compiler.CompileSuccessDiagnostics(
            {
                "main.baml": `
                    function predicate() -> bool { true }
                    function main() -> int {
                        let value: (() -> bool throws never)? = predicate;
                        ${mutation}
                        if (value) { 1 } else { 0 }
                    }
                `,
            },
        );
        assert.equal(diagnostics.length(), 0);
    }
}

test "shared collection entries remain runtime conditions after writes" {
    for (
        let source in [
            `
                function predicate() -> bool { true }
                function main(values: ((() -> bool throws never)?)[]) -> int {
                    values[0] = predicate;
                    let worker = spawn { values[0] = null; };
                    await worker;
                    if (values[0]) { 1 } else { 0 }
                }
            `,
            `
                function predicate() -> bool { true }
                function main(values: map<string, (() -> bool throws never)?>) -> int {
                    values["key"] = predicate;
                    let worker = spawn { values["key"] = null; };
                    await worker;
                    if (values["key"]) { 1 } else { 0 }
                }
            `,
        ]
    ) {
        assert.equal(root.compiler.CompileSuccessDiagnostics({ "main.baml": source }).length(), 0);
    }
}
