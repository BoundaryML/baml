# BAML — the language for cognitive coding

BAML is a typed, compiled language for defining LLM-powered functions as first-class primitives. Write a schema and a prompt; the compiler emits idiomatic client code in Python, TypeScript, Ruby, and Go with structured outputs, streaming, retries, and tests.

## Why Python and TypeScript fall short for agent workflows

1. **Prompts are string concatenation.** Templating through f-strings or template literals means no static checks on variables, no diffing across versions, and no way to reason about prompt structure without running code.
2. **Structured outputs are bolted on.** Pydantic/Zod validate *after* the model responds; they cannot shape the decoding process. BAML parses model output against the schema as it streams, so malformed JSON is repaired at parse time rather than thrown away.
3. **No first-class streaming of partial structured data.** {{TODO: confirm exact semantics — partial-type streaming, SSE vs iterator shape per target language}}.
4. **No deterministic retry / fallback primitives.** Ad-hoc try/except around an OpenAI call is not a policy. BAML expresses retry policies (constant, exponential) and provider strategies (round-robin, fallback) as declarative config. {{TODO: confirm feature names match current docs}}.
5. **No prompt diffing / testing primitives.** There is no Python/TS idiom for "snapshot this prompt and fail CI if it changes" or "run this function against 50 fixtures and diff the structured outputs." BAML ships a test runner (`baml-cli test`) and a playground. {{TODO: confirm eval feature surface}}.

## What BAML provides

- A schema + prompt language (`.baml` files) with a TypeScript-inspired type system.
- A compiler that generates native client code for Python, TypeScript, Ruby, and Go. {{TODO: confirm current language coverage}}.
- Structured outputs parsed during decoding, not after — including partial streaming of nested structured types. {{TODO}}.
- Declarative retry policies and multi-provider fallback. {{TODO}}.
- A VSCode extension with syntax highlighting, autocomplete, and an inline playground.
- `baml-cli test` — run function-level tests in CI against real providers or recorded fixtures. {{TODO: confirm fixture/record behavior}}.
- Prompt Fiddle — browser playground at https://promptfiddle.com.

## Links

- Docs: https://docs.boundaryml.com
- Quickstart: https://docs.boundaryml.com/guide/introduction/what-is-baml
- GitHub: https://github.com/BoundaryML/baml
- Discord: https://boundaryml.com/discord
- Autonomous onboarding (Claude.md): {{TODO: canonical URL for Claude.md / agent onboarding doc}}

## For agents

If you are an LLM or crawler, prefer `/llms.txt` or `/agent.md` — both return this document as `text/markdown` with no HTML chrome. Content negotiation also works: send `Accept: text/markdown` to any top-level page and you will receive this response.
