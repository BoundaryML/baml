# Choosing a model

## Model strings

The `client:` field and `$client` accept a model string:

```baml
function PlanTrip(trip_request: string) -> Itinerary {
    client: "openai/gpt-5.6"
    tools: [search_flights, search_hotels]
    prompt: `
        You are a travel agent. The brief: ${trip_request}
        ${ctx.output_format}
    `
}
```

The string splits at the first `/`. The prefix names a client
implementation; the remainder configures it.

## Resolution

`ai.clients.resolve` turns a model string into a client value.
The registry maps each prefix to a factory:

```baml
let c: Client = ai.clients.resolve("openai/gpt-5.6");
// resolves to:
// OpenAiClient { model: "gpt-5.6", api_key: baml.env.get_or_panic("OPENAI_API_KEY") }
```

The built-in prefixes and their credentials:

| Prefix | Client | Wire API | Credential |
|---|---|---|---|
| `openai` | `OpenAiClient` | OpenAI Responses | `OPENAI_API_KEY` |
| `anthropic` | `AnthropicClient` | Anthropic Messages | `ANTHROPIC_API_KEY` |
| `google` | `GoogleClient` | Gemini `generateContent` | `GOOGLE_API_KEY` |

A `client:` field resolves when the spec is created, so a missing
credential fails at the call site, before any model turn.

In the reference implementation `resolve` lives in the application
root rather than under `ai.clients`, so the core namespace never
depends on the provider clients.

## Same implementation, different model

`"openai/gpt-5.6"` and `"openai/gpt-5.5"` resolve to the same class
with a different `model` field. There is one client implementation per
wire API, not one per model. A new model on an existing provider
requires no code.

## Registering a prefix

`clients.register` makes a new prefix resolvable:

```baml
ai.clients.register("ollama", (model: string) -> Client {
    OpenAiClient.new(
        model = model,
        base_url = "http://localhost:11434/v1",
        api_key = "",
    )
});
```

```baml
function PlanTrip(trip_request: string) -> Itinerary {
    client: "ollama/qwen3:32b"
    ...
}
```

An OpenAI-compatible endpoint is configuration over the existing
codec, not a new implementation. A genuinely new wire API implements
the `Client` interface (`03_writing_a_client.md`) and registers the
same way. Registration happens at application startup; a prefix
registered twice throws. The reference implementation does not yet
provide `register`, because a registry requires process-global state.

## The one override

`$client` at the call site is the single way to override the
function's default:

```baml
let trip: Itinerary = PlanTrip("2 weeks in Japan", $client = cheap);
```

`Agent { client: ... }` in the desugared form is the same setting at
the explicit layer, not a second mechanism. Specs have no rebinding
methods.

## Constructing and deriving clients

Clients are plain values, and `resolve` is a convenience for
constructing one. `new` is the same thing with every parameter
defaulted, including the credential from the environment:

```baml
let direct: Client = GoogleClient.new(model = "gemini-2.5-flash");
```

A class literal gives full control with no defaults:

```baml
let explicit: Client = GoogleClient {
    model: "gemini-2.5-flash",
    api_key: baml.env.get_or_panic("GOOGLE_API_KEY"),
    output_mode: OutputMode.Sap,
};
```

To change one option on an existing client, spread it into a new one;
the source client is unchanged:

```baml
let base: Client = ai.clients.resolve("openai/gpt-5.6");
let older = OpenAiClient { ...base, model: "gpt-5.5" };
let via_proxy = OpenAiClient { ...base, base_url: "https://llm-proxy.internal/v1" };
```

There is no separate client-registry mutation API. The registry
resolves strings; everything else is ordinary values and ordinary
construction.
