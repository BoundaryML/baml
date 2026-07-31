# Structured outputs and tool calling

An `ai.AgentProvider` must turn each provider response into exactly one of:

```baml
class ModelStep<T> {
  outcome: T | ai.tools.ToolCalls,
  metadata: ai.ResponseMetadata,
  assistant_text: string?,
  reasoning_text: string?,
}
```

The provider decides how `T` and the application-tool schemas are represented
on its wire. The Agent does not know whether the provider used a result
function, a JSON response schema, or SAP text parsing. The two optional
display channels carry provider-neutral visible text when the vendor exposes
it — assistant preamble alongside the outcome, and displayable reasoning
text; the Agent republishes them as `AssistantTextEvent` and
`ReasoningEvent`.

## The decision matrix

There are two independent choices:

1. how the final `T` is represented;
2. how application tools are represented.

| Output mode | Tool mode | Final `T` | Application tools |
| --- | --- | --- | --- |
| Native/Strict | Native | Provider-native schema or reserved result function | Provider-native function/tool API |
| SAP | Native | Text parsed as `T` by SAP | Provider-native function/tool API |
| SAP | Prompt | Text parsed as `T`, or `T \| ToolCalls` when tools are present | Reflected schemas in `ctx.output_format` |

`ToolMode.Prompt` requires `OutputMode.Sap`. Public OpenAI, Anthropic, and
Google constructors select the mode; their prompt conversation classes are
private implementation details. Claude Code uses a separate schema-envelope
protocol described below.

Prompt mode adds the `ToolCalls` branch only when the active application-tool
roster is nonempty. A zero-tool step renders and parses plain `T`.

The defaults are native tool APIs:

```baml
openai.responses()
anthropic.messages()
google.vertex.gemini()
google.ai.gemini()
```

OpenAI defaults to a non-strict result function. Anthropic defaults to its
strict JSON output schema. Google uses controlled JSON when there are no
application tools and a reserved result function when native tools are
present.

## Output shape does not control tool concurrency

`T`, `T[]`, and `A | B` change the schema for one final result. They do not
change the number of application tool calls the model may request.

| Declared output | Result representation | Parallel application tools |
| --- | --- | --- |
| `Resolution` | one object schema | controlled by provider configuration |
| `Resolution[]` | one array schema | controlled by provider configuration |
| `Answer \| Escalation` | one union schema | controlled by provider configuration |

In particular, `T[]` does not mean `parallel_tool_calls = true`. Parallelism
is valid only for independent application calls in one provider turn. The
Agent correlates all returned IDs and may execute those calls before one
`submit`.

## OpenAI Responses

BEP-064 implements `POST /v1/responses`. It does not implement or fall back
to Chat Completions.

### Native result

Given:

```baml
class Resolution {
  category: string,
  summary: string,
  reply: string,
}

function ResolveTicket(ticket: SupportTicket) -> Resolution {
  provider: openai.responses()
  prompt: `
    Resolve ${ticket.id}.
    ${ctx.output_format}
  `
}
```

an Agent step sends one reserved Responses function:

```json
{
  "model": "gpt-5.6-luna",
  "input": [
    {
      "role": "user",
      "content": "Resolve T-100.\n..."
    }
  ],
  "tools": [
    {
      "type": "function",
      "name": "__baml_return_output",
      "description": "Return the final typed result. Call this only when the task is complete.",
      "parameters": {
        "type": "object",
        "properties": {
          "value": {
            "type": "object",
            "properties": {
              "category": { "type": "string" },
              "summary": { "type": "string" },
              "reply": { "type": "string" }
            },
            "required": ["category", "summary", "reply"]
          }
        },
        "required": ["value"],
        "additionalProperties": false
      },
      "strict": false
    }
  ],
  "store": true,
  "tool_choice": "required",
  "parallel_tool_calls": false
}
```

With no application tools, the reserved result function is the only required
choice.

The `value` wrapper is required because a function's parameter root must be an
object, while BAML permits any output type. Therefore:

- `Resolution[]` puts one array schema under `value`;
- `string` puts one string schema under `value`;
- `Answer | Escalation` puts one union schema under `value`;
- recursive types lift `$defs` to the parameter root.

Arrays and unions do not create several result functions.

In OpenAI `Native` and `Strict` output modes, application tools may not be
named `__baml_return_output`; construction fails before the request is sent.
SAP modes do not add that result function.

OpenAI may return:

```json
{
  "id": "resp_123",
  "status": "completed",
  "output": [
    {
      "type": "function_call",
      "call_id": "result_1",
      "name": "__baml_return_output",
      "arguments": "{\"value\":{\"category\":\"billing\",\"summary\":\"Duplicate charge\",\"reply\":\"We will investigate.\"}}"
    }
  ]
}
```

The adapter exact-decodes `.value` as `T`. Native function arguments are not
passed through SAP.

### Application tools

An ordinary BAML function is both the schema source and the implementation:

```baml
/// Search the support knowledge base.
function search_knowledge(query: string) -> json throws never {
  {
    "query": query,
    "article": "Duplicate charges are normally pending authorizations.",
  }
}
```

`ai.tools.tool(search_knowledge)` retains the function as
`baml.AnyFunction`. A handler may be ANY function — ordinary fallible
functions and LLM functions included (agent-as-tool); `invoke_tool` is the
total boundary that reifies a throw into a `ToolError` with the typed
original in `cause`. Reflection supplies the tool name, docstring, argument
names, argument types, and defaults:

```baml
let signature = reflect.signature(search_knowledge)
```

The OpenAI adapter serializes it as:

```json
{
  "type": "function",
  "name": "search_knowledge",
  "description": "Search the support knowledge base.",
  "parameters": {
    "type": "object",
    "properties": {
      "query": { "type": "string" }
    },
    "required": ["query"],
    "additionalProperties": false
  },
  "strict": false
}
```

In native mode, this function and `__baml_return_output` appear in the same
request. If OpenAI returns:

```json
{
  "type": "function_call",
  "call_id": "search_1",
  "name": "search_knowledge",
  "arguments": "{\"query\":\"duplicate charge\"}"
}
```

the provider returns `ToolCalls` from `step`. The Agent resolves the exact
tool name, validates arguments, and invokes the BAML function:

```baml
let value = reflect.call_any<json, never>(
  search_knowledge,
  { "query": "duplicate charge" },
)
```

The provider does not call `reflect.call_any`. After the Agent executes the
function, it calls `submit` with the correlated result. OpenAI records:

```json
{
  "previous_response_id": "resp_tools_1",
  "input": [
    {
      "type": "function_call_output",
      "call_id": "search_1",
      "output": "{\"query\":\"duplicate charge\",\"article\":\"Duplicate charges are normally pending authorizations.\"}"
    }
  ],
  "store": true
}
```

Every adapter serializes the submitted payload through
`ai.tools.result_payload`: a `ToolOk` contributes its JSON output, and a
`ToolError` contributes the canonical `{"error": message}` object. Each wire
format differs — Anthropic sets an `is_error` boolean, OpenAI stringifies the
function output — but the model sees the same error shape on every provider.
`ToolError.cause` — the typed original `ai.Failure` or
`baml.errors.UnknownError` — stays on the application channel for
`after_tool_call` callbacks and observers; it is never serialized to the
model.

The next Agent iteration calls `step` again. It may return more application
calls or the reserved result call.

### Strict mode

```baml
let provider = openai.responses(
  output_mode = openai.OutputMode.Strict,
)
```

Strict lowering:

- closes every object with `additionalProperties: false`;
- marks every property required while representing BAML optionals as nullable;
- recursively lowers nested arrays, unions, and definitions;
- rejects map-like objects and unconstrained `json`;
- applies the same strictness to application tools and the result function;
- disables parallel application tool calls.

### SAP modes

Hybrid SAP keeps native application tools:

```baml
let provider = openai.responses(
  output_mode = openai.OutputMode.Sap,
)
```

The request omits `__baml_return_output`. The result decision is:

```text
one or more function_call items → ToolCalls
no function_call               → parse output_text as T
```

Full prompt/SAP avoids the Responses function API:

```baml
let provider = openai.responses(
  output_mode = openai.OutputMode.Sap,
  tool_mode = openai.ToolMode.Prompt,
)
```

When application tools are present, the private prompt adapter re-renders
`ctx.output_format` for:

```text
T | ToolCalls
```

For `T = Answer | Escalation`, the combined contract is:

```text
(Answer | Escalation) | ToolCalls
```

The compiler first renders the schema for `(Answer | Escalation) |
ToolCalls` at `${ctx.output_format}`. The private adapter then appends this
protocol text and the reflected catalog:

```text
The output contract above permits either the final result or a ToolCalls value.
To use tools, return ToolCalls with one or more calls. Each call must have a
unique id, an exact tool name from the catalog, and args matching that tool's
parameters JSON Schema. Do not invent tool names. After tool results are
provided, choose tools again or return the final result.

Available tools:
[{"name":"search_knowledge","description":"Search the support knowledge base.","parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}}]
```

The HTTP request has that text in `input` and no `tools` or `tool_choice`.
SAP parses either the final union branch or `ToolCalls`. If it gets calls, the
Agent uses the same reflection-based dispatch as native mode, and the private
adapter adds the results to its prompt history before the next step.

With no application tools, prompt mode renders and parses plain `T`; it does
not add an unreachable `ToolCalls` branch.

Use literal discriminators for branches of a result union. A result branch
that structurally overlaps `{ calls: ... }` is ambiguous in prompt mode.

## Google Vertex and Google AI

The public adapters share Gemini `generateContent` semantics:

```baml
let vertex = google.vertex.gemini(
  project_id = "my-project",
  location = "us-central1",
)

let vertex_express = google.vertex.gemini(
  api_key = baml.env.get_or_panic("VERTEX_API_KEY"),
)

let ai_studio = google.ai.gemini(
  api_key = baml.env.get_or_panic("GOOGLE_API_KEY"),
)
```

Vertex accepts a credential file, inline credential JSON, or Application
Default Credentials. An API key selects Vertex Express Mode and cannot be
combined with project/location/ADC settings. Google AI uses
`x-goog-api-key`. Continuation requests re-authenticate without rebuilding the
provider-owned conversation body.

For a native Agent step with no application tools, `T` becomes controlled
JSON:

```json
{
  "generationConfig": {
    "responseMimeType": "application/json",
    "responseJsonSchema": {
      "type": "object",
      "properties": {
        "category": { "type": "string" },
        "summary": { "type": "string" },
        "reply": { "type": "string" }
      },
      "required": ["category", "summary", "reply"]
    }
  }
}
```

When native application tools are present, Gemini uses
`functionDeclarations`. The final `T` becomes the reserved
`{ value: T }` function so application calls and the final result share one
unambiguous function-calling turn:

```json
{
  "tools": [
    {
      "functionDeclarations": [
        {
          "name": "search_knowledge",
          "parametersJsonSchema": {
            "type": "object",
            "properties": {
              "query": { "type": "string" }
            },
            "required": ["query"]
          }
        },
        {
          "name": "__baml_return_output",
          "parametersJsonSchema": {
            "type": "object",
            "properties": {
              "value": { "...": "schema for T" }
            },
            "required": ["value"]
          }
        }
      ]
    }
  ],
  "toolConfig": {
    "functionCallingConfig": {
      "mode": "ANY"
    }
  }
}
```

Gemini reserves `__baml_return_output` in every output mode. An application
tool with that name is rejected even in SAP mode, so changing output modes
cannot change the meaning of an existing tool.

The adapter preserves complete model content, including thought signatures,
and correlates each `functionCall` with a `functionResponse`. Several
function-call parts mean parallel application calls; `T[]` remains one final
array.

`OutputMode.Sap + ToolMode.Native` is hybrid. With application tools,
`Sap + Prompt` uses the private prompt adapter and the same `T | ToolCalls`
contract described above.

## Anthropic Messages

Anthropic has two output modes:

```baml
let provider = anthropic.messages(
  output_mode = anthropic.OutputMode.Strict,
  tool_mode = anthropic.ToolMode.Native,
  strict_tool_inputs = true,
)
```

Strict `T` uses `output_config.format`:

```json
{
  "output_config": {
    "format": {
      "type": "json_schema",
      "schema": {
        "type": "object",
        "properties": {
          "category": { "type": "string" },
          "summary": { "type": "string" },
          "reply": { "type": "string" }
        },
        "required": ["category", "summary", "reply"],
        "additionalProperties": false
      }
    }
  }
}
```

Anthropic can combine this schema with native `tool_use` blocks. It does not
need a synthetic result function: tool-use blocks produce `ToolCalls`, while
normal text contains the final JSON `T`.

```json
{
  "tools": [
    {
      "name": "search_knowledge",
      "description": "Search the support knowledge base.",
      "input_schema": {
        "type": "object",
        "properties": {
          "query": { "type": "string" }
        },
        "required": ["query"],
        "additionalProperties": false
      },
      "strict": true
    }
  ],
  "tool_choice": {
    "type": "auto",
    "disable_parallel_tool_use": false
  }
}
```

The adapter retains the whole assistant content array, including thinking
signatures and tool-use blocks. `submit` appends all correlated
`tool_result` blocks in provider call order. Missing, duplicate, or unknown
IDs fail before state is mutated.

`OutputMode.Sap` parses final text as `T`. With application tools,
`Sap + Prompt` removes native tools and uses the private `T | ToolCalls`
adapter.

## Claude Code CLI

Claude Code does not expose `OutputMode`, `ToolMode`, or SAP parsing. With no
application tools, the CLI receives the JSON Schema for `T` directly. With
tools, its private adapter invokes the CLI with `--json-schema` for one
object-rooted envelope:

```json
{
  "type": "object",
  "properties": {
    "outcome": {
      "anyOf": [
        { "...": "schema for T" },
        { "...": "schema for ToolCalls" }
      ]
    }
  },
  "required": ["outcome"],
  "additionalProperties": false
}
```

Prompt text supplies the reflected application-tool catalog and explains the
choice, but the returned envelope is exact-decoded as `T | ToolCalls`; it is
not recovered with SAP. Claude Code's built-in CLI tools remain internal to
the CLI. Only BAML application calls in `ToolCalls` are dispatched by the
outer Agent.

## Rules shared by every adapter

- `begin` makes no model request.
- `step` makes exactly one model request.
- A replay-safe failed `step` leaves its `Conversation` unchanged.
- `submit` records correlated results and makes no model request.
- Providers never execute application functions.
- Native malformed tool arguments are not repaired by SAP.
- A provider does not silently switch output or tool modes after a protocol
  error.
- A turn cannot mix final output with application calls; that would create an
  ambiguous effect boundary.
- Saved conversation state contains continuation coordinates, not provider
  credentials.
- Media lowering belongs to the provider primitive: images and supported
  document/audio parts retain their structural representation.

Runnable request-shape and live integration coverage lives in:

```text
ns_ai_scenarios/01_tasks_and_providers/tests/
ns_ai_scenarios/02_tools_and_agents/tests/
ns_openai/ns_internal/responses/request_tests.baml
ns_anthropic/ns_internal/request_tests.baml
ns_google/ns_internal/request_tests.baml
```
