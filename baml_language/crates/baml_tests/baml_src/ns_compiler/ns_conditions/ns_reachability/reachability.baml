function diagnostics(source: string) -> reflect.Diagnostic[] throws never {
    root.compiler.CompileSuccessDiagnostics({ "main.baml": source })
}

function unreachable_branch(source: string, condition: string) -> void throws never {
    let warnings = diagnostics(source);
    assert.equal(warnings.length(), 1);
    let warning = warnings[0];
    assert.equal(warning.code, "E0167");
    assert.equal(warning.severity, "warning");
    assert.equal(warning.phase, "type");
    let primary = warning.annotations.find((annotation) -> {
        annotation.is_primary
    })
        ?? baml.sys.panic("missing condition annotation");
    assert.equal(source.slice(primary.span.start, primary.span.end), condition);
    let branch = warning.annotations.find((annotation) -> {
        !annotation.is_primary
    })
        ?? baml.sys.panic("missing unreachable branch annotation");
    assert.equal(branch.message, "this branch is unreachable");
    assert.is_true(source.slice(branch.span.start, branch.span.end).includes("unreachable"));
}

function unreachable_statement(source: string) -> void throws never {
    let warnings = diagnostics(source).filter((warning) -> {
        warning.code == "E0146"
    });
    assert.equal(warnings.length(), 1);
    assert.equal(warnings[0].severity, "warning");
    let primary = warnings[0].annotations.find((annotation) -> {
        annotation.is_primary
    })
        ?? baml.sys.panic("missing unreachable statement annotation");
    assert.is_true(source.slice(primary.span.start, primary.span.end).includes("unreachable"));
}

test "truthy literals identify the unreachable else branch" {
    for (let condition in ["true", "1", "\"nonempty\""]) {
        unreachable_branch(
            `function main() -> string { if (${condition}) { "taken" } else { "unreachable" } }`,
            condition,
        );
    }
}

test "falsy literals identify the unreachable then branch" {
    for (let condition in ["false", "null", "0", "\"\""]) {
        unreachable_branch(
            `function main() -> string { if (${condition}) { "unreachable" } else { "taken" } }`,
            condition,
        );
    }
}

test "class values identify the unreachable else branch" {
    unreachable_branch(
        `
            class Marker {}
            function main(marker: Marker) -> string {
                if (marker) { "taken" } else { "unreachable" }
            }
        `,
        "marker",
    )
}

test "narrowed local types identify unreachable branches" {
    unreachable_branch(
        `
            class Marker {}
            function main(marker: Marker?) -> string {
                if (marker != null) {
                    if (marker) { "taken" } else { "unreachable" }
                } else { "missing" }
            }
        `,
        "marker",
    )
}

test "always truthy loop conditions warn except explicit unconditional loops" {
    let warnings = diagnostics(`function main(flag: bool) -> int { while (flag || true) { break; } 0 }`);
    assert.equal(warnings.length(), 1);
    assert.equal(warnings[0].code, "E0167");
    assert.is_true(warnings[0].message.includes("always truthy"));
    assert.equal(diagnostics(`function main() -> int { while (true) { break; } 0 }`).length(), 0)
}

test "always truthy conditions warn even without an else branch" {
    let source = `function main() -> void { if (true) {} }`;
    let warnings = diagnostics(source);
    assert.equal(warnings.length(), 1);
    assert.equal(warnings[0].code, "E0167");
    assert.equal(warnings[0].severity, "warning");
    let primary = warnings[0].annotations.find((annotation) -> {
        annotation.is_primary
    })
        ?? baml.sys.panic("missing condition annotation");
    assert.equal(source.slice(primary.span.start, primary.span.end), "true")
}

test "constant match guards warn and label impossible arms" {
    let source = `
        function main(value: int) -> string {
            match (value) { let n if false => "unreachable", _ => "taken" }
        }
    `;
    let warnings = diagnostics(source);
    assert.equal(warnings.length(), 1);
    assert.equal(warnings[0].code, "E0167");
    let arm = warnings[0].annotations.find((annotation) -> {
        !annotation.is_primary
    })
        ?? baml.sys.panic("missing unreachable match arm annotation");
    assert.equal(arm.message, "this match arm is unreachable");
    assert.is_true(source.slice(arm.span.start, arm.span.end).includes("unreachable"));
    let truthy = diagnostics(
        `
            function main(value: int) -> string {
                match (value) { let n if true => "taken", _ => "fallback" }
            }
        `,
    );
    assert.is_true(
        truthy.some((warning) -> {
            warning.code == "E0167"
        }),
    );
}

test "compound boolean conditions identify unreachable branches" {
    for (let condition in ["!true", "flag && false", "!(flag || true)"]) {
        let source = `function main(flag: bool) -> string { if (${condition}) { "unreachable" } else { "taken" } }`;
        let warnings = diagnostics(source);
        let warning = warnings.find((warning) -> {
            warning.annotations.some((annotation) -> {
                annotation.message == "this branch is unreachable"
            })
        })
            ?? baml.sys.panic("missing unreachable branch warning");
        assert.equal(warning.code, "E0167");
        assert.equal(warning.severity, "warning");
    }
}

test "constant comparisons identify unreachable branches" {
    for (
        let condition in [
            "1 == 1",
            "1 < 2",
            "2 >= 1",
            "1n < 2n",
            "1.0 == 1.00",
            "\"a\" != \"b\"",
            "null == null",
        ]
    ) {
        unreachable_branch(
            `function main() -> string { if (${condition}) { "taken" } else { "unreachable" } }`,
            condition,
        );
    }
    for (let condition in ["1 != 1", "true == false"]) {
        unreachable_branch(
            `function main() -> string { if (${condition}) { "unreachable" } else { "taken" } }`,
            condition,
        );
    }
}

test "false while conditions identify the unreachable loop body" {
    let source = `function main() -> int { while (false) { let unreachable = 1; } 0 }`;
    let warnings = diagnostics(source);
    assert.equal(warnings.length(), 1);
    assert.equal(warnings[0].code, "E0167");
    let body = warnings[0].annotations.find((annotation) -> {
        !annotation.is_primary
    })
        ?? baml.sys.panic("missing unreachable loop annotation");
    assert.equal(body.message, "this loop body is unreachable");
    assert.is_true(source.slice(body.span.start, body.span.end).includes("unreachable"))
}

test "short circuiting identifies the unevaluated operand" {
    for (let condition in ["true || predicate()", "false && predicate()"]) {
        let warnings = diagnostics(
            `
                function predicate() -> bool { false }
                function main() -> bool { ${condition} }
            `,
        );
        assert.equal(warnings.length(), 1);
        assert.equal(warnings[0].code, "E0167");
        assert.is_true(
            warnings[0].annotations.some((annotation) -> {
                annotation.message == "this expression is unreachable"
            }),
        );
    }
}

test "return throw and never returning calls identify unreachable statements" {
    for (let exit in ["return 1;", "throw \"boom\";", "baml.sys.panic(\"boom\");"]) {
        unreachable_statement(`function main() -> int { ${exit} let unreachable = 2; unreachable }`);
    }
}

test "exiting both branches makes subsequent code unreachable" {
    unreachable_statement(
        `
            function main(flag: bool) -> int {
                if (flag) { return 1; } else { return 2; }
                let unreachable = 3;
                unreachable
            }
        `,
    )
}

test "break and continue identify unreachable loop statements" {
    for (let exit in ["break;", "continue;"]) {
        unreachable_statement(
            `
                function main(flag: bool) -> int {
                    while (flag) { ${exit} let unreachable = 3; }
                    0
                }
            `,
        );
    }
}

test "infinite loops make subsequent code unreachable" {
    unreachable_statement(
        `
            function main() -> int {
                while (true) {}
                let unreachable = 3;
                unreachable
            }
        `,
    )
}

test "dynamic conditions and conditional exits remain reachable" {
    assert.equal(
        diagnostics(
            `
                function boolean(flag: bool) -> int { if (flag) { 1 } else { 0 } }
                function scalar(value: int) -> int { if (value) { 1 } else { 0 } }
                function nullable(value: string?) -> int { if (value) { 1 } else { 0 } }
                function conditional_exit(flag: bool) -> int { if (flag) { return 1; } 2 }
                function conditional_panic(flag: bool) -> int { flag && baml.sys.panic("boom"); 2 }
                function breakable_loop(flag: bool) -> int { while (true) { if (flag) { break; } } 2 }
                function conditional_loop(flag: bool) -> int { while (flag) {} 2 }
            `,
        )
            .length(),
        0,
    )
}

test "creating closures and spawned work does not exit the enclosing function" {
    assert.equal(
        diagnostics(
            `
                function main() -> int {
                    let callback = () -> int { return 1; };
                    let worker = spawn { throw "boom"; };
                    callback()
                }
            `,
        )
            .length(),
        0,
    )
}

test "loop mutation and shared field writes do not prove constant conditions" {
    for (
        let source in [
            `function main() -> int { let flag = true; while (flag) { flag = false; } 7 }`,
            `function main() -> int { let flag = true; flag = true; while (flag) { flag = false; } 7 }`,
            `
                class Holder { value: Holder? }
                function main(holder: Holder) -> int {
                    holder.value = holder;
                    let worker = spawn { holder.value = null; };
                    await worker;
                    if (holder.value) { 1 } else { 0 }
                }
            `,
        ]
    ) {
        assert.equal(diagnostics(source).length(), 0);
    }
}
