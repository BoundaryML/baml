// `throws unknown` is an explicit open bound, not the spelling for inferred
// throws. These tests exercise the public compiler boundary so they lock the
// same diagnostics users receive from runtime compilation.

test "rejects an unknown bound when the body does not throw" {
    root.compiler.AssertRejected(
        {
            "case.baml": `
                function run() -> int throws unknown { 1 }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0097",
            message: "`throws unknown` is unnecessary: BAML infers thrown types automatically, and this function does not throw. Remove the declaration; write an explicit `throws` type only to bound what may escape",
        },
    )
}

test "rejects an unknown bound when inference finds a narrower type" {
    root.compiler.AssertRejected(
        {
            "case.baml": `
                function run() -> int throws unknown { throw 1 }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0097",
            message: "`throws unknown` is imprecise: this function only throws `int`. BAML infers thrown types automatically, so remove the declaration; write `throws int` only to explicitly bound what may escape",
        },
    )
}

test "a union containing unknown still requires an unknown throw" {
    root.compiler.AssertRejected(
        {
            "case.baml": `
                function run() -> int throws string | unknown { throw "boom" }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0097",
            message: "`throws unknown` is imprecise: this function only throws `string`. BAML infers thrown types automatically, so remove the declaration; write `throws string` only to explicitly bound what may escape",
        },
    )
}

// Ordinary explicit bounds may be wider than the inferred throws. The stricter
// precision rule applies specifically to an unused `unknown` member.
test "a concrete union bound may include an unused member" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                function run(mode: int) -> int throws int | string | bool {
                    if (mode == 0) {
                        throw 1
                    }
                    throw "boom"
                }
            `,
        },
    )
}

test "an escaping unknown satisfies direct and aliased bounds" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                type OpenErrors = unknown

                function direct(value: unknown) -> int throws unknown {
                    throw value
                }

                function aliased(value: unknown) -> int throws OpenErrors {
                    throw value
                }
            `,
        },
    )
}

test "signature and callback unknown boundaries compile" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                interface Service {
                    function run(self) -> null throws unknown
                }

                function plain_unknown(value: unknown) -> unknown {
                    value
                }

                function callback_boundary(
                    callback: () -> null throws unknown,
                ) -> null throws baml.errors.UnknownError {
                    callback() catch_all (error) {
                        _ => throw baml.errors.UnknownError.from<never>(error),
                    }
                }
            `,
        },
    )
}

test "unknown propagating through a call satisfies the bound" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                function source(value: unknown) -> int { throw value }

                function run(value: unknown) -> int throws unknown {
                    source(value)
                }
            `,
        },
    )
}

test "a mixed bound compiles when every member can escape" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                function run(mode: int, value: unknown) -> int throws string | unknown {
                    if (mode == 0) {
                        throw "boom"
                    }
                    throw value
                }
            `,
        },
    )
}

// The unknown member is satisfied, so an unused concrete member remains the
// ordinary E0097 warning rather than becoming the imprecise-bound error.
test "an unused concrete member does not invalidate a satisfied unknown bound" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                class UnusedError {}

                function run(value: unknown) -> int throws unknown | UnusedError {
                    throw value
                }
            `,
        },
    )
}
