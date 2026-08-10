# API reference

All names live under `ai` unless noted. This page states
signatures and behavior without narrative; the guides explain usage.

## The tree

```
ai
├── FunctionSpec<Out>                     MyFunc@spec(args); one unit of model work, bound and unrun
├── Runner<Out>                           interface: type Output, type Error; run(spec) -> Self.Output
├── Agent<Out>                            the default runner; $ parameters set these fields
├── RunResult<Out>                        value, journal, usage
├── Client                                interface: id(), invoke(ModelTurnInput) -> ModelTurn
├── ModelTurnInput / ModelTurn            one turn's materials; one turn's canonical result
├── Prompt                                the template surface a client renders
├── content                               Text / Reasoning / ToolUse; StopReason
├── Journal / events                      the run record; RunStarted ... FinalProduced
├── tools                                 Tool, Toolbox, tool(), ToolErrorMode
├── clients                               register/resolve; built-ins; Retry, Fallback
├── wire                                  send_as<T>, render_output_format, schema rewrites
└── errors                                the ai.errors namespace: Failure, RetrySafety, the classes, classify_http
```

## `FunctionSpec<Out>`

Created by `MyFunc@spec(args)`. Immutable; getters only.

```baml
class FunctionSpec<Out> {
    default_client: Client,

    function name(self) -> string throws never
    function arguments(self) -> map<string, unknown> throws never
    function output_type(self) -> type throws never
    function prompt(self) -> Prompt throws never
    function tools(self) -> Toolbox throws never
}
```

`default_client` holds the resolved client; `client` is a keyword and
cannot be a method name. Resolution of the function's `client:` string
happens at spec creation and throws there on an unknown prefix or
missing credential.

## `Runner<Out>` and `Agent<Out>`

```baml
interface Runner<Out> {
    type Output
    type Error
    function run(self, spec: FunctionSpec<Out>) -> Self.Output throws Self.Error
}

enum ToolErrorMode { Report, Raise }

class Agent<Out> {
    max_steps: int,                                  // default 12
    client: Client?,                                 // default null: use spec.default_client
    tool_errors: ToolErrorMode,                      // default Report
    on_event: ((Event) -> null throws never)?,       // default null

    function new(
        max_steps: int = 12,
        client: Client? = null,
        tool_errors: ToolErrorMode = ToolErrorMode.Report,
        on_event: ((Event) -> null throws never)? = null,
    ) -> Agent<Out> throws never

    implements Runner<Out> {
        type Output = RunResult<Out>
        type Error = Failure | baml.errors.UnknownError
        function run(self, spec: FunctionSpec<Out>) -> RunResult<Out>
            throws Failure | baml.errors.UnknownError
    }
}
```

`run` throws `StepBudgetExceeded` when `max_steps` model turns
complete without a final output, `ToolFailedError` when a `Raise`-mode
tool fails, and otherwise propagates the client's classified failure.
The loop's exact sequence is
`../02_guides/02_specs_and_runners/02_the_default_runner.md`.

There are no intermediate loop helpers. A custom runner composes the
primitives directly — `Journal.new(spec)` and `append_all`,
`client.invoke` with a `ModelTurnInput` it assembles (which is also
how it supplies a different toolbox), `Tool.call` for dispatch, and
`baml.sap.parse<Out>` for the final parse — and upholds the loop
invariants itself
(`../02_guides/02_specs_and_runners/03_writing_a_runner.md`).

## `RunResult<Out>`

```baml
class RunResult<Out> {
    value: Out,
    journal: Journal,
    usage: Usage,       // aggregate of the run's Usage events
}
```

## `Client`, `ModelTurnInput`, `ModelTurn`

```baml
interface Client {
    function id(self) -> string throws never
    function invoke(self, input: ModelTurnInput) -> ModelTurn
        throws Failure | baml.errors.UnknownError
}

class ModelTurnInput {
    prompt: Prompt,
    journal: Journal,
    toolbox: Toolbox,
    output_type: type,
}

class ModelTurn {
    content: ContentBlock[],
    stop_reason: StopReason,
    usage: Usage?,

    function terminal_text(self) -> string? throws never
        // the terminal Text block — the final candidate the runner parses
    function tool_uses(self) -> ToolUse[] throws never
        // the turn's ToolUse blocks, in order
}
```

Contract for `invoke`: one wire call per invocation in this phase; no
journal writes; no tool execution; no parsing of `Out`; every request
built from `input` alone. A throw means no turn was produced.

## Content blocks and `StopReason`

```baml
type ContentBlock = Text | Reasoning | ToolUse | Media

class Text      { text: string }
class Reasoning { summary: string }
class ToolUse   { id: string, name: string, args: map<string, unknown> }
class Media     { value: image | audio }

enum StopReason { Complete, ToolUse, MaxTokens, Refused }
```

Rules: `ToolUse.id` is unique within a turn. `stop_reason: Complete`
requires a terminal `Text` block, which is the final candidate.
`stop_reason: ToolUse` requires at least one `ToolUse` block.
`MaxTokens` and `Refused` end the run with `ParseFailed` and `Refused`
respectively. `Media` carries inline model output; binding it to a
media return type is a phase 2 rule
(`../05_appendix/03_future_phases.md`).

## `Prompt`

```baml
type InstructionPart = string | image | audio | video | pdf

class Prompt {
    function render(self, output_format: string) -> InstructionPart[] throws never
    function render_text(self, output_format: string) -> string throws baml.errors.Unsupported
}
```

`render` substitutes the bound arguments and the given text at
`${ctx.output_format}` and returns the turn's instructions as parts:
text segments and media arguments alternate in template order, and a
text-only template renders as one text part. `render_text` joins an
all-text rendering and throws `Unsupported` when a media argument is
present. The conversation is not part of the rendering; the client
lowers the journal as messages after the instructions.

## `Journal` and events

```baml
class Journal {
    function entries(self) -> Event[] throws never
    function new<Out>(spec: FunctionSpec<Out>) -> Journal throws never   // appends RunStarted
    function append_all(self, events: Event[]) -> void throws never
        // the write. The driving runner is the only writer; clients and
        // tools never append.
}
```

Event classes and their rules are `02_events.md`.

## `tools`

```baml
class Tool {
    name: string,
    description: string,
    input_schema: json,
    handler: baml.AnyFunction?,              // signature-derived dispatch; null for raw tools
    raw_handler: ((map<string, unknown>) -> string throws unknown)?,
                                             // dynamic-source dispatch over the raw argument map
    on_error: ToolErrorMode?,                // null: inherit the run's tool_errors mode

    function call(self, args: map<string, unknown>) -> string
        throws baml.errors.InvalidArgument | baml.errors.UnknownError
        // the total boundary: validate, dispatch via reflect.call_any,
        // serialize the result as JSON text; a raw tool dispatches its
        // raw_handler over the argument map instead
}

class Toolbox {
    function new(tools: Tool[]) -> Toolbox throws baml.errors.InvalidArgument
        // rejects duplicate names, so a toolbox is well-formed on construction
    function list(self) -> Tool[] throws never
    function get(self, name: string) -> Tool? throws never
}

function tool(
    handler: baml.AnyFunction<Returns = unknown>,
    name: string? = null,                    // default: the function's name
    description: string? = null,             // default: the docstring
    on_error: ToolErrorMode? = null,         // null: inherit the run's tool_errors mode
) -> Tool throws never

function raw_tool(
    name: string,
    description: string,
    input_schema: json,                      // supplied, not signature-derived
    handler: (map<string, unknown>) -> string throws unknown,
    on_error: ToolErrorMode? = null,
) -> Tool throws baml.errors.InvalidArgument // the reserved name
```

A `tools:` list accepts functions and `Tool` values; functions
normalize through `tool()`. Duplicate names in one toolbox throw at
spec creation, and the name `__baml_return_output` is reserved
(`../02_guides/03_clients/05_the_built_in_clients.md`). Tool execution validates arguments against
`input_schema`, calls the handler via `reflect.call_any` — whose
validation widens an integral JSON number into a `float` or `float?`
parameter when the value is exactly representable (up to 2^53; JSON
Schema `number` accepts integers, and a lossy value stays an
`InvalidArgumentError`) — and maps
any throw to a result or `ToolFailedError` per the effective mode. A
null `on_error` inherits the run's `tool_errors` mode; an explicit
per-tool value wins.

## `clients`

```baml
function register(
    prefix: string,
    factory: (model: string) -> Client throws baml.errors.InvalidArgument | baml.errors.UnknownError,
) -> null
    throws baml.errors.InvalidArgument     // duplicate prefix
function resolve(shorthand: string) -> Client
    throws baml.errors.InvalidArgument | baml.errors.UnknownError
    // unknown prefix or bad configuration; factory errors propagate
```

Built-in clients. Every client is a plain value; `new` defaults every
parameter, reads the credential from the environment when `api_key` is
null, and throws when neither is available:

```baml
enum OutputMode { Sap }    // phase 2 adds Native and Strict

class OpenAiClient {
    model: string,
    api_key: string,
    base_url: string?,
    extra_headers: map<string, string>?,
    output_mode: OutputMode,

    function new(
        model: string = "gpt-4o-mini",
        api_key: string? = null,                 // null: read OPENAI_API_KEY
        base_url: string? = null,
        extra_headers: map<string, string>? = null,
        output_mode: OutputMode = OutputMode.Sap,
    ) -> OpenAiClient throws baml.errors.InvalidArgument
}

class AnthropicClient {
    model: string,
    api_key: string,
    base_url: string?,
    max_tokens: int,
    output_mode: OutputMode,

    function new(
        model: string = "claude-haiku-4-5",
        api_key: string? = null,                 // null: read ANTHROPIC_API_KEY
        base_url: string? = null,
        max_tokens: int = 4096,
        output_mode: OutputMode = OutputMode.Sap,
    ) -> AnthropicClient throws baml.errors.InvalidArgument
}

class GoogleClient {
    model: string,
    api_key: string,
    base_url: string?,
    output_mode: OutputMode,

    function new(
        model: string = "gemini-2.5-flash",
        api_key: string? = null,                 // null: read GOOGLE_API_KEY
        base_url: string? = null,
        output_mode: OutputMode = OutputMode.Sap,
    ) -> GoogleClient throws baml.errors.InvalidArgument
}
```

`resolve("openai/gpt-5.6")` is `OpenAiClient.new(model = "gpt-5.6")`.
A class literal remains available for full control, and spreading an
existing client derives a variant (`OpenAiClient { ...base, model: "gpt-5.5" }`). `output_mode` selects how the output contract travels
(`../02_guides/03_clients/05_the_built_in_clients.md`); `Sap` renders
it as prompt text.

`ScriptedClient` is test scaffolding, not a member of the `ai`
namespace: any value implementing `Client` works, so the deterministic
fake ships with the tests and lives in application space. It returns
pre-written turns in order and records the inputs it received:

```baml
class ScriptedClient {
    turns: ModelTurn[],
    function received(self) -> ModelTurnInput[] throws never
}
```

Wrapper clients:

```baml
class Retry {
    inner: Client,
    max_attempts: int,
    retry_if: ((Failure) -> bool throws never)?,   // default judgment: see 03_errors.md
    backoff: Backoff?,
}

class Backoff { initial_ms: int, multiplier: int, max_ms: int }

class Fallback { members: Client[] }
```

Both implement `Client`; `id()` reports the inner or current member's
id.

## `wire`

```baml
function send_as<T>(req: baml.http.Request, provider: string) -> T
    throws RateLimited | NetworkFailure | InvalidRequest | ParseFailed
function render_output_format(t: type) -> string throws baml.errors.Unsupported
function closed_schema(s: json) -> json throws never
function strict_schema(s: json) -> json throws never
```

`send_as` sends, classifies non-2xx status via `classify_http`, and
decodes the body as `T`, throwing `ParseFailed` when the body does not
decode. Unknown fields are ignored, so an envelope class types only
the fields the client reads and keeps open-ended parts as `json`
fields. `send_as<json>` returns the undecoded body. `closed_schema` sets
`additionalProperties: false` recursively and preserves `required`;
`strict_schema` additionally makes every property required. Both walk
`properties`, `$defs`, `definitions`, `patternProperties`, and
`dependentSchemas`.

## `errors`

The failure taxonomy is its own namespace, `ai.errors`, mirroring
`baml.errors`. Classes, fields, and the classification table are
`03_errors.md`.

```baml
interface Failure {
    function retry_safety(self) -> RetrySafety throws never
}
enum RetrySafety { Safe, Unknown, Unsafe }

function classify_http(provider: string, status_code: int, body: string) -> Failure throws never
```

## Standard library dependencies

Existing primitives this API builds on; none are introduced by this
BEP:

| Primitive | Used by |
|---|---|
| `baml.json.schema(t: type) -> json` | `tool()` schema derivation, `wire.render_output_format`, custom clients |
| `baml.sap.parse<T>(text: string) -> T` | the runner's final parse; custom runners |
| `baml.json.parse` / `stringify` / `from_json<T>` / `from_string<T>` | clients, everywhere JSON crosses a boundary |
| `baml.http.Request` / `Response` / `send` / `fetch_sse` | `wire.send_as`; clients that bypass it; streaming later |
| `baml.sys.exec` / `start_process` | process-transport clients; the Claude Code client streams its event transcript live |
| `reflect.type_of<T>` / `signature` / `call_any` | tool schema derivation; tool execution |
| `baml.env.get` / `get_or_panic` | credential resolution in registry factories |

`wire` is a convenience layer over these; dropping to the primitives
is always available.
