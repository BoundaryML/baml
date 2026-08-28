test "explicit specialization and concrete annotation compile" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                type StringCallback = (string) -> string throws never
                function identity<T>(value: T) -> T { value }

                function explicit() -> string {
                    let copy = identity<string>;
                    copy("ok")
                }

                function annotated() -> string {
                    let copy: StringCallback = identity;
                    copy("ok")
                }
            `,
        },
    )
}

test "direct calls may infer a fresh type on every call" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                function identity<T>(value: T) -> T { value }
                function direct_calls() -> string[] {
                    let number = identity(7);
                    let word = identity("eight");
                    [number.to_string(), word]
                }
            `,
        },
    )
}

// Context may come from any actual value slot: a callback argument, return,
// container element, object field, branch, default value, or optional arm.
test "concrete value contexts specialize a generic function" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                type StringCallback = (string) -> string throws never
                class CallbackHolder { cb StringCallback }

                function identity<T>(value: T) -> T { value }
                function takes_string(cb: StringCallback) -> string { cb("ok") }
                function takes_optional(cb: StringCallback?) -> string { "ok" }
                function with_default(cb: StringCallback = identity) -> string { cb("ok") }

                function as_argument() -> string { takes_string(identity) }
                function as_return() -> StringCallback throws never { identity }
                function as_array() -> StringCallback[] { [identity] }
                function as_map() -> map<string, StringCallback> { { "id": identity } }
                function as_field() -> CallbackHolder { CallbackHolder { cb: identity } }
                function as_branch(flag: bool) -> StringCallback throws never {
                    if (flag) { identity } else { identity }
                }
                function as_optional_arm() -> string { takes_optional(identity) }
            `,
        },
    )
}

// Inference can use a sibling argument in the same call, but it cannot travel
// backward from a later use and turn an existing local into a generic binding.
test "a later sibling call argument may provide context" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                function identity<T>(value: T) -> T { value }
                function apply<T>(cb: (T) -> T throws never, value: T) -> T { cb(value) }
                function demo() -> int { apply(identity, 1) }
            `,
        },
    )
}

test "a later use cannot retroactively specialize a stored value" {
    root.compiler.AssertRejected(
        {
            "case.baml": `
                type StringCallback = (string) -> string throws never
                function identity<T>(value: T) -> T { value }
                function takes_string(cb: StringCallback) -> string { cb("ok") }
                function demo() -> string {
                    let copy = identity;
                    takes_string(copy)
                }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "generic function `identity` needs concrete type arguments before it can be stored in `copy`. Specialize it explicitly, for example `identity<string>`. Or write the concrete function type after the binding name: `let copy: (string) -> string throws never = identity`. Calling `identity(...)` directly works only when that call's arguments or expected result determine every type argument",
        },
    )
}

// Both function arms are plausible, so a union does not provide one concrete
// function type to specialize against.
test "ambiguous callback union does not guess a specialization" {
    root.compiler.AssertRejected(
        {
            "case.baml": `
                type StringCallback = (string) -> string throws never
                type IntCallback = (int) -> int throws never
                function identity<T>(value: T) -> T { value }
                function accepts(cb: StringCallback | IntCallback) -> bool { true }
                function demo() -> bool { accepts(identity) }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "cannot determine every type argument for generic function `identity` from the expected function type. Specialize it explicitly, for example `identity<int>`. Bind it first with a concrete function type: `let function_value: (int) -> int throws never = identity`. Calling `identity(...)` directly works only when that call's arguments or expected result determine every type argument",
        },
    )
}
