# The built-in clients

This page documents the wire choices each built-in client makes: how
the output type and the tools are represented on each provider's API,
with the concrete request and response bodies.
`02_the_client_interface.md` defines the contract these clients
implement; `../../05_appendix/03_future_phases.md` records the native
output modes that arrive in phase 2.

Every example on this page renders this function:

```baml
class Itinerary {
    flights: Flight[],
    hotels: Hotel[],
    total_cost_usd: float,
}

/// Search available flights for a route and month.
function search_flights(origin: string, destination: string, month: string) -> Flight[] {
    flight_api.search(origin, destination, month)
}

function PlanTrip(trip_request: string) -> Itinerary {
    client: "openai/gpt-5.6"
    tools: [search_flights]
    prompt: `
        You are a travel agent. The brief: ${trip_request}
        ${ctx.output_format}
    `
}
```

## The two representation choices

A client makes two independent choices per request, and each has a
visible knob:

1. How the final `T` is represented — the `output_mode` field on each
   built-in client.
2. How the tools are represented — native function calling, in every
   built-in client. The alternative is describing the tool catalog in
   prompt text; that is not a client field but a phase 2 wrapper
   client named `PromptTools`, introduced in the last section of this
   page.

| `output_mode` | Tools | Status |
|---|---|---|
| `Sap` — schema text in the prompt (`${ctx.output_format}`), parsed with repair by the runner | native function calling | this phase, the default everywhere |
| `Native` / `Strict` — OpenAI strict schemas or a reserved result function, Anthropic `output_config`, Gemini `responseJsonSchema` | native function calling | phase 2 values of the same field |
| any | tool catalog in prompt text, via `PromptTools` | phase 2 wrapper |

`output_mode` ships now with the single value `Sap`, so enabling a
phase 2 mode is a field change, not a new configuration surface. The
two axes compose without cross-field rules: no field can request
prompt-mode tools, so no combination of settings is invalid. The
wrapper works over any `output_mode` — over a `Sap` inner client the
whole contract lives in prompt text, and over a `Native` inner client
the `T | ToolCalls` envelope travels inside the native schema.

## Rules shared by every client

- Every tool schema on this page derives from one source.
  `reflect.signature(search_flights)` supplies the name, the docstring
  as the description, and the parameters with their types and
  defaults; `baml.json.schema` lowers the types into
  `Tool.input_schema`. A client only reshapes that
  field into its wire format — the three `parameters` /
  `input_schema` / `parametersJsonSchema` bodies below are the same
  schema in three framings.
- Executing a call is never wire work. The runner validates the
  model's arguments against the schema and dispatches through
  `reflect.call_any`; a client only reports the calls as `ToolUse`
  blocks and later lowers their results.
- The decision per turn is uniform: any function call in the response
  produces `ToolUse` blocks and `stop_reason: ToolUse`; otherwise the
  terminal text is the final candidate and the stop reason is
  `Complete`. A turn with tool calls is never a final turn.
- Tool results serialize uniformly before wire framing: a
  `ToolCompleted` contributes its JSON output, a `ToolFailed`
  contributes `{"error": message}`. The framing differs per API; the
  model sees the same error shape everywhere.
- Malformed tool arguments from the model are not repaired. They fail
  validation and return to the model as a tool error.
- The tool name `__baml_return_output` is reserved in every mode, so
  enabling a phase 2 native mode cannot change the meaning of an
  existing tool. `tool()` and spec creation reject it.
- Reasoning content lowers to its readable projection only. Signed and
  encrypted reasoning data is dropped in this phase; exact
  same-provider replay of reasoning is a phase 2 capability.

## OpenAI (`OpenAiClient`)

`POST /v1/responses`, with `store: false` and the complete input on
every turn. The Responses API is the target, rather than Chat
Completions, because phase 3 response chaining exists only there; an
OpenAI-compatible endpoint that speaks only Chat Completions registers
the compatibility codec instead (`01_choosing_a_model.md`).

The first turn of `PlanTrip("2 weeks in Japan")`:

```json
{
  "model": "gpt-5.6",
  "store": false,
  "input": [
    { "role": "user", "content": "You are a travel agent. The brief: 2 weeks in Japan\n..." }
  ],
  "tools": [
    {
      "type": "function",
      "name": "search_flights",
      "description": "Search available flights for a route and month.",
      "parameters": {
        "type": "object",
        "properties": {
          "origin": { "type": "string" },
          "destination": { "type": "string" },
          "month": { "type": "string" }
        },
        "required": ["origin", "destination", "month"],
        "additionalProperties": false
      },
      "strict": false
    }
  ],
  "tool_choice": "auto",
  "parallel_tool_calls": true
}
```

Invoking the bound prompt template produces ordered messages, which the client places first in
`input`; the lowered journal follows them. Role-less prompt content maps to
`user`, authored roles remain roles accepted by Responses, and an authored
`tool` prompt role falls back to `user` because tool results use dedicated
input-item shapes. The prompt renders fresh on every turn.

The model calls a tool:

```json
{
  "id": "resp_123",
  "status": "completed",
  "output": [
    {
      "type": "function_call",
      "call_id": "call_a1",
      "name": "search_flights",
      "arguments": "{\"origin\":\"SFO\",\"destination\":\"NRT\",\"month\":\"2026-10\"}"
    }
  ],
  "usage": {
    "input_tokens": 812,
    "output_tokens": 41,
    "input_tokens_details": { "cached_tokens": 0 },
    "output_tokens_details": { "reasoning_tokens": 0 }
  }
}
```

The client normalizes that to:

```baml
ModelTurn {
    content: [ToolUse { id: "call_a1", name: "search_flights", args: {...} }],
    stop_reason: StopReason.ToolUse,
    usage: Usage { input_tokens: 812, output_tokens: 41, cached_input_tokens: 0, reasoning_tokens: 0 },
}
```

After the runner executes the tool, the next turn's `input` ends with
the lowered result. The earlier `function_call` item is lowered from
the `AssistantMessage` blocks; the result correlates by `call_id`:

```json
{
  "type": "function_call_output",
  "call_id": "call_a1",
  "output": "[{\"airline\":\"ANA\",\"price_usd\":950}]"
}
```

A turn with no function calls ends the loop: the `output_text` content
is the final candidate, and the runner parses it as `Itinerary`.
`Reasoning` blocks come from reasoning item summaries.
`status: incomplete` with a token limit maps to `MaxTokens`; a
`refusal` content part maps to `Refused`.

In phase 2, the native output option replaces the schema text with a
reserved result function, so the final result and the tool calls share
one function-calling turn:

```json
{
  "tools": [
    { "type": "function", "name": "search_flights", "...": "..." },
    {
      "type": "function",
      "name": "__baml_return_output",
      "description": "Return the final typed result. Call this only when the task is complete.",
      "parameters": {
        "type": "object",
        "properties": { "value": { "...": "schema for Itinerary" } },
        "required": ["value"],
        "additionalProperties": false
      },
      "strict": false
    }
  ],
  "tool_choice": "required"
}
```

The `value` wrapper exists because a function's parameter root must be
an object while a BAML output type can be anything: `Itinerary[]` puts
an array schema under `value`, `string` puts a string schema there,
and recursive types lift `$defs` to the parameter root. The strict
option additionally closes every object and marks every property
required, with BAML optionals represented as nullables. Phase 3 adds
`previous_response_id` chaining.

## Anthropic (`AnthropicClient`)

`POST /v1/messages`, with the `anthropic-version` header and the
required `max_tokens` from the client's configuration.

```json
{
  "model": "claude-haiku-4-5",
  "max_tokens": 4096,
  "messages": [
    {
      "role": "user",
      "content": [
        { "type": "text", "text": "You are a travel agent. The brief: 2 weeks in Japan\n..." }
      ]
    }
  ],
  "tools": [
    {
      "name": "search_flights",
      "description": "Search available flights for a route and month.",
      "input_schema": {
        "type": "object",
        "properties": {
          "origin": { "type": "string" },
          "destination": { "type": "string" },
          "month": { "type": "string" }
        },
        "required": ["origin", "destination", "month"],
        "additionalProperties": false
      }
    }
  ],
  "tool_choice": { "type": "auto", "disable_parallel_tool_use": false }
}
```

The client extracts authored system prompt messages into Anthropic's top-level
`system` blocks. Other prompt messages map to `user` or `assistant` and precede
the lowered journal; role-less content maps to `user`, as in the example. The
API requires a non-empty message list beginning with a user message. If the
combined prompt and journal would not begin with one and system blocks are
available, those blocks move into a leading user message and `system` is
omitted. Otherwise the authored system blocks remain in `system`.

The model calls a tool with interleaved content blocks:

```json
{
  "stop_reason": "tool_use",
  "content": [
    { "type": "text", "text": "I will check flights first." },
    { "type": "thinking", "thinking": "October pricing is usually...", "signature": "Eo8BCkYw..." },
    {
      "type": "tool_use",
      "id": "toolu_b7",
      "name": "search_flights",
      "input": { "origin": "SFO", "destination": "NRT", "month": "2026-10" }
    }
  ],
  "usage": { "input_tokens": 780, "output_tokens": 65 }
}
```

normalizes to:

```baml
ModelTurn {
    content: [
        Text { text: "I will check flights first." },
        Reasoning { summary: "October pricing is usually..." },
        ToolUse { id: "toolu_b7", name: "search_flights", args: {...} },
    ],
    stop_reason: StopReason.ToolUse,
    usage: Usage { input_tokens: 780, output_tokens: 65 },
}
```

The `signature` on the thinking block is dropped in this phase;
preserving it for exact same-provider replay is the phase 2 capsule.
Tool results lower as one user message of `tool_result` blocks:

```json
{
  "role": "user",
  "content": [
    {
      "type": "tool_result",
      "tool_use_id": "toolu_b7",
      "content": "[{\"airline\":\"ANA\",\"price_usd\":950}]",
      "is_error": false
    }
  ]
}
```

`stop_reason` maps directly: `end_turn` to `Complete`, `tool_use` to
`ToolUse`, `max_tokens` to `MaxTokens`, `refusal` to `Refused`.

In phase 2 the native output option sets `output_config` instead of
rendering schema text:

```json
{
  "output_config": {
    "format": { "type": "json_schema", "schema": { "...": "schema for Itinerary" } }
  }
}
```

This composes with native tools — text carries the final JSON while
`tool_use` blocks carry calls — so Anthropic never needs the reserved
result function. Anthropic's closed-schema rewrite sets
`additionalProperties: false` but preserves the original `required`
list, unlike OpenAI strict.

## Google (`GoogleClient`)

`POST .../models/{model}:generateContent` against AI Studio, with
`x-goog-api-key`. Vertex authentication modes are a later variant of
the same codec.

```json
{
  "contents": [
    { "role": "user", "parts": [{ "text": "You are a travel agent. The brief: 2 weeks in Japan\n..." }] }
  ],
  "tools": [
    {
      "functionDeclarations": [
        {
          "name": "search_flights",
          "description": "Search available flights for a route and month.",
          "parametersJsonSchema": {
            "type": "object",
            "properties": {
              "origin": { "type": "string" },
              "destination": { "type": "string" },
              "month": { "type": "string" }
            },
            "required": ["origin", "destination", "month"]
          }
        }
      ]
    }
  ],
  "toolConfig": { "functionCallingConfig": { "mode": "AUTO" } }
}
```

The client extracts authored system prompt messages into
`systemInstruction`. Other prompt messages precede the lowered journal in
`contents`: role-less content maps to `user`, authored assistant content maps
to `model`, and other non-system roles map to `user`. If the resulting prompt
would be system-only or assistant-first and system parts are available, those
parts move into a leading user content instead of `systemInstruction`. This
preserves Gemini's required leading-user shape without flattening an otherwise
well-formed multi-message prompt.

The model's calls arrive as parts of one candidate:

```json
{
  "candidates": [
    {
      "finishReason": "STOP",
      "content": {
        "parts": [
          { "text": "Checking flight options.", "thought": true },
          { "functionCall": { "name": "search_flights", "args": { "origin": "SFO", "destination": "NRT", "month": "2026-10" } } }
        ]
      }
    }
  ],
  "usageMetadata": { "promptTokenCount": 700, "candidatesTokenCount": 58, "thoughtsTokenCount": 21 }
}
```

Gemini sends no call ids, so the client synthesizes deterministic ids
from the journal position (`call_9_0` for the first call of the turn
committed at sequence 9); the same turn produces the same ids on
re-render. Parts flagged `thought: true` lower to `Reasoning`; opaque
`thoughtSignature` data is dropped in this phase. Results lower as
`functionResponse` parts in one user content:

```json
{
  "role": "user",
  "parts": [
    {
      "functionResponse": {
        "id": "call_9_0",
        "name": "search_flights",
        "response": { "result": [{ "airline": "ANA", "price_usd": 950 }], "is_error": false }
      }
    }
  ]
}
```

Refusal detection reads `promptFeedback.blockReason` first, then the
safety finish reasons (`SAFETY`, `RECITATION`, `PROHIBITED_CONTENT`,
and their variants); `MAX_TOKENS` maps to `MaxTokens`. The API has no
parallel-tool-calls switch; the client accepts whatever set of calls
the model returns.

In phase 2 the native output option uses controlled generation — but
only without tools, because Gemini cannot combine it with function
calling:

```json
{
  "generationConfig": {
    "responseMimeType": "application/json",
    "responseJsonSchema": { "...": "schema for Itinerary" }
  }
}
```

When tools are present, the native mode falls back to the same
reserved `__baml_return_output` function declaration OpenAI uses, so
application calls and the final result share one function-calling
turn.

## Media lowering

A media argument lowers structurally, never as inlined text:

| | `image` | `audio` | `pdf` | `video` |
|---|---|---|---|---|
| OpenAI | `input_image` (URL or data URL) | `input_audio` (`wav`, `mp3`, `flac`, `ogg`, `webm`) | `input_file` | rejected |
| Anthropic | `image` block (URL or base64) | rejected | `document` block | rejected |
| Google | `inlineData` / `fileData` | `inlineData` / `fileData` | `inlineData` / `fileData` | `inlineData` / `fileData` |

A rejected cell throws `baml.errors.Unsupported` when the request is
rendered.

Media output is a phase 2 capability. A Gemini image model returns
images as inline parts, which normalize to `Media` blocks; OpenAI
image generation is a hosted tool and follows hosted-tool support.
When the function's return type is exactly `image` or `image[]`, the
runner binds the turn's `Media` blocks as the final value instead of
parsing text (`../../05_appendix/03_future_phases.md`).

## Claude Code (`ClaudeCodeClient`)

A harness client: the transport is the authenticated `claude` CLI as a
local process, not HTTP, which is the first proof that `Client`
abstracts more than wire APIs. One `invoke` is one CLI run:

```
claude -p --output-format json --model haiku --permission-mode default \
    --no-session-persistence --tools "" --json-schema <schema> <prompt>
```

- The output contract travels natively: the client invokes the prompt template
  with an empty string, projects the resulting `ai.Prompt` with `.text()`, and
  sends the schema through `--json-schema`
  — the one shipped client whose contract is on the wire rather than
  in the prompt.
- The CLI's `-p` mode takes a single prompt, so the journal folds into
  it as labeled text (`user:`, `assistant:`,
  `tool result id=... is_error=false: ...`); each invoke is a fresh
  process whose only memory is that text.
- The transport is `baml.sys.start_process` with
  `--output-format stream-json`: the harness's complete event
  transcript — init, hooks, its inner assistant turns and tool
  attempts — logs live, line by line, as the run executes. This is
  observability, not journal content; the journal records only the
  normalized turn.
- Credentials are the CLI's own login. There is no API key.

BAML tools ride the `outcome` envelope, the native-schema variant of
the `PromptTools` technique: when the toolbox is non-empty the schema
becomes one object that offers either the final result or a tool
request, and the prompt carries the catalog and the protocol:

```json
{
  "type": "object",
  "properties": {
    "outcome": {
      "anyOf": [
        { "...": "schema for Itinerary; its $defs lift to the envelope root" },
        {
          "type": "object",
          "properties": {
            "calls": { "type": "array", "items": { "...": "{id, name, args}" } }
          },
          "required": ["calls"]
        }
      ]
    }
  },
  "required": ["outcome"]
}
```

A `calls` outcome normalizes to `ToolUse` blocks with
`stop_reason: ToolUse`; the runner executes the BAML tools and the
next invoke shows their results in the conversation text. A final
outcome becomes the terminal `Text` candidate. Three protocol lines
are load-bearing, each learned from a live event transcript: the
catalog tools must be declared NOT installed (the harness's inner
agent otherwise attempts native `tool_use` calls against them and
burns a round of internal errors before finding the envelope); an
outcome with no tool results yet in the conversation must be a calls
request (the model otherwise invents data); and a successful result
must never be re-requested (each invoke is a fresh process). The event
transcript also shows how `--json-schema` works internally: the
constrained output is emitted through a harness-internal
`StructuredOutput` tool. Claude Code's own tools (`--tools`) run inside the harness
and are configured separately via `harness_tools`; only envelope calls
dispatch to BAML.

The harness's MCP support is client configuration: the `mcp_servers`
field maps a server name to that server's launch configuration, in the
same shape Claude Code users write in an `.mcp.json` file. Every
invoke renders the map into `--mcp-config`, adds `--strict-mcp-config`
so only the listed servers attach (never the user's or the project's
own MCP configuration), and adds `--allowedTools=mcp__<name>` because
the CLI runs non-interactively and nothing can answer a permission
prompt during a turn. `--tools` governs only the built-in set, so
`harness_tools` and MCP attachment are independent. The map is read on
every invoke and each invoke is a fresh process, so an entry added
between turns attaches on the next turn
(`../../03_how_to/05_attach_mcp_servers_to_claude_code.md`). An
attached server's tools run inside the harness like `harness_tools`,
and their calls are not recorded in the journal; for MCP tools that
are journaled and work with every client, use the `root.mcp` library
(`../../03_how_to/06_use_mcp_tools_with_any_client.md`).

The result JSON carries `structured_output` (decoded directly, no
repair), `usage` (with `cache_read_input_tokens` mapping to
`cached_input_tokens`), `is_error`/`subtype` (mapped to `Refused`),
and a `session_id` that the CLI's `--resume` accepts — a natural
continuation checkpoint for phase 3.

The responsibility split holds: the runner drives the loop, and the
harness's internal episode is an inside-the-turn capability, like a
hosted tool. Two consequences follow. `max_steps` counts envelope
rounds, not the harness's underlying model calls, so one step can be
long and expensive. And retry safety is honest only while
`harness_tools` is empty and no attached MCP server's tools change
state outside the run: with either enabled, a failed turn may have
failed after such a change, so the client should classify the failure
`Unknown` rather than `Safe` — not yet implemented.

## Prompt-mode tools are a wrapper, not a mode

Prompt-mode tool calling — describing the tool catalog in text and
recognizing a `T | ToolCalls` union in the reply — is not a
configuration flag on the built-in clients. It arrives in phase 2 as
one wrapper client that composes with any inner client:

```baml
let weak: Client = ai.clients.PromptTools {
    inner: OpenAiClient { model: "qwen3:8b", base_url: "http://localhost:11434/v1", api_key: "" },
};
let trip: Itinerary = PlanTrip("2 weeks in Japan", $client = weak);
```

`PromptTools.invoke` renders the tool catalog and a calls protocol
into the instructions, passes the inner client an empty toolbox so no
native function calling engages, and lowers tool-bearing journal turns
as the same text protocol. On the response side it recognizes a
`ToolCalls` envelope in the reply and rewrites it into `ToolUse`
blocks with `stop_reason: ToolUse`; any other reply passes through as
the final candidate. The runner cannot tell the difference.

The wrapper exists for models with unreliable native tool support and
for wire APIs without function calling, and it works with custom
clients as well as the built-ins. One caution: an output type that
structurally overlaps the calls envelope is ambiguous in this mode, so
give union branches literal discriminators.
`../../05_appendix/02_alternatives_considered.md` records why this is
a wrapper rather than a per-client mode.
