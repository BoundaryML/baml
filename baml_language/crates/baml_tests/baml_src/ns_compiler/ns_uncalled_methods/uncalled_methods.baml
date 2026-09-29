// E0176: a receiver-bound method (`recv.m`, no call) used where a function
// value can't be what the author meant - interpolated, compared, or passed
// as `unknown`. Each would otherwise type-check and misbehave at runtime.

function diagnostics(source: string) -> reflect.Diagnostic[] throws never {
    root.compiler.CompileSuccessDiagnostics({ "main.baml": source })
}

function prelude() -> string throws never {
    `
        class Point {
            x: int
            function get(self) -> int { self.x }
            function add(self, d: int) -> int { self.x + d }
        }
        function take(value: unknown) -> string { "taken" }
    `
}

function method_warning(
    body: string,
    name: string,
    headline: string,
    replacement: string?,
) -> reflect.Diagnostic throws never {
    let source = prelude() + body;
    let warnings = diagnostics(source);
    assert.equal(warnings.length(), 1);
    let warning = warnings[0];
    assert.equal(warning.code, "E0176");
    assert.equal(warning.severity, "warning");
    assert.equal(warning.phase, "type");
    assert.equal(warning.headline, "method `" + name + "` " + headline);
    let primary = warning.annotations.find((annotation) -> {
        annotation.is_primary
    })
        ?? baml.sys.panic("missing primary annotation");
    // These fixtures are ASCII, so byte offsets are also string indices.
    assert.equal(source.slice(primary.span.start, primary.span.end), name);
    let label = primary.message ?? baml.sys.panic("missing call hint");
    assert.is_true(label.includes("did you mean to call this method?"));
    if (replacement != null) {
        assert.is_true(label.includes("Replace with `" + replacement + "`"));
    } else {
        assert.is_true(!label.includes("Replace with"));
    }
    warning
}

test "interpolated method warns" {
    method_warning(
        "function main(s: string) -> string { `len=${s.length}` }",
        "s.length",
        "is interpolated without being called",
        "s.length()",
    );
}

test "compared method replaces the always-false warning" {
    method_warning(
        "function main(s: string) -> bool { s.length == 3 }",
        "s.length",
        "is compared without being called",
        "s.length()",
    );
    method_warning(
        "function main(s: string) -> bool { 3 != s.length }",
        "s.length",
        "is compared without being called",
        "s.length()",
    );
}

test "compared method in a condition labels the unreachable branch" {
    let warning = method_warning(
        "function main(s: string) -> int { if (s.length == 3) { 1 } else { 2 } }",
        "s.length",
        "is compared without being called",
        "s.length()",
    );
    let secondary = warning.annotations.find((annotation) -> {
        !annotation.is_primary
    })
        ?? baml.sys.panic("missing unreachable branch annotation");
    assert.equal(secondary.message, "this branch is unreachable")
}

test "compared method inside a block still labels the unreachable branch" {
    let warning = method_warning(
        "function main(s: string) -> int { if (({ s.length }) == 3) { 1 } else { 2 } }",
        "s.length",
        "is compared without being called",
        "s.length()",
    );
    assert.equal(warning.annotations.length(), 2)
}

test "method flowing into unknown warns in every value position" {
    for (
        let body in [
            "function main(p: Point) -> string { take(p.get) }",
            "function main(p: Point) -> unknown { p.get }",
            "function main(p: Point) -> unknown { return p.get; }",
            "function main(p: Point) -> int { let v: unknown = p.get; 1 }",
            "function main(p: Point) -> unknown[] { [p.get] }",
            "type Any = unknown;\nfunction main(p: Point) -> Any { p.get }",
        ]
    ) {
        method_warning(body, "p.get", "is used as a value where `unknown` is expected", "p.get()");
    }
}

test "required arguments do not get an invalid empty call suggestion" {
    method_warning(
        "function main(p: Point) -> string { take(p.add) }",
        "p.add",
        "is used as a value where `unknown` is expected",
        null,
    );
}

test "deliberate function values do not warn" {
    assert.equal(
        diagnostics(
            prelude()
                + `
                    function apply(f: () -> int throws never) -> int { f() }
                    function callback(p: Point) -> int {
                        let f = p.get;
                        apply(p.get) + f()
                    }
                    function unbound() -> unknown { Point.get }
                    function free() -> unknown { take }
                    function called(p: Point, s: string) -> string {
                        take(p.get()) + take(s.length() == 3)
                    }
                `,
        )
            .length(),
        0,
    )
}
