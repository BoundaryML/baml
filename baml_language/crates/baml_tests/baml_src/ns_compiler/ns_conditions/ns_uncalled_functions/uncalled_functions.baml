function diagnostics(source: string) -> reflect.Diagnostic[] throws never {
    root.compiler.CompileSuccessDiagnostics({ "main.baml": source })
}

function function_warning(
    source: string,
    name: string,
    replacement: string?,
) -> reflect.Diagnostic throws never {
    let warnings = diagnostics(source);
    assert.equal(warnings.length(), 1);
    let warning = warnings[0];
    assert.equal(warning.code, "E0167");
    assert.equal(warning.severity, "warning");
    assert.equal(warning.phase, "type");
    assert.equal(warning.headline, "function `" + name + "` is always truthy");
    let primary = warning.annotations.find((annotation) -> {
        annotation.is_primary
    })
        ?? baml.sys.panic("missing primary annotation");
    // These fixtures are ASCII, so byte offsets are also string indices.
    assert.equal(source.slice(primary.span.start, primary.span.end), name);
    let label = primary.message ?? baml.sys.panic("missing call hint");
    assert.is_true(label.includes("did you mean to call this function?"));
    if (replacement != null) {
        assert.is_true(label.includes("Replace with `" + replacement + "`"));
    } else {
        assert.is_true(!label.includes("Replace with"));
    }
    warning
}

test "named function labels the reference and unreachable else" {
    let source = `
        function condition() -> bool { false }
        function main() -> string {
            if (condition) { "taken" } else { "unreachable" }
        }
    `;
    let warning = function_warning(source, "condition", "condition()");
    let secondary = warning.annotations.find((annotation) -> {
        !annotation.is_primary
    })
        ?? baml.sys.panic("missing unreachable branch annotation");
    assert.equal(secondary.message, "this branch is unreachable");
    let branch = source.slice(secondary.span.start, secondary.span.end);
    assert.is_true(branch.includes("unreachable"));
    assert.is_true(!branch.includes("taken"));
    assert.equal(warning.related_info.length(), 0)
}

test "function values get the call hint in every condition position" {
    for (
        let condition in [
            "if (predicate) { 1 } else { 0 }",
            "{ while (predicate) { break; } 0 }",
            "if (predicate && true) { 1 } else { 0 }",
            "if (false || predicate) { 1 } else { 0 }",
            "if (!predicate) { 1 } else { 0 }",
            "match (1) { let n if predicate => n, _ => 0 }",
        ]
    ) {
        let source = `function main(predicate: () -> bool throws never) -> int { ${condition} }`;
        function_warning(source, "predicate", "predicate()");
    }
}

test "aliases closures and function fields get the call hint" {
    for (let setup in ["let predicate = condition;", "let predicate = () -> bool { false };"]) {
        let source = `
            function condition() -> bool { false }
            function main() -> int { ${setup} if (predicate) { 1 } else { 0 } }
        `;
        function_warning(source, "predicate", "predicate()");
    }
    function_warning(
        `
            class Holder { predicate: () -> bool throws never }
            function main(holder: Holder) -> int { if (holder.predicate) { 1 } else { 0 } }
        `,
        "holder.predicate",
        "holder.predicate()",
    );
}

test "deferred generic function value gets the same diagnostic" {
    let warning = function_warning(
        `
            function identity<T>(value: T) -> T { value }
            function condition() -> bool { false }
            function main() -> int { if (identity(condition)) { 1 } else { 0 } }
        `,
        "identity(condition)",
        "identity(condition)()",
    );
    assert.equal(warning.annotations.length(), 2)
}

test "required arguments do not get an invalid empty call suggestion" {
    function_warning(
        `
            function condition(value: int) -> bool { value > 0 }
            function main() -> int { if (condition) { 1 } else { 0 } }
        `,
        "condition",
        null,
    );
    function_warning(
        `
            function condition(value: int = 1) -> bool { value > 0 }
            function main() -> int { if (condition) { 1 } else { 0 } }
        `,
        "condition",
        "condition()",
    );
}

test "called and nullable functions do not warn" {
    assert.equal(
        diagnostics(
            `
                function condition() -> bool { false }
                function called() -> int { if (condition()) { 1 } else { 0 } }
                function nullable(predicate: (() -> bool throws never)?) -> int {
                    if (predicate) { if (predicate()) { 1 } else { 2 } } else { 0 }
                }
                function explicit_check(predicate: (() -> bool throws never)?) -> bool {
                    predicate != null
                }
            `,
        )
            .length(),
        0,
    )
}

test "non function conditions keep the existing warning" {
    let warnings = diagnostics(
        `
            class Marker {}
            function main(marker: Marker) -> int { if (marker) { 1 } else { 0 } }
        `,
    );
    assert.equal(warnings.length(), 1);
    assert.is_true(warnings[0].message.starts_with("this condition is always truthy:"));
    assert.is_true(!warnings[0].message.includes("call"))
}

test "shared nullable fields are not treated as stable function values" {
    assert.equal(
        diagnostics(
            `
                class Holder { predicate: (() -> bool throws never)? }
                function condition() -> bool { false }
                function main(holder: Holder) -> int {
                    holder.predicate = condition;
                    let worker = spawn { holder.predicate = null; };
                    let result = if (holder.predicate) { 1 } else { 0 };
                    await worker;
                    result
                }
            `,
        )
            .length(),
        0,
    )
}

test "warning preserves function truthiness and call behavior" {
    let package = reflect.Package.compile(
        {
            "main.baml": `
                function condition() -> bool { false }
                function main() -> int {
                    let reference = if (condition) { 10 } else { 0 };
                    let called = if (condition()) { 1 } else { 2 };
                    reference + called
                }
            `,
        },
    );
    let main = package.get_function<() -> int>("root.main") ?? baml.sys.panic("missing main");
    assert.equal(main(), 12)
}
