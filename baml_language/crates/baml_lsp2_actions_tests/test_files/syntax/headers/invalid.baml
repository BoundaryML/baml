// This file contains invalid MDX header usage patterns

function HeaderInPrompt() -> string {
    client: GPT4
    prompt: `
        //# This is inside a prompt
        It should be treated as literal text, not a header
    `
}

function MixedOnSameLine() -> string {
    let x = "hello" ## Inline Header // Should fail
    x
}

function HeaderWithoutSpace() -> string {
    //#No Space After Hash // Should potentially fail depending on grammar rules
    let x = "test"
    x
}

function EmptyHeader() -> string {
    //#
    // Empty header title - this should fail
    let x = "test"
    //##
    // Another empty header
    x
}

// Test headers in places they shouldn't parse
function HeaderInExpression() -> string {
    let x = "start" + # Middle Header // Should fail
        "end"
    x
}

function HeaderInStringLiteral() -> string {
    let x = "This string contains # Header Text"
    let y = #"
        Raw string with # Header
        Should not be parsed
    "#
    x + y
}

client GPT4 = openai.ResponsesClient.new(
    model = "gpt-4",
    api_key = env.OPENAI_API_KEY,
);

//----
//- diagnostics
// E0010
//
//   × unexpected token
//     ╭─[headers_invalid.baml:12:21]
//  12 │     let x = "hello" ## Inline Header // Should fail
//     ·                     ┬
//     ·                     ╰── expected `expression`, found `'#'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[headers_invalid.baml:12:22]
//  12 │     let x = "hello" ## Inline Header // Should fail
//     ·                      ┬
//     ·                      ╰── expected `expression`, found `'#'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[headers_invalid.baml:33:23]
//  33 │     let x = "start" + # Middle Header // Should fail
//     ·                       ┬
//     ·                       ╰── expected `expression`, found `'#'`
//     ╰────
// E0003
//
//   × unresolved name: `openai.ResponsesClient.new`
//     ╭─[headers_invalid.baml:47:15]
//  47 │ client GPT4 = openai.ResponsesClient.new(
//     ·               ───────────────────────
//     ╰────
