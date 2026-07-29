# Structured output and tools

Providers expose the same BAML contract—typed `T`, application tools, and
provider-owned conversation state—but use their actual wire protocols. This
page starts with the detailed OpenAI mapping, then describes the Google and
Anthropic differences.

The public provider namespaces are intentionally configuration-sized:

- `openai` contains the Responses and Realtime providers;
- `google.vertex` contains Gemini on Vertex AI;
- `google.ai` contains the Gemini API/Google AI provider;
- `anthropic` contains the Messages provider.

Request builders, wire envelopes, schema transforms, and concrete
conversation classes live in each provider's `internal` namespace.

| Provider | Typed `T` by default | Application tools | Text fallback |
| --- | --- | --- | --- |
| OpenAI Responses | reserved `__baml_return_output` function | Responses functions | `OutputMode.Sap` |
| Google Vertex / AI | `responseJsonSchema` for one-shot generation; reserved result function in an agent | Gemini `functionDeclarations` | `OutputMode.Sap` |
| Anthropic | `output_config.format` | Messages `tool_use` / `tool_result` | `OutputMode.Sap` |

## OpenAI Responses

BEP-064 uses `POST /v1/responses` only. Chat Completions is not a fallback.

The adapter puts two different concepts on OpenAI's function-tool wire format:

- an **application tool** asks the BAML runner to execute a BAML function;
- `__baml_return_output` returns the final value of type `T`.

An application call continues the agent loop. The reserved result call ends
it.

## Output modes

Native function calling is the default:

```baml
let provider = openai.responses()
```

The relevant configuration is:

```baml
openai.responses(
  output_mode = openai.OutputMode.Native,
  tool_mode = openai.ToolMode.Native,
  parallel_tool_calls = true,
)
```

The four supported behaviors are:

1. `Native + Native` (default): application tools and final `T` are Responses
   functions.
2. `Strict + Native`: the same protocol with OpenAI strict schemas.
3. `Sap + Native`: application tools remain native, but final `T` is
   `output_text` parsed by SAP. This is the hybrid mode.
4. `Sap + Prompt`: no OpenAI function tools. The model writes either `T` or
   `ToolCalls` as text.

`ToolMode.Prompt` requires `OutputMode.Sap`. The OpenAI provider selects the
prompt adapter internally; users do not construct a second provider wrapper.

`root.openai` is intentionally small. It exposes provider configuration:
`Responses`, `responses`, `OutputMode`, `ToolMode`, `Realtime`,
`RealtimeServerVad`, and `realtime_audio_format`. Wire models, request
builders, schema lowering, conversation state, jobs, and test inspection
helpers live under `root.openai.internal`.

## Example 1: `T` becomes an HTTP function

Given:

```baml
enum TicketPriority {
  Low
  Normal
  Urgent
}

class Resolution {
  category: string,
  priority: TicketPriority,
  summary: string,
  reply: string,
}

function ResolveTicket(ticket: SupportTicket) -> Resolution {
  client: "openai-responses/gpt-5.6-luna"
  prompt: `
    Resolve this support ticket.
    ${ctx.output_format}
  `
}
```

the provider reads `task.output_type()` and sends one flat Responses function:

```json
{
  "model": "gpt-5.6-luna",
  "input": [
    {
      "role": "user",
      "content": "Resolve this support ticket.\n..."
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
              "priority": {
                "type": "string",
                "enum": ["Low", "Normal", "Urgent"]
              },
              "summary": { "type": "string" },
              "reply": { "type": "string" }
            },
            "required": ["category", "priority", "summary", "reply"]
          }
        },
        "required": ["value"],
        "additionalProperties": false
      },
      "strict": false
    }
  ],
  "tool_choice": {
    "type": "function",
    "name": "__baml_return_output"
  },
  "parallel_tool_calls": false
}
```

Responses tools are flat: `name`, `description`, `parameters`, and `strict`
are siblings of `"type": "function"`.

The `value` wrapper is necessary because OpenAI function parameters require an
object root while BAML can return any `T`.

A valid response is:

```json
{
  "id": "resp_123",
  "status": "completed",
  "output": [
    {
      "type": "function_call",
      "call_id": "call_result",
      "name": "__baml_return_output",
      "arguments": "{\"value\":{\"category\":\"billing\",\"priority\":\"Urgent\",\"summary\":\"Duplicate charge\",\"reply\":\"We will investigate.\"}}"
    }
  ]
}
```

BAML reads `.value` and exact-decodes it with
`baml.json.from_json<Resolution>`. Native function arguments never pass
through SAP.

The same wrapper handles every return shape:

- `Resolution[]` puts one array schema under `value`;
- `string` puts one string schema under `value`;
- `Answer | Escalation` puts one union schema under `value`;
- recursive types lift `$defs` to the function-parameter root.

Arrays and unions do not create multiple result functions.

## Example 2: reflection turns a BAML function into a tool

An ordinary BAML function contains the tool definition and implementation:

```baml
/// Search the support knowledge base.
function search_knowledge(query: string) -> json {
  {
    "query": query,
    "article": "Duplicate charges are normally pending authorizations."
  }
}
```

`ai.tools.tool(search_knowledge)` stores the function as a
`baml.AnyFunction`. It gathers metadata with:

```baml
let signature = reflect.signature(search_knowledge)

// name:      "search_knowledge"
// docstring: "Search the support knowledge base."
// args:      [{ name: "query", type: string }]
// opts:      {}
```

`_schema_for_function` converts the reflected arguments to:

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

A native `Agent<Resolution>` sends that application tool beside the result
tool:

```json
{
  "tools": [
    {
      "type": "function",
      "name": "search_knowledge",
      "parameters": {
        "type": "object",
        "properties": {
          "query": { "type": "string" }
        },
        "required": ["query"],
        "additionalProperties": false
      },
      "strict": false
    },
    {
      "type": "function",
      "name": "__baml_return_output",
      "parameters": {
        "type": "object",
        "properties": {
          "value": { "...": "Resolution schema" }
        },
        "required": ["value"],
        "additionalProperties": false
      },
      "strict": false
    }
  ],
  "tool_choice": "required",
  "parallel_tool_calls": false
}
```

Suppose OpenAI requests:

```json
{
  "type": "function_call",
  "call_id": "call_search_1",
  "name": "search_knowledge",
  "arguments": "{\"query\":\"duplicate charge\"}"
}
```

The provider produces a `ToolCall`. The runner finds the matching
`AnyFunction`, validates the named arguments, and invokes it:

```baml
let output = reflect.call_any<json, never>(
  search_knowledge,
  { "query": "duplicate charge" },
)
```

The actual BAML spelling is `reflect.call_any`. It applies normal BAML default
arguments, supports bound methods without exposing `self`, and reports
missing, unknown, or incorrectly typed arguments as correlated tool errors.

The result is sent back using the same call ID:

```json
{
  "previous_response_id": "resp_tools_1",
  "input": [
    {
      "type": "function_call_output",
      "call_id": "call_search_1",
      "output": "{\"query\":\"duplicate charge\",\"article\":\"Duplicate charges are normally pending authorizations.\"}"
    }
  ],
  "store": true
}
```

The next turn may request another application function or call
`__baml_return_output`. A turn cannot mix application calls with the final
result because that would create an ambiguous side-effect boundary.

This is also the default direct-call behavior. A direct LLM function call
lowers to `ai.run.Completion<T>`, and OpenAI completion selects one generation
when the task has no application tools or a bounded `begin` / `step` /
`submit` loop when it does. `ai.run.Generation<T>` is the separate primitive
for callers that require exactly one Responses request.

## Example 3: the exact SAP prompt for a union plus tools

### Hybrid SAP

Direct use keeps application tools native:

```baml
let provider = openai.responses(
  output_mode = openai.OutputMode.Sap,
)

let outcome = task.run(
  runner = ai.run.Agent<Answer | Escalation>.new(
    tools = [search_knowledge],
  ),
)
```

The request contains the native `search_knowledge` function but no
`__baml_return_output`. `tool_choice` is `"auto"`:

```text
function_call present → application ToolCalls
no function_call       → parse output_text as Answer | Escalation
```

### Full prompt/SAP

To avoid the tool API entirely:

```baml
let provider = openai.responses(
  output_mode = openai.OutputMode.Sap,
  tool_mode = openai.ToolMode.Prompt,
)
```

Internally, the provider delegates the agent turn to BAML's prompt-tool
adapter. It re-renders the task with `T | ToolCalls` and appends the reflected
tool catalog to `ctx.output_format`. The adapter invokes the OpenAI
`GenerationProvider` for one turn; it does not re-enter the agent runner.

For:

```baml
class SapAnswer {
  kind: "answer",
  message: string,
}

class SapEscalation {
  kind: "escalation",
  reason: string,
}

type SapSupportDecision = SapAnswer | SapEscalation
```

the live scenario sends this actual prompt:

```text
First call search_knowledge for "duplicate charge". After its
result, return the answer branch with kind "answer" and a short
customer-facing message.
Answer in JSON using any of these schemas:
{
  kind: "answer",
  message: string,
} or {
  kind: "escalation",
  reason: string,
} or {
  calls: [
    {
      id: string,
      name: string,
      args: baml.json.json,
    }
  ],
}

The output contract above permits either the final result or a ToolCalls value.
To use tools, return ToolCalls with one or more calls. Each call must have a
unique id, an exact tool name from the catalog, and args matching that tool's
parameters JSON Schema. Do not invent tool names. After tool results are
provided, choose tools again or return the final result.

Available tools:
[{"name":"search_knowledge","description":"Search the support knowledge base.","parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}}]
```

The HTTP body has that prompt in `input` and no `tools` or `tool_choice`.

The first text response can select the tool branch:

```json
{
  "calls": [
    {
      "id": "call-1",
      "name": "search_knowledge",
      "args": {
        "query": "duplicate charge"
      }
    }
  ]
}
```

SAP parses `ToolCalls`; the runner dispatches with `reflect.call_any`, appends
the result to prompt history, and asks again. The next text response can select
a branch of `T`:

```json
{
  "kind": "answer",
  "message": "The second charge is likely a pending authorization."
}
```

For `T = A | B`, the combined text contract is:

```text
(A | B) | ToolCalls
```

BAML does not turn `A` and `B` into functions. Use literal discriminators for
the result branches. The `{ calls: ... }` shape is reserved for `ToolCalls`;
a structurally overlapping result branch is currently ambiguous.

The task template must include `ctx.output_format`. It controls where this
generated contract appears.

## Rules that apply to every mode

`OutputMode.Strict`:

- closes every object with `additionalProperties: false`;
- marks every property required while preserving nullable BAML optionals;
- recursively lowers nested arrays, unions, and `$defs`;
- rejects map-like objects and unconstrained `json`;
- applies the same strictness to application tools and the result tool;
- forces `parallel_tool_calls: false`.

Parallel calls:

- apply only to independent application functions;
- never depend on whether `T` is a value, array, or union;
- are always disabled for a bounded typed result.

Native and hybrid continuation uses `previous_response_id` and correlated
`function_call_output` items. Saving conversation state stores response IDs
and pending input, not API keys.

Protocol errors do not silently switch modes. Malformed native calls, mixed
result/application calls, exact-decode failures, and unparseable SAP text
surface as typed failures. SAP never repairs malformed native arguments.

## Google Vertex and Google AI

`google.vertex.Gemini` and `google.ai.Gemini` share the Gemini
`generateContent` body. Their endpoint and authentication differ:

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

Vertex uses Google Cloud credentials by default. The provider accepts a
credential file, inline credential JSON, or Application Default Credentials.
Supplying `api_key` instead selects Vertex Express Mode's project-less
publisher endpoint and cannot be combined with project, location, or ADC
settings.
Google AI uses `x-goog-api-key`. The initial request goes through
`PrimitiveClient.build_request`, which also lowers structural image, audio,
video, and PDF values. Later agent turns call
`PrimitiveClient.authenticate_request`; this refreshes Vertex OAuth and
resolves auth-owned URL fields without rewriting the BAML-owned conversation
body.

For a bounded generation in `OutputMode.Native`, `T` becomes Gemini's native
JSON response constraint:

```json
{
  "generationConfig": {
    "responseMimeType": "application/json",
    "responseJsonSchema": {
      "type": "object",
      "properties": {
        "category": { "type": "string" },
        "priority": {
          "type": "string",
          "enum": ["Low", "Normal", "Urgent"]
        },
        "summary": { "type": "string" },
        "reply": { "type": "string" }
      },
      "required": ["category", "priority", "summary", "reply"]
    }
  }
}
```

BAML reads every non-thought text part, parses JSON, and exact-decodes `T`.
`T[]` makes the response schema an array; it does not enable parallel tool
calls.

An agent turn uses Gemini function declarations. For compatibility with
Gemini models before combined structured-output/function-calling support, the
final result becomes the same reserved `{ value: T }` function used by
OpenAI:

```json
{
  "tools": [{
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
  }],
  "toolConfig": {
    "functionCallingConfig": {
      "mode": "ANY"
    }
  }
}
```

The provider preserves the complete model `content`, including thought
signatures, and correlates each `functionCall` with a later
`functionResponse`. Multiple function-call parts mean parallel application
calls; the shape of `T` is unrelated. If parallel calls are disabled, more
than one call is rejected because Gemini has no equivalent request flag.

`OutputMode.Sap + ToolMode.Native` is hybrid: application calls stay Gemini
functions and the final text is parsed by SAP. `Sap + Prompt` puts the entire
`T | ToolCalls` decision in `ctx.output_format` and sends no Gemini tools.
Google streaming is not advertised yet. Message import is deliberately
text-and-role only, and prompt-mode conversations cannot be saved or imported
without the original task render recipe.

## Anthropic Messages

Anthropic has a native strict JSON response format, so its output modes are
`Strict` and `Sap`:

```baml
let provider = anthropic.messages(
  output_mode = anthropic.OutputMode.Strict,
  tool_mode = anthropic.ToolMode.Native,
  strict_tool_inputs = true,
)
```

Strict `T` is sent through `output_config.format`:

```json
{
  "output_config": {
    "format": {
      "type": "json_schema",
      "schema": {
        "type": "object",
        "properties": {
          "category": { "type": "string" },
          "priority": {
            "type": "string",
            "enum": ["Low", "Normal", "Urgent"]
          },
          "summary": { "type": "string" },
          "reply": { "type": "string" }
        },
        "required": ["category", "priority", "summary", "reply"],
        "additionalProperties": false
      }
    }
  }
}
```

Anthropic can combine that final-output schema with application tools. It does
not need a synthetic result tool: `tool_use` continues the agent, while a
normal text content block contains the final JSON `T`.

```json
{
  "tools": [{
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
  }],
  "tool_choice": {
    "type": "auto",
    "disable_parallel_tool_use": false
  }
}
```

The provider retains the whole assistant content array—text, thinking
signatures, and `tool_use` blocks—then appends all correlated `tool_result`
blocks in provider call order in one user message. Missing, duplicate, or
unknown result IDs fail before conversation state is mutated. Native state is
serializable for save/restore without storing API keys or endpoint
credentials.

`OutputMode.Sap` omits `output_config` and parses final text with SAP.
`Sap + Prompt` also renders application tools into the prompt. As with the
other providers, malformed native tool arguments are never repaired by SAP.
Strict streaming sends the same `output_config` before the SSE accumulator is
opened. Prompt-mode conversations cannot be saved or imported without their
original task render recipe.

Anthropic input media uses its Messages content blocks. Images and PDFs are
supported as URL or base64 sources; video is rejected before the request is
sent.
