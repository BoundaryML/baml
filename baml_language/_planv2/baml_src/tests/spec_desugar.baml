// Offline tests for the `@spec` desugar itself: what the compiler-built
// FunctionSpec contains, and the edge cases the desugar must not mangle.

// A shared toolbox produced by an expression (not a list literal) — the
// `tools` value is an arbitrary expression producing `ai.tools.Tool[]`.
function shared_tools() -> ai.tools.Tool[] {
    [ai.tools.tool(search_flights)]
}

// A parameter named `output_format` must stay visible to the template: the
// spec prompt closure's own parameter is internal (leading-space name) and
// `ctx.output_format` is the only schema placeholder.
function EchoFormat(output_format: string) -> string {
    client "openai/gpt-4o-mini"
    tools []
    prompt `
        Answer as ${output_format}.
        ${ctx.output_format}
    `
}

function NamedToolbox(q: string) -> string {
    client "openai/gpt-4o-mini"
    tools shared_tools()
    prompt `
        ${q}
        ${ctx.output_format}
    `
}

test "spec prompt keeps a user parameter named output_format" {
    let spec = EchoFormat@spec(output_format = "haiku");
    let text = spec.prompt().render_text("SCHEMA_TEXT");
    // the argument lands in the prompt; the runner-supplied schema text only
    // substitutes at ${ctx.output_format}
    assert.contains(text, "Answer as haiku.");
    assert.contains(text, "SCHEMA_TEXT")
}

test "empty tools opts a one-turn function into spec mode" {
    let spec = EchoFormat@spec(output_format = "prose");
    assert.equal(spec.name(), "EchoFormat");
    assert.is_true(spec.tools().is_empty())
}

test "tools accepts a named ai.tools.Tool[] expression" {
    let spec = NamedToolbox@spec(q = "find flights");
    assert.equal(spec.tools().list().length(), 1);
    let t = spec.tools().get("search_flights");
    assert.is_true(t != null)
}
