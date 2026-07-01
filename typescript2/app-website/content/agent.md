# BAML for agents

BAML is a typed, compiled language for defining LLM-powered and ML-powered functions as first-class primitives. Write a schema and a prompt; the compiler emits idiomatic client code in Python, TypeScript, Ruby, and Go with structured outputs, streaming, retries, and tests.

This file is intended for AI coding agents. It contains the product thesis and the commands to install BAML.

## Thesis

**We want to make a language that agents are really good at writing, and humans are really good at understanding.** The next wave of software will be edited by agents and reviewed by humans. That only works if the source is explicit, compact, compiler-checkable, and close to the model boundary it controls.

- Agents should edit one coherent program instead of chasing hidden prompts, JSON schemas, parser code, and client wrappers across a repo.
- Humans should be able to scan that same program and understand the types, prompts, providers, tests, and control flow without running the whole app in their head.
- The compiler should be the shared feedback loop for both of them: fast enough for agents to iterate, strict enough for people to trust.

**BAML started at the model boundary because that is where today's AI code breaks first.** Prompts, schemas, parsing, retries, tests, and generated clients tend to drift apart as soon as a demo becomes a product. BAML puts those pieces in one file and gives them language semantics instead of framework conventions.

```baml
class ToolCall {
  intent "answer" | "read_file" | "run_bash"
  confidence float
  args map<string, string>
}

function PickTool(history: string[]) -> ToolCall {
  client GPT4o
  prompt #"
    Choose the next tool for this agent loop.
    {{ ctx.output_format }}
    {{ _.role("user") }} {{ history }}
  "#
}

function should_continue(call: ToolCall) -> bool {
  call.intent != "answer" && call.confidence > 0.7
}
```

**For v1, that foundation is growing into a Turing-complete language.** Not just schemas. Typed functions, tagged unions, `match`, loops, tests, a standard library, and a VM. The goal is to let the ML-shaped part of an application become an actual program.

```baml
type Tool = Answer | ReadFile | RunBash

function dispatch(tool: Tool) -> string {
  match (tool) {
    a: Answer   => a.text,
    r: ReadFile => baml.fs.read(r.path),
    b: RunBash  => baml.sys.shell(b.command).stdout,
  }
}
```

**Tests have to live next to the behavior they protect.** AI systems are probabilistic, but the product contract still needs to be checked. BAML test blocks make the expected behavior visible to developers, CI, and coding agents.

```baml
test "refund-is-high-priority" {
  TriageTicket("I was charged twice and need a refund today")
}

testset "triage-regression" {
  test "password-reset" {
    let ticket = TriageTicket("I cannot reset my password")
    assert.eq(ticket.priority, "medium")
  }
}
```

**Provider choice, deployment shape, and observability should be language-level concerns.** If model calls are part of the program, switching providers, tracking cost and reliability, and deploying changes should not require a pile of disconnected glue code.

**The thesis is not that every AI app needs another framework.** It is that agent-authored software needs a source format that is precise enough for machines to edit and legible enough for humans to own. BAML is our attempt to make that format a real programming language.

## Installation

### Claude Code plugin (recommended)

Inside Claude Code, run:

```
/plugin marketplace add BoundaryML/baml-skill
/plugin install baml@boundaryml-baml
```

### Manual install

- **Python**: `uv add baml-py && uv run baml-cli init`
- **TypeScript**: `npm install @boundaryml/baml && npx baml-cli init`
- **Ruby**: `bundle add baml && bundle exec baml-cli init`
- **Go**: `go install github.com/boundaryml/baml/baml-cli@latest && baml-cli init`
- **Other languages**: https://docs.boundaryml.com/guide/installation-language/rest-api-other-languages

## Links

- Docs: https://docs.boundaryml.com
- Quickstart: https://docs.boundaryml.com/guide/introduction/what-is-baml
- GitHub: https://github.com/BoundaryML/baml
- Discord: https://boundaryml.com/discord
- Full agent guide: https://boundaryml.com/agent/guide
