# BEP: LLM functions, specs, runners, and clients

## Summary

This BEP defines how a BAML LLM function executes. It reifies each call
into a spec, drives the spec with a runner, and performs each model turn
through a client. All three are ordinary values with ordinary
interfaces, so applications can write their own runners and their own
clients without new language features.

The core pieces:

- **LLM functions.** A function whose body is a prompt and whose return
  type is the schema the model must produce. A `tools:` list turns the
  function into an agent that works in a loop until it produces the
  return type.
- **Specs.** `MyFunc@spec(args)` binds the function's arguments and
  returns a `FunctionSpec` value without calling a model. A spec is the
  currency between the language and the runtime.
- **Runners.** A runner consumes a spec and drives it to completion.
  Calling an LLM function runs the built-in `Agent` runner. Custom
  runners implement one interface.
- **Clients.** A client performs one model turn over one provider wire
  API. `"openai/gpt-5.6"` and `"openai/gpt-5.5"` resolve to the same
  client implementation with a different model field. Custom clients
  implement one interface.
- **The journal.** Every run records an append-only log of typed
  events. The journal is both the transcript source for the next model
  turn and the trace of the finished run.

This BEP does not include sessions, steering, policies, or background
jobs. The design reserves room for them; see
`pages/05_appendix/03_future_phases.md`.

## The flow

```
PlanTrip("2 weeks in Japan")                          // sugar for:
Agent<Itinerary>.new().run(PlanTrip@spec(trip_request = "2 weeks in Japan")).value

FunctionSpec ──run──► Agent (runner) ──invoke──► Client ──HTTP──► provider API
                        │                          │
                        │◄──────── ModelTurn ──────┘
                        │   content blocks, stop reason, usage
                        │
                        ├─ commit the turn's events to the journal
                        ├─ execute tool calls, append correlated results
                        └─ repeat until the output parses, then return
                           RunResult { value, journal, usage }
```

The runner owns the loop, tool execution, result correlation, and the
typed parse of the final output. The client owns rendering, transport,
and normalizing the wire response into canonical content. The journal
owns all state; a client holds none.

A `$` parameter at a call site sets the matching field on the default
runner, so `PlanTrip(req, $client = c)` means
`Agent { client: c }.run(PlanTrip@spec(trip_request = req)).value`.
Anything a `$` parameter cannot express uses a runner explicitly.

## The API

```
ai
├── FunctionSpec<Out>                     MyFunc@spec(args); one unit of model work, bound and unrun
│   ├── name() / arguments()              identity and the bound argument values
│   ├── output_type()                     the return type as a runtime type value
│   └── tools() / default_client          the initial toolbox; the resolved default client
├── Runner<Out>                           interface: type Output, type Error; run(spec) -> Self.Output
├── Agent<Out>                            the default runner; $ parameters set these fields
│   ├── max_steps                         model-turn budget, default 12
│   ├── client                            overrides the spec's client when set
│   ├── tool_errors                       Report (default) or Raise; per-tool settings win
│   ├── on_event                          observes journal events as they append
│   └── run(spec) -> RunResult<Out>       the loop: invoke, commit, tools, parse, repeat
├── RunResult<Out>                        value, journal, usage
├── Client                                interface: id(), invoke(ModelTurnInput) -> ModelTurn
├── ModelTurnInput                        prompt, journal, toolbox, output_type
├── Prompt                                the function's template with arguments bound
│   ├── render(output_format)             the turn's instruction parts (text and media arguments)
│   └── render_text(output_format)        all-text convenience; throws when a media argument is present
├── ModelTurn                             content, stop_reason, usage
├── content
│   ├── Text / Reasoning / ToolUse / Media   canonical assistant content blocks
│   └── StopReason                        Complete | ToolUse | MaxTokens | Refused
├── Journal                               append-only typed record of one run
├── events                                RunStarted, UserMessage, AssistantMessage, ToolRequested,
│                                         ToolCompleted, ToolFailed, Usage, FinalProduced
├── tools
│   ├── Tool                              name, description, input schema, handler, on_error
│   ├── Toolbox                           the active tool set; render and lookup
│   ├── tool(fn, name =, description =, on_error =)   explicit constructor; schemas come from signatures
│   └── raw_tool(name, description, schema, handler)  dynamic tool sources (MCP); the schema is supplied
├── clients
│   ├── register(prefix, factory)         makes "prefix/model" strings resolvable
│   ├── resolve(shorthand)                "openai/gpt-5.6" -> OpenAiClient { model: "gpt-5.6" }
│   ├── OpenAiClient                      the OpenAI Responses wire API
│   ├── AnthropicClient                   the Anthropic Messages wire API
│   ├── GoogleClient                      the Gemini generateContent wire API
│   ├── ClaudeCodeClient                  the Claude Code CLI as a client; tools via the outcome envelope
│   ├── Retry                             wrapper client; retries replay-safe failures
│   └── Fallback                          wrapper client; advances to the next member
├── wire                                  shared helpers for client authors
│   ├── send_as<T>(req, provider)         send, classify the status, decode the body as T
│   ├── render_output_format(type)        the ctx.output_format text in the standard dialect
│   └── closed_schema / strict_schema     per-API schema rewrites
└── errors                                the ai.errors namespace, mirroring baml.errors
    ├── Failure                           interface: retry_safety() -> RetrySafety
    ├── RetrySafety                       Safe | Unknown | Unsafe
    ├── RateLimited / NetworkFailure      classified provider failures
    ├── InvalidRequest / Refused / ParseFailed
    ├── StepBudgetExceeded                thrown when max_steps is exhausted
    ├── ToolFailedError                   thrown when a Raise-mode tool fails; the failure is journaled first
    ├── classify_http(provider, status, body)
    └── (baml.errors.UnknownError)        the untyped catch-all channel; wraps foreign throws
```

The `wire` helpers are conveniences over existing standard library
primitives. Client and runner authors also use those primitives
directly:

```
baml.schema
└── json_schema(t)                        BAML type -> JSON Schema; $defs/$ref for recursive classes
baml.sap
└── parse<T>(text)                        schema-aligned parse with repair; the runner's final parse
baml.json
├── parse(text) / stringify(j)            string <-> json
└── from_json<T>(j) / from_string<T>(s)   json or string -> typed value
baml.http
├── Request / Response                    method, url, headers, body; text(), ok()
├── send(request, timeout =)              one blocking call
└── fetch_sse(request) -> SseStream       server-sent events; streaming clients later
reflect
├── type_of<T>() / signature(f)           types and signatures as runtime values
└── call_any(f, args)                     validated dynamic call; how the runner executes tools
baml.env
└── get(name) / get_or_panic(name)        credential resolution in registry factories
```

`tool()` derives each `Tool.input_schema` with `baml.json.schema`
over the signature; `wire.render_output_format` lowers the same schema into
prompt text; `wire.send_as<T>` wraps `baml.http.send` with status
classification and typed decoding of the response body, with
`T = json` as the untyped form. A client that needs different behavior
drops down to the primitives.

## Reading order

The pages follow Diátaxis: a tutorial, topic guides, reference, and
explanation.

**Introduction.** `01_getting_started` (zero to agent, as a tutorial),
`02_why` (motivation), `03_concepts` (the vocabulary; read before the
guides).

**Guides.** In the order the concepts appear:
`01_functions/` (LLM functions, tools, calling),
`02_specs_and_runners/` (specs, the default runner, writing a runner),
`03_clients/` (model strings, the client interface, writing a client,
reliability, the built-in clients' wire mappings), `04_the_journal.md`.

**How-to.** One task per page: retry a failed parse with feedback,
test without a network, use a local model, observe a run with
`on_event`, attach MCP servers to Claude Code, use MCP tools with any
client. The section grows as recipes accumulate.

**Reference.** `01_api` (the tree above, expanded per item),
`02_events` (the event catalog), `03_errors` (the error catalog).

**Appendix.** `01_comparisons` (pi, Pydantic AI, OpenAI Agents SDK, and
the two earlier BAML designs), `02_alternatives_considered` (each
settled decision, the options weighed, and why),
`03_future_phases` (fidelity and streaming, continuations, remote
state, sessions).

`outline.md` lists every page and header. `style.md` states the prose
rules for pages in this BEP.
