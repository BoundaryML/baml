// The orthogonal basis for class-constructor completeness. Concrete non-nullable
// type kinds all share the same rule, so this suite tests semantic boundaries
// instead of enumerating every downstream manifestation of an invalid value.

test "aggregates omitted fields in declaration order" {
    root.compiler.AssertRejected(
        {
            "case.baml": `
                class Job { id: int, label: string, attempts: int }
                function run() -> Job { Job { id: 1 } }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "class `Job` is missing required fields: `label`, `attempts`",
        },
    )
}

test "unknown requires a value but accepts explicit null" {
    root.compiler.AssertRejected(
        {
            "omitted.baml": `
                class Holder { value: unknown }
                function run() -> Holder { Holder {} }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "class `Holder` is missing required field: `value`",
        },
    );
    root.compiler.AssertCompiles(
        {
            "explicit.baml": `
                class Holder { value: unknown }
                function run() -> Holder { Holder { value: null } }
            `,
        },
    )
}

// A broken declaration should fail for the declaration itself. Whether a
// constructor writes the poisoned slot is not a second, independently useful
// error: the compiler cannot know the slot's actual completeness rule until
// its type resolves.
test "an unresolved field type throws only its primary diagnostic" {
    root.compiler.AssertRejected(
        {
            "declaration.baml": `
                class Holder { value: MissingType }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0002",
            message: "unresolved type: MissingType",
        },
    );
    root.compiler.AssertRejected(
        {
            "omitted.baml": `
                class Holder { value: MissingType }
                function run() -> Holder { Holder {} }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0002",
            message: "unresolved type: MissingType",
        },
    );
    root.compiler.AssertRejected(
        {
            "explicit.baml": `
                class Holder { value: MissingType }
                function run(value: unknown) -> Holder { Holder { value } }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0002",
            message: "unresolved type: MissingType",
        },
    )
}

// Error recovery remains precise: a poisoned slot does not hide omissions
// whose declared types are valid and whose requirements are known.
test "an unresolved field type preserves genuine missing-field diagnostics" {
    root.compiler.AssertMultiRejected(
        {
            "case.baml": `
                class Holder { poisoned: MissingType, required: string }
                function run() -> Holder { Holder {} }
            `,
        },
        [
            root.compiler.ExactDiagnostic {
                code: "E0002",
                message: "unresolved type: MissingType",
            },
            root.compiler.ExactDiagnostic {
                code: "E0001",
                message: "class `Holder` is missing required field: `required`",
            },
        ],
    )
}

test "generic substitution controls whether omission is allowed" {
    root.compiler.AssertRejected(
        {
            "required.baml": `
                class Box<T> { value: T }
                function run() -> Box<string> { Box<string> {} }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "class `Box` is missing required field: `value`",
        },
    );
    root.compiler.AssertCompiles(
        {
            "nullable.baml": `
                class Box<T> { value: T }
                function run() -> Box<string?> { Box<string?> {} }
            `,
        },
    )
}

test "rigid generic parameters require explicit values" {
    root.compiler.AssertRejected(
        {
            "omitted.baml": `
                class Box<T> { value: T }
                function make<T>() -> Box<T> { Box<T> {} }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "class `Box` is missing required field: `value`",
        },
    );
    root.compiler.AssertCompiles(
        {
            "explicit.baml": `
                class Box<T> { value: T }
                function make<T>(value: T) -> Box<T> { Box<T> { value } }
            `,
        },
    )
}

test "alias resolution controls whether omission is allowed" {
    root.compiler.AssertRejected(
        {
            "required.baml": `
                type Text = string
                class Holder { value: Text }
                function run() -> Holder { Holder {} }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "class `Holder` is missing required field: `value`",
        },
    );
    root.compiler.AssertCompiles(
        {
            "nullable.baml": `
                type MaybeText = string?
                class Holder { value: MaybeText }
                function run() -> Holder { Holder {} }
            `,
        },
    )
}

test "only unions admitting null may be omitted" {
    root.compiler.AssertRejected(
        {
            "required.baml": `
                class Choice { value: string | int }
                function run() -> Choice { Choice {} }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "class `Choice` is missing required field: `value`",
        },
    );
    root.compiler.AssertCompiles(
        {
            "nullable.baml": `
                class Omittable { nullable: string?, unit: null }
                function run() -> Omittable { Omittable {} }
            `,
        },
    )
}

test "an exact class spread supplies omitted fields" {
    root.compiler.AssertRejected(
        {
            "partial.baml": `
                class Job { id: int, label: string }
                function run(base: Job) -> Job { Job { id: base.id } }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "class `Job` is missing required field: `label`",
        },
    );
    root.compiler.AssertCompiles(
        {
            "spread.baml": `
                class Job { id: int, label: string }
                function run(base: Job) -> Job { Job { ...base } }
            `,
        },
    )
}

test "mounted classes use the same completeness rule" {
    let dependency = root.compiler.CompileFixturePackage(
        {
            "dependency.baml": `
                class MountedFixture {
                    required: string,
                    nullable: string?,
                }
            `,
        },
    );
    root.compiler.AssertRejected(
        {
            "omitted.baml": `
                function run() -> dep.MountedFixture {
                    dep.MountedFixture {}
                }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "class `dep.MountedFixture` is missing required field: `required`",
        },
        packages = { "dep": dependency },
    );
    root.compiler.AssertCompiles(
        {
            "explicit.baml": `
                function run() -> dep.MountedFixture {
                    dep.MountedFixture { required: "set" }
                }
            `,
        },
        packages = { "dep": dependency },
    )
}

test "multiple rejected constructors match multiple diagnostics" {
    root.compiler.AssertMultiRejected(
        {
            "case.baml": `
                class First { value: string }
                class Second { count: int }
                function first() -> First { First {} }
                function second() -> Second { Second {} }
            `,
        },
        [
            root.compiler.ExactDiagnostic {
                code: "E0001",
                message: "class `First` is missing required field: `value`",
            },
            root.compiler.ExactDiagnostic {
                code: "E0001",
                message: "class `Second` is missing required field: `count`",
            },
        ],
    )
}
