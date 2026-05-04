# BAML for agents

BAML is a typed, compiled language for defining LLM-powered and ML-powered functions as first-class primitives. Write a schema and a prompt; the compiler emits idiomatic client code in Python, TypeScript, Ruby, and Go with structured outputs, streaming, retries, and tests.

This file is intended for AI coding agents. It explains the product thesis first, then includes the practical BAML agent guide.

## Thesis

We want to make a language that agents are really good at writing. BAML files are small, explicit, and compiler-checkable. The prompt, schema, tests, provider, and generated client boundary are all in one place, so an agent does not have to infer the contract by chasing Python decorators, JSON schemas, hidden prompts, and parser code across a repo.

- Agents can edit one BAML function instead of four drift-prone files.
- The compiler gives fast feedback when a field, union arm, or return type is wrong.
- `baml describe` gives agents a semantic description of project and stdlib APIs before they guess.

```bash
$ baml describe TriageTicket

function TriageTicket(input: string) -> Ticket
client: GPT4o
input:
  input string
output:
  Ticket {
    priority "low" | "medium" | "high"
    summary string
  }
generated:
  TriageTicket$parse
  TriageTicket$render_prompt
  TriageTicket$build_request
```

We want to give people the right abstractions to build on top of their ML models: everything from inline comments that get stripped from your LLM prompts to support for Python and Typescript and making it easy to switch between ML service providers.

```baml
class Ticket {
  priority "low" | "medium" | "high"
  summary string
}

function TriageTicket(input: string) -> Ticket {
  client GPT4o
  prompt #"
    Classify this support ticket.
    {{ ctx.output_format }}

    Ticket: {{ input }}
  "#
}
```

We want to enable people to test the ML features and products that they're building, which is especially important when you're dealing with probabilistic systems and defining correctness is harder than enumerating edge cases!

- `baml-cli test` runs test blocks against real providers or recorded fixtures.
- `@@assert` constraints fail CI when an ML feature regresses.

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

We want it to be easy to deploy changes to your ML features: you should be able to both self-host everything that calls an OpenAI API and ask us to handle that for you, function-as-a-service style.

We want our users to be able to monitor their ML usage and ask questions about the precision and recall of their deployed model, about the costs of the current deployment, and about the reliability of the current deployment.

We want it to be straightforward to refine your ML usage, whether that means LLM prompt tuning, fine-tuning an existing open-source model, or training a special-purpose model from scratch.

And we think that the right way to do all this is to start with:

- a freely available, open-source schema language for your ML APIs,
- code generation for your LLM interactions, and
- robust, fast, easy-to-use tooling to support every step of the process.

For v1, that foundation is growing into a Turing-complete language: typed functions, tagged unions, `match`, loops, tests, a standard library, and a VM.

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

Importantly, this approach has a number of advantages compared to competitors in the space:

- We can offer our users a flexible, end-to-end platform. No one likes stitching together 10 products to build their workflow.
- We don't have lock-in: our schema language, compiler, and IDE integrations are all freely available and open-source, so if users want to use just those, they're more than welcome to.
- We can build our platform and ecosystem incrementally. Every platform suffers from the critical mass challenge - that you have to build out an entire platform for using it to be attractive, and then get enough adoption to accrue network effects - but everything that we want to build will be independently useful, so we'll be able to respond much more quickly to our users as we build out.
- We're not tied to LLMs: if the winds shift and the industry discovers new model architectures, hosting patterns, or whatnot, we'll be well-positioned to respond, because our value proposition is giving you the right abstractions for your ML APIs. We have a lot of special support for working with LLMs, because the existing general-purpose LLMs are wildly useful. But there's definitely some insanity to the fact that API calls in the LLM world can and do take multiple seconds.

## Links

- Docs: https://docs.boundaryml.com
- Quickstart: https://docs.boundaryml.com/guide/introduction/what-is-baml
- GitHub: https://github.com/BoundaryML/baml
- Discord: https://boundaryml.com/discord
- Full agent guide: https://boundaryml.com/agent/guide

## Detailed BAML Agent Guide

The full agent guide follows below.
