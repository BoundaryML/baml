// The Jinja enum-comparison surface is gone; prompts are backtick templates
// that simply interpolate the enum values.
enum Status {
    Active @alias("active")
    Inactive @alias("inactive")
    Pending
}

enum Priority {
    High @alias("urgent")
    Low
}

function TestEnumJinjaSyntax(status: Status, priority: Priority, nullable_status: Status?) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        Status Display: ${status}
        Priority Display: ${priority}
        Nullable Display: ${nullable_status ?? "none"}

        ${ctx.output_format()}
    `
}

//----
//- diagnostics
// <no-diagnostics-expected>
