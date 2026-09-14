// Reflected equivalents of the two source-diagnostic tests from
// hir_ty_package_interface.rs. The dependency is compiled as a real mounted
// package so package arity and shadowing follow the same path as the Rust test.

function AppPackage() -> reflect.Package throws never {
    root.compiler.CompileFixturePackage(
        {
            "lib.baml": `
                interface Parent {
                    type Root = string
                }

                interface View<T> requires Parent {
                    type Item = T
                }

                enum Status { Active Retired }
                type Score = int
            `,
        },
    )
}

function ProjectDiagnostics(
    source: string,
    app: reflect.Package,
) -> reflect.Diagnostic[] throws never {
    root.compiler
        .CompileDiagnostics(
            { "ns_reflect/local.baml": `class Type<T> { value T }`, "main.baml": source },
            packages = { "app": app },
        )
        .filter((diagnostic) -> bool {
            diagnostic.severity == "error"
        })
}

test "bare_type_is_not_a_value_type_annotation" {
    let diagnostics = ProjectDiagnostics(`function removed(value: type) -> type { value }`, AppPackage());
    let count = 0;
    for (let diagnostic in diagnostics) {
        if (
            diagnostic.message.includes("`type` no longer names a runtime type value")
                && diagnostic.message.includes("write `reflect.Type` instead")
        ) {
            count += 1;
        }
    }
    if (count < 2) {
        baml.sys.panic(`expected at least two bare-type diagnostics, got ${diagnostics}`)
    }
}

test "mounted_type_validation_and_package_shadowing_are_fail_closed" {
    let app = AppPackage();
    root.compiler.AssertCompiles(
        {
            "ns_reflect/local.baml": `class Type<T> { value T }`,
            "main.baml": `
                function ok(
                    local: root.reflect.Type<int>,
                    view: app.View<int>,
                    status: app.Status,
                    score: app.Score,
                ) -> int throws never { 0 }

                function json_shorthand() -> string throws never {
                    json.stringify(null)
                }
            `,
        },
        packages = { "app": app },
    );
    root.compiler.AssertHasMessage(
        {
            "ns_reflect/local.baml": `class Type<T> { value T }`,
            "main.baml": `function shadowed() -> unknown throws never { reflect.Type.of<int>() }`,
        },
        "unresolved name: `of`",
        packages = { "app": app },
    );

    let diagnostics = ProjectDiagnostics(
        `
            function bad(
                missing: app.View,
                extra: app.View<int, string>,
                unknown_pin: app.View<int, Nope = string>,
                enum_args: app.Status<int>,
                alias_args: app.Score<int>,
            ) -> int throws never { 0 }
        `,
        app,
    );
    let type_argument_count = 0;
    let names_nope = false;
    for (let diagnostic in diagnostics) {
        if (diagnostic.message.includes("type argument")) {
            type_argument_count += 1;
        }
        if (diagnostic.message.includes("Nope")) {
            names_nope = true;
        }
    }
    if (type_argument_count < 4) {
        baml.sys.panic(`expected at least four type-argument diagnostics, got ${diagnostics}`)
    }
    assert.equal(names_nope, true)
}
