# Getting started

This page is a tutorial. It goes from one typed model call to an agent
with tools, and shows what every call records. The other introduction
pages explain why the system is shaped this way (`02_why.md`) and
define its vocabulary (`03_concepts.md`).

## A typed LLM call

An LLM function has a prompt for a body and a return type that the
model must produce:

```baml
class Recipe {
    title: string,
    ingredients: string[],
    steps: string[],
}

function ExtractRecipe(text: string) -> Recipe {
    client: "openai/gpt-5.6"
    prompt: `
        Extract the recipe from this text.
        ${text}
        ${ctx.output_format}
    `
}
```

Calling it looks like calling any function:

```baml
let recipe: Recipe = ExtractRecipe(page_text);
```

The return type is the schema. `${ctx.output_format}` renders it into
the prompt, and the parser validates and repairs the model's output
before your code sees it. There is no separate response object to
unwrap; a failed call throws.

## Make it an agent

Add a `tools:` list and the function works in a loop until it produces
the return type:

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

/// Search hotels in a city within a nightly budget.
function search_hotels(city: string, max_nightly_usd: float) -> Hotel[] {
    hotel_api.search(city, max_nightly_usd)
}

function PlanTrip(trip_request: string) -> Itinerary {
    client: "openai/gpt-5.6"
    tools: [search_flights, search_hotels]
    prompt: `
        You are a travel agent. The brief: ${trip_request}
        ${ctx.output_format}
    `
}
```

```baml
let trip: Itinerary = PlanTrip("2 weeks in Japan, mid-range budget");
```

The tools are plain BAML functions. Their signatures and docstrings
become the schemas the model sees, and their results flow back into
the conversation until the model produces an `Itinerary`.

## What a call does

A call is sugar for three steps: bind the arguments into a spec, run
the spec with the default runner, and unwrap the value.

```baml
let trip: Itinerary = PlanTrip("2 weeks in Japan");
// is the same as:
let result: RunResult<Itinerary> = ai.Agent<Itinerary>
    .new()
    .run(PlanTrip@spec(trip_request = "2 weeks in Japan"));
let trip: Itinerary = result.value;
```

A function without `tools:` runs the same loop and completes on its
first model turn. The desugared form is not implemented yet; until it
is, write it manually where you need it.

## Inspect the run

The explicit form returns a `RunResult`, which carries the journal —
the complete typed record of the run — and the token usage:

```baml
let result: RunResult<Itinerary> = ai.Agent<Itinerary>
    .new()
    .run(PlanTrip@spec(trip_request = "2 weeks in Japan"));

for (let e in result.journal.entries()) {
    match (e) {
        let t: ToolRequested => log.info(`called ${t.name}`),
        _ => null,
    }
}
log.info(`tokens: ${result.usage.input_tokens} in, ${result.usage.output_tokens} out`);
```

`../02_guides/04_the_journal.md` lists every event a run records.

## Point it at another model

The `client:` field names a default. Override it per call with
`$client`:

```baml
let cheap: Client = ai.clients.resolve("google/gemini-2.5-flash");
let trip: Itinerary = PlanTrip("2 weeks in Japan", $client = cheap);
```

A client is a plain value, and `resolve` is a convenience for
constructing one from a string. Constructing it yourself is the same
thing:

```baml
let local: Client = OpenAiClient.new(
    model = "qwen3:32b",
    base_url = "http://localhost:11434/v1",
    api_key = "",
);
let trip: Itinerary = PlanTrip("2 weeks in Japan", $client = local);
```

`"openai/gpt-5.6"` and `"openai/gpt-5.5"` resolve to the same client
implementation with a different model field.
`../02_guides/03_clients/01_choosing_a_model.md` explains resolution
and how to register your own prefix.

## Where to go next

- `../02_guides/01_functions/01_llm_functions.md` — the function form
  in full.
- `../02_guides/01_functions/02_tools.md` — tool declaration,
  validation, and failure policy.
- `../02_guides/02_specs_and_runners/02_the_default_runner.md` — the
  loop in detail.
- `../02_guides/03_clients/03_writing_a_client.md` — adding a
  provider.
