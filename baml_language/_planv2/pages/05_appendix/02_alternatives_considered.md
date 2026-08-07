# Alternatives considered

Each section records one design decision: what was chosen, the options
considered, and the reasons. Guides state behavior; this page records
why the behavior is what it is.

## 1. One public `invoke` versus three public phases

The client interface is `id()` plus `invoke(ModelTurnInput) ->
ModelTurn`. The sessions draft exposed `render`, `invoke`, and
`ingest` as three public client methods.

Rejected: three public phases. The runner would orchestrate wire-level
objects it cannot interpret. A safe fallback that requires
re-rendering — a rejected continuation cursor in phase 3 — would need
the runner to understand a structured continuation error and call
`render` again, moving wire policy into the generic loop. Rendering
and ingestion remain pure functions inside each client, and tests
target them directly; they are not the stable interface.

Cost of the chosen option: phase-level conformance cannot be checked
from outside the client. The wire library keeps the internal phases
uniform in practice.

## 2. No `runner:` field in the function block

The function block carries `client:` and `tools:` and nothing about
how it runs.

Rejected: a `runner:` field. A function declaration is a static
template that must work as a plain call, and a runner is application
infrastructure — often stateful, configured where the application
runs, not where the function is declared. A block-level runner would
also hide the output type of a call site behind a declaration the
reader is not looking at. The explicit form is
`my_runner.run(f@spec(...))`.

## 3. `$` parameters are runner fields

A `$` name at a call site sets the matching field on the default
runner; `PlanTrip(r, $client = c)` is
`Agent { client: c }.run(PlanTrip@spec(...)).value`.

Rejected: a separate configuration namespace with its own semantics.
One desugar rule answers every question of the form "what does
`$x` do" — it does what the `Agent` field `x` does — and new runner
fields become call-site parameters without new rules. The cost is that
`$` parameters can only express what the default runner has a field
for; anything else uses a runner explicitly, which is the intended
pressure.

## 4. One client override

`$client` is the only way to change the client for a run.

Rejected: `with_client` on the spec. It says the same thing as the
runner field, so every reader would face two spellings of one setting
and every reviewer would ask which one wins. Rejected: mutation
through a registry API, as in the earlier ClientRegistry. Clients are
plain values, so deriving a variant is a spread expression
(`OpenAiClient { ...base, model: "gpt-5.5" }`) and needs no API.

## 5. Specs are immutable

`FunctionSpec` has getters only.

Rejected: rebinding methods (`with_client`, `with_tools`). A spec that
can be rewritten is a second place run configuration lives, and the v4
experience showed the failure mode: `with_provider` re-rendered the
prompt inside the task and silently discarded steering applied before
the swap. With getters only, one value describes the work, one value
describes how to run it, and only the second varies.

## 6. Tool errors report by default

A failed tool becomes the call's result; the model sees it and
adapts. Raising is opt-in per tool (`tool(f, on_error = Raise)`) or
per run (`$tool_errors = Raise`), the per-tool setting wins, and the
failure is journaled before the exception propagates.

Rejected: throwing by default. The model is the party that can recover
mid-run, and a throw discards the run's accumulated work for failures
as ordinary as a 404 from a search API. Rejected: report-only with no
raise path. Some failures are not recoverable by the model, and
forcing applications to encode "stop now" as a sentinel tool result
makes the model responsible for control flow it cannot see. Two levels
exist because they answer different questions — "this tool's failures
are never recoverable" versus "this caller wants fail-fast" — but they
share one enum and one precedence rule.

## 7. Materials, not renderings

`ModelTurnInput` carries the prompt template and the output type.

Rejected: a pre-rendered prompt and schema string. The
`${ctx.output_format}` text is a per-client dialect, the placement of
the output contract (prompt text, native schema field, reserved tool)
trades off against the request body, and tool schemas lower to
per-API shapes. The v4 providers demonstrate all three divergences.
Passing the type as a runtime value lets phase 2 add native
structured-output modes as client options with no interface change.

## 8. Structured content blocks versus string plus `raw_json`

An assistant turn is `ContentBlock[]`.

Rejected: `content: string` plus the raw response body, as in the
sessions draft's provisional design. A string cannot represent
parallel tool calls, so the tool loop cannot lower results as deltas;
the draft's OpenAI client compensated with an adapter-owned action
parser, which made agent semantics part of one client. A complete HTTP
response is also not valid model input; request echoes, headers, and
diagnostics do not belong in a transcript source. The narrow replay
data that same-provider fidelity needs arrives in phase 2 as an
optional capsule.

## 9. Journaled state versus provider-owned conversations

All run state lives in the journal; clients are stateless.

Rejected: v4's mutable `Conversation` with `begin`/`step`/`submit`.
The lifecycle forced every provider to hand-implement an atomicity
discipline, required ownership checks through delegate chains, made
wrapper providers wrap conversations as well as calls, and put a
serialization obligation on every provider for resume. With a
value-returning `invoke` and a runner that commits, the atomicity
contract is structural, and one client value serves concurrent runs.

## 10. The runner parses, the client normalizes

The client surfaces a final candidate; the runner runs
`baml.sap.parse<Out>`.

Rejected: clients parsing the output type, as v4's `step<T>` did. It
makes every client generic over the output type, duplicates parse
logic per provider, and couples wire adapters to the repair engine.
Rejected: the runner reading wire responses. The mechanism that
carries the answer (terminal text, a reserved tool call, a native
structured output) is wire knowledge. The split point is the canonical
candidate: the client knows where the answer is, the runner knows what
it must be.

## 11. Registry resolution versus a public service-descriptor interface

`"prefix/model"` resolves through a registry of factories; the
descriptor concept stays internal.

Rejected: a public descriptor interface like pi's `createProvider`.
It is a second interface to learn and version, and its value —
OpenAI-compatible reuse — is already delivered by a factory that
configures an existing codec. If model metadata (context windows,
pricing) later needs a home, a descriptor can be introduced behind
resolution without breaking any client.

## 12. SAP-first structured output

In this phase the built-in clients render the schema as prompt text
and the runner parses with repair. The choice is a named field:
`output_mode` ships with the single value `Sap`, and the native
mechanisms (OpenAI strict schemas, Anthropic `output_config`, Gemini
`responseJsonSchema`) arrive in phase 2 as `Native` and `Strict`
values of the same field.

Rejected: native-first. The v4 providers spent roughly 300 lines each
on native output modes and their interactions with tools — Gemini
cannot combine controlled generation with function calling and needs a
reserved-tool fallback — and none of it changes the interface. SAP
works uniformly on day one; native modes are additive accuracy and
cost improvements.

## 13. Wrapper clients for reliability

`Retry` and `Fallback` are clients that wrap clients.

Rejected: retry configuration on the runner. The runner cannot know
which failures are safe to resend or how a provider hints backoff;
the client layer can. Rejected: retry configuration inside each
provider client, as legacy `retry_policy` was. It duplicates policy
per provider and cannot compose with fallback. Wrapping one method is
trivial, and whole-run retries remain available as a wrapping runner,
at a different boundary, without overlap.

## 14. Optional capability interfaces versus a capabilities struct

Future optional features (streaming, background, continuations) are
separate interfaces discovered by `match`, with
`baml.errors.Unsupported` as the uniform rejection.

Rejected: a `capabilities()` struct of flags. A struct must be
versioned as capabilities appear, invites stale self-reports, and
answers "can it" without providing "how to". An interface is both the
declaration and the implementation surface, and the `match` pattern is
already established in the language.

## 15. Deterministic synthesized call ids

A wire API that sends no tool-call ids (Gemini) gets ids synthesized
from the turn's journal position and the block index.

Rejected: a counter in client or conversation state, as v4's Google
provider kept (`next_call_id`, serialized into resume tokens).
Clients are stateless, so the id must be a function of the input;
journal position makes the same turn produce the same ids on replay.

## 16. The prompt is the instructions; no transcript placeholder

The prompt template renders to one instructions string per turn, and
the journal lowers as messages after it. There is no
`${ctx.transcript}` placeholder, and the template does not control
where the conversation appears.

Rejected: a positional transcript marker in the template, as the
sessions draft had. At the wire level the position is fiction — chat
APIs do not interleave message history into prompt text — so the
marker's only real effect was a suffix rendered after the history.
That suffix works against prompt caching, because the cacheable static
prefix ends where the growing history begins, and its one use case —
a per-turn reminder placed after the history — is better served by
injected messages, which arrive as journal events when sessions do.
Dropping the marker also removes the `RenderedPrompt` before/after
split from the client surface; `Prompt.render` returns one
string. The cost is that a template author cannot place static content
after the conversation; a client that needs a trailing element on the
wire synthesizes it from the instructions.

## 17. Prompt-mode tools are a wrapper client, not a client mode

The built-in clients lower tools to native function calling only.
Prompt-mode tool calling — a tool catalog in text and a
`T | ToolCalls` union recognized in the reply — is a phase 2 wrapper
client (`PromptTools { inner: Client }`) that renders the protocol,
passes the inner client an empty toolbox, and rewrites a calls
envelope into `ToolUse` blocks.

Rejected: a `tool_mode` configuration flag on each client, as v4 had.
Under v4's provider-owned conversations the flag forced a private
prompt adapter inside every provider — roughly 250 lines copied three
times — and a flag on the v5 clients would still branch every client's
`invoke`. The v5 boundary is materials in, canonical turn out, so the
lowering is writable once and composes with any inner client,
including custom clients for APIs without function calling. Rejected:
omitting the capability entirely. Models with unreliable native tool
support are common on OpenAI-compatible local endpoints, and the
wrapper is the difference between supporting them and not. The cost is
the v4 ambiguity caveat: an output type that structurally overlaps the
calls envelope needs literal discriminators, and the wrapper ships in
phase 2 rather than now.

## 18. The `Runner` interface requires no fields

`Runner<Out>` declares the associated `Output` and `Error` types and
`run`, and nothing else. The `Error` type exists so that `run` never
throws an untyped error; each runner declares its union and callers
match it with typed arms.

Rejected: required properties on the interface (`max_steps`,
`client`, `on_event`) so that all runners share options. An
implementor can add members but cannot remove them, and no run option
is universal: a budget means nothing on a delegating wrapper, and a
client or event callback is ambiguous on a runner that owns several
runs. A required field an implementation ignores is a false promise.
Nothing consumes the uniformity either — `$` parameters desugar to the
default runner's constructor, not through the interface. The
convention instead is composition: a runner that drives the standard
loop embeds an `Agent`, keeping the common knobs under one vocabulary.
The cost is that option uniformity is a convention, not a compiler
guarantee.

## 19. No sessions in this BEP

There is no session object, steering channel, policy layer, or job
handle.

The sessions draft showed these are separable: they consume journal
events and drive runs, and nothing in specs, runners, or clients needs
to know they exist. Cutting them makes the first release reviewable
and implementable, at the cost of no long-lived conversations yet.
Re-entry requires journal persistence and a session runner; the
surface they build on is already stable
(`03_future_phases.md`).
