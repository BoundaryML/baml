# Use a local model

An OpenAI-compatible server (Ollama, vLLM, LM Studio) reuses the
OpenAI codec with different configuration. No client code is needed.

To make the model available as a string, register a prefix once at
application startup (`register` is designed surface; the reference
implementation does not provide it yet, so use the client-value form
below there):

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
    tools: [search_flights, search_hotels]
    prompt: `
        You are a travel agent. The brief: ${trip_request}
        ${ctx.output_format}
    `
}
```

For one call site, skip the registry and pass a client value:

```baml
let local: Client = OpenAiClient.new(
    model = "qwen3:32b",
    base_url = "http://localhost:11434/v1",
    api_key = "",
);
let trip: Itinerary = PlanTrip("2 weeks in Japan", $client = local);
```

Small local models are the main audience for the phase 2 `PromptTools`
wrapper, which replaces unreliable native tool calling with a prompt
protocol (`../02_guides/03_clients/05_the_built_in_clients.md`).
Resolution and registration details are
`../02_guides/03_clients/01_choosing_a_model.md`.
