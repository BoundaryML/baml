// BEP spec mode: LLM functions with and without a `tools` field.
//
// PlainChat has NO tools field: an instructions-only backtick prompt with the
// `${ctx.output_format()}` hole.
//
// SpecAgent has a tools field: it gets the SpecAgent$spec companion, its
// direct call runs the ai Agent loop, and `tools` accepts both a literal
// list of function references and a named ai.Tool[] expression.

class Answer {
  text string
}

/// Doubles a number.
function double_it(n: int) -> int {
  n * 2
}

function more_tools() -> ai.tools.Tool[] {
  [ai.tools.tool(double_it, name = "double_named")]
}

function PlainChat(q: string) -> string {
  client: "openai/gpt-4o-mini"
  prompt: `
    Be terse.
    ${q}
    ${ctx.output_format()}
  `
}

function SpecAgent(q: string) -> Answer {
  client: "openai/gpt-4o-mini"
  tools: [double_it]
  prompt: `
    ${q}
    ${ctx.output_format()}
  `
}

function NamedTools(q: string) -> Answer {
  client: "anthropic/claude-haiku-4-5"
  tools: more_tools()
  prompt: `
    ${q}
    ${ctx.output_format()}
  `
}

function build_spec(q: string) -> ai.FunctionSpec<Answer> {
  SpecAgent@spec(q = q)
}
