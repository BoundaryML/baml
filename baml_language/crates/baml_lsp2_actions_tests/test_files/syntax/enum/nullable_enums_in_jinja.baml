// The Jinja enum-comparison warning surface is gone; this fixture now only
// checks that nullable enums and enum/string unions interpolate cleanly.
enum Status {
    Active
    Inactive @alias("inactive")
    Pending
}

enum Priority {
    High
    Medium @alias("med")
    Low
}

class MyInput {
    nullable_status Status?
    nullable_priority Priority?

    status_or_string (Status | string)?
    enum_with_string (Status | string | null)?
    complex_union (Status | Priority | string)?

    nullable_string string?
    int_or_enum (int | Status)?
    regular_status Status
    regular_string string
}

function TestNullableEnumComparisons(input: MyInput) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
    Status: ${input.nullable_status ?? "none"}
    Priority: ${input.nullable_priority ?? "none"}
    Union: ${input.status_or_string ?? "none"}

    ${ctx.output_format()}
    `
}

function TestCrossNullableEnumComparisons(input: MyInput, other_input: MyInput) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
    First: ${input.regular_status}
    Second: ${other_input.regular_status}

    ${ctx.output_format()}
    `
}

//----
//- diagnostics
// <no-diagnostics-expected>
