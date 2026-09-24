function main() -> string {
    let item_type = reflect.class.new("Item", { "value": reflect.Type.of<string>() }).as_type();
    let app = reflect.Package.compile({ "empty.baml": "" }).with_types({ "Item": item_type });
    let compiled = reflect.Package.compile(
        {
            "wrong.baml": `
                function Run() -> int { app.Item { value: "x" } }
            `,
        },
        packages = { "app": app },
    ) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message,
        _ => "not a CompilationError",
    };
    if (compiled is string) {
        compiled
    } else {
        "the wrong program compiled"
    }
}

test "a_runtime_compile_diagnostic_names_a_mounted_runtime_class" {
    let diagnostic = main();
    assert.is_true(!diagnostic.includes("$dyn"));
    assert.equal(diagnostic, "mismatched types: expected `int`, found `app.Item`")
}
