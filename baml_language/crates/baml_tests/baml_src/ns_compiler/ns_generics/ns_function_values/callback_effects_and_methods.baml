// `foo` invokes its callback, so the callback's concrete effect is forwarded
// through `foo` and must be admitted by its caller.
test "invoked generic callback effect propagates through wrapper" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                function fail_zero<T>() -> T throws string { throw "boom" }
                function foo(cb: () -> string) -> string { cb() }
                function demo() -> string throws string { foo(fail_zero) }
            `,
        },
    )
}

test "invoked callback effect is rejected by a closed caller" {
    root.compiler.AssertRejected(
        {
            "case.baml": `
                function fail_zero<T>() -> T throws string { throw "boom" }
                function foo(cb: () -> string) -> string { cb() }
                function demo() -> string throws never { foo(fail_zero) }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0096",
            message: "declared throws is `never`, but this function may also throw `string`",
        },
    )
}

// Merely receiving a callback does not propagate its effect. Calling it under
// a catch also closes the effect before returning from the wrapper.
test "unused and caught callback effects do not escape" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                function fail_zero<T>() -> T throws string { throw "boom" }
                function ignores(cb: () -> string) -> string { "ok" }
                function catches(cb: () -> string) -> string {
                    cb() catch_all (error) { _ => "fallback" }
                }
                function ignored() -> string throws never { ignores(fail_zero) }
                function caught() -> string throws never { catches(fail_zero) }
            `,
        },
    )
}

test "explicit throws never callback rejects a throwing generic function" {
    root.compiler.AssertRejected(
        {
            "case.baml": `
                function fail_zero<T>() -> T throws string { throw "boom" }
                function requires_never(cb: () -> string throws never) -> string { cb() }
                function demo() -> string throws never { requires_never(fail_zero) }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "mismatched types: expected `() -> string throws never`, found `() -> string throws string`",
        },
    )
}

test "generic method values follow the same realization rule" {
    root.compiler.AssertRejected(
        {
            "case.baml": `
                class Box {
                    function same<T>(self, value: T) -> T { value }
                }
                function demo(box: Box) -> string[] {
                    let copy = box.same;
                    let number = copy(7);
                    let word = copy("eight");
                    [number.to_string(), word]
                }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "generic function `same` needs concrete type arguments before it can be stored in `copy`. Specialize it explicitly, for example `box.same<int>`. Or write the concrete function type after the binding name: `let copy: (int) -> int throws never = box.same`. Calling `box.same(...)` directly works only when that call's arguments or expected result determine every type argument",
        },
    )
}

test "generic method value can specialize from callback context" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                type StringCallback = (string) -> string throws never
                class Box {
                    function same<T>(self, value: T) -> T { value }
                }
                function takes_string(cb: StringCallback) -> string { cb("ok") }
                function demo(box: Box) -> string { takes_string(box.same) }
            `,
        },
    )
}

// A qualified interface projection is callable with method type arguments,
// but the standalone value form cannot spell `<...>`. Its diagnostic must not
// recommend syntax the parser rejects; the concrete annotation is the fix.
test "qualified method value recommends its available annotation spelling" {
    root.compiler.AssertRejected(
        {
            "case.baml": `
                interface Embosser {
                    function emboss<U>(u: U) -> string throws never
                }
                class Plate<T> {
                    implements Embosser {
                        function emboss<U>(u: U) -> string throws never { "ok" }
                    }
                }
                function demo() -> string {
                    let stamp = (Plate<int> as Embosser).emboss;
                    stamp(7)
                }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "generic function `emboss` needs concrete type arguments before it can be stored in `stamp`. This reference form cannot take type arguments as a stored value; choose concrete types for U with a function type annotation instead. Or write the concrete function type after the binding name: `let stamp: (int) -> string throws never = (Plate<int> as Embosser).emboss`. Calling `(Plate<int> as Embosser).emboss(...)` directly works only when that call's arguments or expected result determine every type argument",
        },
    )
}

test "qualified method value compiles with a concrete annotation" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                interface Embosser {
                    function emboss<U>(u: U) -> string throws never
                }
                class Plate<T> {
                    implements Embosser {
                        function emboss<U>(u: U) -> string throws never { "ok" }
                    }
                }
                function demo() -> string {
                    let stamp: (int) -> string throws never = (Plate<int> as Embosser).emboss;
                    stamp(7)
                }
            `,
        },
    )
}
