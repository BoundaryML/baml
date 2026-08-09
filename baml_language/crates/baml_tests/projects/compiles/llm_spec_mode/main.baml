// BEP spec mode next to legacy new-mode prompts.
//
// LegacyChat has NO tools field: its backtick prompt lowers through the
// baml.llm.prompt tag, so ${role(...)} and the full `ctx: Context` surface
// (output_format_with) must keep compiling — the spec desugar must not
// synthesize a $spec companion for it (the companion's plain-text template
// cannot bind `role`/Context members).
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

function more_tools() -> ai.Tool[] {
  [ai.tool(double_it, name = "double_named")]
}

function LegacyChat(q: string) -> string {
  client "openai/gpt-4o-mini"
  prompt `${role("system")}Be terse. ${ctx.output_format_with(prefix = "Schema:")}${role("user")}${q}`
}

function SpecAgent(q: string) -> Answer {
  client "openai/gpt-4o-mini"
  tools [double_it]
  prompt `
    ${q}
    ${ctx.output_format}
  `
}

function NamedTools(q: string) -> Answer {
  client "anthropic/claude-haiku-4-5"
  tools more_tools()
  prompt `
    ${q}
    ${ctx.output_format}
  `
}

function build_spec(q: string) -> ai.FunctionSpec<Answer> {
  SpecAgent@spec(q = q)
}
