class PublicSuccessCase {
    name: string,
    expression: string,
    given: baml.json.json,
    expected: baml.json.json,
}

class PublicErrorCase {
    name: string,
    expression: string,
    given: baml.json.json,
    expected: JmesPathErrorKind,
}

function public_success_cases() -> PublicSuccessCase[] {
    [
        PublicSuccessCase {
            name: "field",
            expression: "foo.bar",
            given: baml.json.parse(`{"foo":{"bar":"baz"}}`),
            expected: "baz",
        },
        PublicSuccessCase {
            name: "projection",
            expression: "people[*].name",
            given: baml.json.parse(`{"people":[{"name":"Ada"},{"name":"Grace"}]}`),
            expected: baml.json.parse(`["Ada","Grace"]`),
        },
        PublicSuccessCase {
            name: "filter and multiselect hash",
            expression: "people[?age >= `30`].{name: name, age: age}",
            given: baml.json.parse(`{"people":[{"name":"Ada","age":36},{"name":"Linus","age":24}]}`),
            expected: baml.json.parse(`[{"name":"Ada","age":36}]`),
        },
        PublicSuccessCase {
            name: "function pipe",
            expression: "numbers | sort(@) | reverse(@)",
            given: baml.json.parse(`{"numbers":[3,1,2]}`),
            expected: baml.json.parse(`[3,2,1]`),
        },
        PublicSuccessCase {
            name: "missing field is null",
            expression: "missing",
            given: baml.json.parse(`{"present":true}`),
            expected: null,
        },
    ]
}

function public_error_cases() -> PublicErrorCase[] {
    [
        PublicErrorCase {
            name: "syntax",
            expression: "foo.",
            given: null,
            expected: JmesPathErrorKind.Syntax,
        },
        PublicErrorCase {
            name: "invalid arity",
            expression: "length(@, @)",
            given: [],
            expected: JmesPathErrorKind.InvalidArity,
        },
        PublicErrorCase {
            name: "invalid type",
            expression: "abs(@)",
            given: true,
            expected: JmesPathErrorKind.InvalidType,
        },
        PublicErrorCase {
            name: "invalid value",
            expression: "[::0]",
            given: [],
            expected: JmesPathErrorKind.InvalidValue,
        },
        PublicErrorCase {
            name: "unknown function",
            expression: "no_such_function(@)",
            given: null,
            expected: JmesPathErrorKind.UnknownFunction,
        },
    ]
}

function assert_public_success(c: PublicSuccessCase) -> null {
    match (jmespath_search(c.expression, InlineJsonSource { value: c.given })) {
        JmesPathSuccess { value: let actual } => assert.equal(actual, c.expected),
        let failure: JmesPathFailure => {
            baml.sys.panic(`expected success, got ${failure.kind}: ${failure.message}`)
        },
    }
}

function assert_public_error(c: PublicErrorCase) -> null {
    match (jmespath_search(c.expression, InlineJsonSource { value: c.given })) {
        let success: JmesPathSuccess => {
            baml.sys.panic(`expected ${c.expected}, got success ${success.value}`)
        },
        JmesPathFailure { kind: let actual } => assert.equal(actual, c.expected),
    }
}

testset "jmespath_public_success" {
    for (let c in public_success_cases()) {
        test c.name {
            assert_public_success(c)
        }
    }
}

testset "jmespath_public_errors" {
    for (let c in public_error_cases()) {
        test c.name {
            assert_public_error(c)
        }
    }
}

test "file JSON source" {
    match (jmespath_search("people[0].name", FileJsonSource { path: "testdata/public-input.json" })) {
        JmesPathSuccess { value: let actual } => assert.equal(actual, "Ada"),
        let failure: JmesPathFailure => {
            baml.sys.panic(`expected success, got ${failure.kind}: ${failure.message}`)
        },
    }
}
