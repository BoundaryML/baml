// The orthogonal basis for class-constructor completeness. Concrete non-nullable
// type kinds all share the same rule, so this suite tests semantic boundaries
// instead of enumerating every downstream manifestation of an invalid value.

class RejectedConstructorCase {
    name: string,
    source: string,
    diagnostic: root.compiler.ExactDiagnostic,
}

testset "required omissions" {
    let cases: RejectedConstructorCase[] = [
        RejectedConstructorCase {
            name: "aggregates fields in declaration order",
            source: `
                class Job { id: int, label: string, attempts: int }
                function run() -> Job { Job { id: 1 } }
            `,
            diagnostic: root.compiler.ExactDiagnostic {
                code: "E0001",
                message: "class `Job` is missing required fields: `label`, `attempts`",
            },
        },
        RejectedConstructorCase {
            name: "unknown requires an explicit value",
            source: `
                class Holder { value: unknown }
                function run() -> Holder { Holder {} }
            `,
            diagnostic: root.compiler.ExactDiagnostic {
                code: "E0001",
                message: "class `Holder` is missing required field: `value`",
            },
        },
        RejectedConstructorCase {
            name: "concrete generic argument is substituted",
            source: `
                class Box<T> { value: T }
                function run() -> Box<string> { Box<string> {} }
            `,
            diagnostic: root.compiler.ExactDiagnostic {
                code: "E0001",
                message: "class `Box` is missing required field: `value`",
            },
        },
        RejectedConstructorCase {
            name: "rigid generic parameter is not assumed nullable",
            source: `
                class Box<T> { value: T }
                function make<T>() -> Box<T> { Box<T> {} }
            `,
            diagnostic: root.compiler.ExactDiagnostic {
                code: "E0001",
                message: "class `Box` is missing required field: `value`",
            },
        },
    ];

    for (let case in cases) {
        test case.name {
            root.compiler.AssertRejected({ "case.baml": case.source }, case.diagnostic)
        }
    }
}

test "nullable union and null fields may be omitted" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                class Omittable { nullable: string?, unit: null }
                function run() -> Omittable { Omittable {} }
            `,
        },
    )
}

test "unknown accepts an explicit null" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                class Holder { value: unknown }
                function run() -> Holder { Holder { value: null } }
            `,
        },
    )
}

test "nullable alias is resolved before completeness checking" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                type MaybeText = string?
                class Holder { value: MaybeText }
                function run() -> Holder { Holder {} }
            `,
        },
    )
}

test "nullable generic argument is substituted" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                class Box<T> { value: T }
                function run() -> Box<string?> { Box<string?> {} }
            `,
        },
    )
}

test "complete spread supplies every field" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                class Job { id: int, label: string }
                function run(base: Job) -> Job { Job { ...base } }
            `,
        },
    )
}

test "mounted required field is rejected" {
    let dependency = root.compiler.CompileFixturePackage(
        {
            "dependency.baml": `
                class MountedRequiredFieldsFixture {
                    required: string,
                    nullable: string?,
                }
            `,
        },
    );
    root.compiler.AssertRejected(
        {
            "consumer.baml": `
                function run() -> dep.MountedRequiredFieldsFixture {
                    dep.MountedRequiredFieldsFixture {}
                }
            `,
        },
        root.compiler.ExactDiagnostic {
            code: "E0001",
            message: "class `dep.MountedRequiredFieldsFixture` is missing required field: `required`",
        },
        packages = { "dep": dependency },
    )
}

test "mounted nullable field may be omitted" {
    let dependency = root.compiler.CompileFixturePackage(
        {
            "dependency.baml": `
                class MountedRequiredFieldsFixture {
                    required: string,
                    nullable: string?,
                }
            `,
        },
    );
    root.compiler.AssertCompiles(
        {
            "consumer.baml": `
                function run() -> dep.MountedRequiredFieldsFixture {
                    dep.MountedRequiredFieldsFixture { required: "set" }
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
