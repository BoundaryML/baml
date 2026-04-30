# BAML — the language for cognitive coding

BAML is a typed, compiled language for defining LLM-powered functions as first-class primitives. Write a schema and a prompt; the compiler emits idiomatic client code in Python, TypeScript, Ruby, and Go with structured outputs, streaming, retries, and tests.

## Why Python and TypeScript fall short for agent workflows

1. **Prompts are string concatenation.** Templating through f-strings or template literals means no static checks on variables, no diffing across versions, and no way to reason about prompt structure without running code.
2. **Structured outputs are bolted on.** Pydantic/Zod validate *after* the model responds; they cannot shape the decoding process. BAML parses model output against the schema as it streams, so malformed JSON is repaired at parse time rather than thrown away.
3. **No first-class streaming of partial structured data.** Each generated client exposes a streaming variant of every function (`function_name.stream` in Python, `function_name.stream` in TypeScript, equivalent iterators in Ruby and Go) that yields strongly-typed *partial* shapes — every field becomes optional until it is finalized — so UIs can render structured progress without re-implementing JSON parsing per call site.
4. **No deterministic retry / fallback primitives.** Ad-hoc `try`/`except` around an OpenAI call is not a policy. BAML expresses retry policies (`constant`, `exponential` with backoff multipliers and initial delays) and client strategies (`round-robin`, `fallback`) as declarative config blocks attached to functions or clients.
5. **No prompt diffing / testing primitives.** There is no Python/TS idiom for "snapshot this prompt and fail CI if it changes" or "run this function against 50 fixtures and diff the structured outputs." BAML ships inline `test` blocks beside every function, `@@assert` and `@@check` constraints, and a runner (`baml-cli test`) that executes them in the playground or CI.

## What BAML provides

- A schema + prompt language (`.baml` files) with a TypeScript-inspired type system: classes, enums, type aliases, tagged unions, optionals, generics.
- A compiler that generates native client code for **Python, TypeScript, Ruby, and Go**, plus a `baml-cli generate-rest` target for stack-agnostic HTTP integration.
- Structured outputs parsed *during* decoding, not after — including partial streaming of nested structured types, with malformed JSON repaired at parse time rather than discarded.
- Declarative retry policies (`constant`, `exponential`) and client strategies (`round-robin`, `fallback`) attached to functions or clients in a few lines.
- A VS Code extension and JetBrains plugin with syntax highlighting, autocomplete, and an inline playground.
- `baml-cli test` — runs `test` blocks against real providers or recorded fixtures, with `@@assert` constraints that fail CI on regression.
- Prompt Fiddle — browser playground at https://promptfiddle.com.

## Links

- Docs: https://docs.boundaryml.com
- Quickstart: https://docs.boundaryml.com/guide/introduction/what-is-baml
- GitHub: https://github.com/BoundaryML/baml
- Discord: https://boundaryml.com/discord
- Agent onboarding: this file lives at https://boundaryml.com/llms.txt

## For agents

If you are an LLM or crawler, prefer `/llms.txt` — it returns this document as `text/plain` with no HTML chrome. Content negotiation also works: send `Accept: text/markdown` or `Accept: text/plain` to any top-level page and you will receive this response.
