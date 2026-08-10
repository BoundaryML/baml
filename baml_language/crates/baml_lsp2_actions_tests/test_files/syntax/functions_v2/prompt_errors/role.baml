// `${role(...)}` markers are removed from LLM function prompts; this fixture
// pins the migration error.
function Foo() -> string {
    client "openai/gpt-4o"
    prompt `
        You are a helpful assistant.
        ${ctx.output_format}

        ${role("assistant")}
        Hi
    `
}

//----
//- diagnostics
// E0010
//
//   × `${role(...)}` markers are removed; the prompt of `Foo` is instructions-only — the conversation lives in the journal, and message roles are the client's wire concern
//    ╭─[functions_v2_prompt_errors_role.baml:9:11]
//  9 │         ${role("assistant")}
//    ·           ────────┬────────
//    ·                   ╰── delete the role marker; write plain instructions
//    ╰────
// E0003
//
//   × unresolved name: `role`
//    ╭─[functions_v2_prompt_errors_role.baml:9:11]
//  9 │         ${role("assistant")}
//    ·           ────
//    ╰────
// E0096
//
//   × declared throws is `never`, but this function may also throw `unknown`
//     ╭─[functions_v2_prompt_errors_role.baml:5:12]
//   5 │ ╭─▶     prompt `
//   6 │ │           You are a helpful assistant.
//   7 │ │           ${ctx.output_format}
//   8 │ │
//   9 │ │           ${role("assistant")}
//  10 │ │           Hi
//  11 │ ╰─▶     `
//     ╰────
