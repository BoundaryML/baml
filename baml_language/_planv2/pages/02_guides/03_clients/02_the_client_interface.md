# The client interface

## One operation

```baml
interface Client {
    function id(self) -> string throws never
    function invoke(self, input: ModelTurnInput) -> ModelTurn
        throws Failure | baml.errors.UnknownError
}
```

`id` names the client for events and traces (`"openai/gpt-5.6"`).
`invoke` performs exactly one model turn: it renders the input into
the client's wire format, makes the call, and returns the result as
canonical content. A failed `invoke` throws; a thrown turn produced no
value, and the runner commits nothing.

`Failure` is the classified-error interface, and in a `throws`
position it stands for every class that implements it — the built-in
vocabulary and any custom failure alike (`04_reliability.md`).
Anything a client throws that is not a `Failure` reaches the caller
wrapped as `baml.errors.UnknownError`, so the channel is closed: no
signature in this BEP throws an untyped error.

Rendering and response parsing exist inside every client as pure
functions, and tests target them directly. They are not part of the
interface. Keeping `invoke` as the single public operation lets a
client re-render internally — which server-side continuations require
in a later phase — without the runner ever seeing a wire request.

## `ModelTurnInput`

```baml
class ModelTurnInput {
    prompt: Prompt,    // the function's template with arguments bound; not rendered
    journal: Journal,        // the run so far; the transcript source
    toolbox: Toolbox,        // the active tools
    output_type: type,       // the return type as a runtime type value
}
```

The input carries materials, not renderings. The prompt arrives as a
template and the contract arrives as a type, because what the model
should see is a per-client decision. A pre-rendered prompt or schema
string would bake one client's dialect into every other client's
request.

## The client owns the transformation

Four decisions belong to the client, and different wire APIs make them
differently:

- **Output-contract placement.** The schema can render into the prompt
  as text (`prompt.render(wire.render_output_format(input.output_type))`)
  or travel in the request body while the prompt's schema slot renders
  empty. In this phase the built-in clients render it as text; native
  body placement arrives as later values of their `output_mode` field
  (`05_the_built_in_clients.md`).
- **Output-format dialect.** The text at `${ctx.output_format}` is the
  client's rendering of the schema, not a fixed string.
- **Tool lowering.** Tool schemas lower to the wire API's shape: flat
  function objects, `input_schema` blocks, or `functionDeclarations`.
- **Transcript lowering.** Journal events lower to native messages
  after the instructions: assistant turns from content blocks, tool
  results by call id in the API's result format, roles per the API's
  role set. On a first turn with an empty journal, an API that
  requires a user message may receive the instructions as the sole
  user message, and a wire API may require the instructions as the
  leading user content on every turn, as Gemini does
  (`05_the_built_in_clients.md`).

The concrete mappings for the built-in clients, with the wire bodies,
are `05_the_built_in_clients.md`.

## `ModelTurn` and content blocks

```baml
type ContentBlock = Text | Reasoning | ToolUse | Media

class Text      { text: string }
class Reasoning { summary: string }                   // the readable projection only
class ToolUse   { id: string, name: string, args: map<string, unknown> }
class Media     { value: image | audio }              // inline model output, normalized

enum StopReason { Complete, ToolUse, MaxTokens, Refused }

class ModelTurn {
    content: ContentBlock[],
    stop_reason: StopReason,
    usage: Usage?,
}
```

A turn is structured content, never a bare string and never the HTTP
envelope. When `stop_reason` is `Complete`, the turn's terminal `Text`
block is the final candidate the runner parses. When it is `ToolUse`,
every requested call appears as a `ToolUse` block with a unique id.
A wire API that sends no call ids (Gemini) gets deterministic
synthesized ids from the turn's position in the journal. A `Media`
block carries inline model output such as a generated image; the
built-in clients target text models in this phase and produce it only
when the API returns inline media, and binding media to the return
type is a phase 2 rule (`../../05_appendix/03_future_phases.md`).

## Statelessness

A client holds configuration and nothing else. Every request is built
fresh from the input, so one client value serves any number of
concurrent runs, a failed turn can be retried by calling `invoke`
again, and no ownership checks tie a run to the client instance that
started it. Statelessness is also what makes the later phases
additive: replay data and continuation cursors attach to journal
entries, not to client objects
(`../../05_appendix/03_future_phases.md`).

## What a client never does

A client never writes the journal, never executes a tool, never parses
the return type, and never decides whether the loop continues. Those
belong to the runner. A client also never retries across turns;
`Retry` and `Fallback` are wrapper clients that compose at this
boundary (`04_reliability.md`).
