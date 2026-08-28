// The main regression: a local binding is monomorphic. Later calls cannot
// independently specialize the same stored generic function value.
test "bare generic function binding reports both concrete fixes" {
    root.compiler.AssertRejected(
        {
            "case.baml": `
                function identity<T>(value: T) -> T { value }

                function polymorphic_value_examples() -> string[] {
                    let copy = identity;
                    let number = copy(7);
                    let word = copy("eight");
                    [number.to_string(), word]
                }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "generic function `identity` needs concrete type arguments before it can be stored in `copy`. Specialize it explicitly, for example `identity<int>`. Or write the concrete function type after the binding name: `let copy: (int) -> int throws never = identity`. Calling `identity(...)` directly works only when that call's arguments or expected result determine every type argument",
        },
    )
}

// Generic-parameter arity and ordinary function arity are independent. The
// specialization has two types while the concrete function type has 3 inputs.
test "multiple generic parameters produce an arity-correct diagnostic" {
    root.compiler.AssertRejected(
        {
            "case.baml": `
                function project<A, B>(first: A, second: B, fallback: B) -> B { second }
                function demo() -> bool {
                    let project_value = project;
                    true
                }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "generic function `project` needs concrete type arguments before it can be stored in `project_value`. Specialize it explicitly, for example `project<int, int>`. Or write the concrete function type after the binding name: `let project_value: (int, int, int) -> int throws never = project`. Calling `project(...)` directly works only when that call's arguments or expected result determine every type argument",
        },
    )
}

// A made-up `<int>` example can violate a declared bound. In that case the
// compiler describes the parameters and signature shape without pretending a
// particular specialization is valid.
test "bounded generic diagnostic does not invent invalid type arguments" {
    root.compiler.AssertRejected(
        {
            "case.baml": `
                function bounded<A, B extends baml.ops.Compare>(first: A, second: B) -> A { first }
                function demo() -> bool {
                    let bounded_value = bounded;
                    true
                }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "generic function `bounded` needs concrete type arguments before it can be stored in `bounded_value`. Specialize it explicitly by choosing concrete types for A, B; each type must satisfy its declared bounds. Or write the concrete function type after the binding name, using `(A, B) -> A throws never` as the shape and replacing A, B with the types you want: `let bounded_value: ... = bounded`. Calling `bounded(...)` directly works only when that call's arguments or expected result determine every type argument",
        },
    )
}

// A function annotation cannot infer a parameter that never appears in the
// function type. Only explicit specialization can choose this phantom T.
test "phantom generic explains why callback context is insufficient" {
    root.compiler.AssertRejected(
        {
            "case.baml": `
                function phantom<T>() -> string { "ok" }
                function requires_never(cb: () -> string throws never) -> string { cb() }
                function demo() -> string { requires_never(phantom) }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "cannot determine every type argument for generic function `phantom` from the expected function type. Specialize it explicitly, for example `phantom<int>`. Calling `phantom(...)` directly works only when that call's arguments or expected result determine every type argument",
        },
    )
}

// `reflect.AnyFunction` erases the concrete signature, so it cannot provide
// the type information needed to realize a generic function value.
test "erased reflection function type does not specialize a generic value" {
    root.compiler.AssertRejected(
        {
            "case.baml": `
                function ident<T>(x: T) -> T throws never { x }
                function demo() -> bool {
                    let f: reflect.AnyFunction = ident;
                    true
                }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "cannot determine every type argument for generic function `ident` from the expected function type. Specialize it explicitly, for example `ident<int>`. Or write the concrete function type after the binding name: `let f: (int) -> int throws never = ident`. Calling `ident(...)` directly works only when that call's arguments or expected result determine every type argument",
        },
    )
}

// Every expected diagnostic is matched against one actual diagnostic. This
// prevents message fragments from separate errors from jointly passing one
// assertion, and locks both diagnostic count and source order.
test "multiple bare bindings report distinct ordered diagnostics" {
    root.compiler.AssertMultiRejected(
        {
            "case.baml": `
                function identity<T>(value: T) -> T { value }
                function demo() -> bool {
                    let first = identity;
                    let second = identity;
                    true
                }
            `,
        },
        [
            root.compiler.ExactDiagnostic {
                code: "E0001",
                message: "generic function `identity` needs concrete type arguments before it can be stored in `first`. Specialize it explicitly, for example `identity<int>`. Or write the concrete function type after the binding name: `let first: (int) -> int throws never = identity`. Calling `identity(...)` directly works only when that call's arguments or expected result determine every type argument",
            },
            root.compiler.ExactDiagnostic {
                code: "E0001",
                message: "generic function `identity` needs concrete type arguments before it can be stored in `second`. Specialize it explicitly, for example `identity<int>`. Or write the concrete function type after the binding name: `let second: (int) -> int throws never = identity`. Calling `identity(...)` directly works only when that call's arguments or expected result determine every type argument",
            },
        ],
    )
}
