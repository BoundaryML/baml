# LLM functions

## An LLM function is a typed function

An LLM function declares parameters, a return type, a client, and a
prompt:

```baml
class Itinerary {
    flights: Flight[],
    hotels: Hotel[],
    total_cost_usd: float,
}

function PlanTrip(trip_request: string) -> Itinerary {
    client: "openai/gpt-5.6"
    prompt: `
        You are a travel agent. The brief: ${trip_request}
        ${ctx.output_format}
    `
}
```

The parameters interpolate into the prompt. The return type is the
contract the model must satisfy. The `client:` field names the default
model as a string; `../03_clients/01_choosing_a_model.md` explains how
the string resolves to a client value.

An LLM function is the only place BAML talks to a model. Code that
reaches a provider through raw HTTP records no journal and gets no
typed parsing.

## The prompt is the instructions

The prompt template renders to one instructions string, fresh on every
model turn, and the conversation so far lowers as messages after it.
In a chat wire API the rendered prompt is the system prompt. There is
no placeholder for the conversation; its position on the wire is the
client's decision, not the template's
(`../../05_appendix/02_alternatives_considered.md`).

One placeholder exists:

- `${ctx.output_format}` — the return type's schema, rendered as text
  in the client's dialect. Include it wherever the model must produce
  the return type.

On the first turn of a run there is no conversation yet. A wire API
that requires a user message may receive the instructions as the sole
user message on that turn, and a wire API may require the instructions
as the leading user content on every turn, as Gemini does; the mapping
belongs to the client
(`../03_clients/02_the_client_interface.md`).

## The return type is the contract

The model's output is parsed, validated, and repaired against the
return type before your code sees it. Malformed output triggers repair
attempts within the same model turn, under the turn's attempt budget;
output that cannot be repaired fails the turn with `ParseFailed`. Your
code receives an `Itinerary` or an exception, never a string to check.

Union return types work: a function typed
`-> Itinerary | CannotPlan` returns whichever variant the model
produced, and the schema shows the model both.

## Media arguments and outputs

A parameter of a media type (`image`, `audio`, `pdf`, `video`)
interpolates into the prompt as a media part, not as text. The
instructions render as a part list, and the client lowers each media
part in its wire API's format
(`../03_clients/05_the_built_in_clients.md`).

A media return type works differently, because image data cannot ride
a JSON schema. When the return type is exactly `image` or `image[]`,
the final value is the turn's media output rather than parsed text;
this binding is a phase 2 capability of clients whose wire APIs
produce images (`../../05_appendix/03_future_phases.md`). A media type
nested inside a class or union with other data is rejected at spec
creation with `baml.errors.Unsupported`, because no provider protocol
can bind image data into a field of a JSON object.

## Defaults in the function block

The function block carries per-function defaults:

- `client:` — the default client, as a model string.
- `tools:` — the initial tool list (`02_tools.md`).

Both apply to every call of the function. Per-call overrides use `$`
parameters at the call site (`03_calling_functions.md`). The block
accepts no runner, no budgets, and no callbacks; those belong to the
caller, not the function.
