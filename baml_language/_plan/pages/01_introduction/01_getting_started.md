# Getting started

This page takes you from a typed LLM call to a multi-turn agent.

## A typed LLM call

An LLM function has a prompt for a body and a return type the model must
produce:

```baml
class Itinerary {
    destination: string,
    days: int,
    daily_plan: string[] @description("one entry per day"),
}

function PlanTrip(trip_request: string) -> Itinerary {
    client: "openai/gpt-5.2"
    prompt: `
        You are a travel agent. Plan this trip: ${trip_request}
        ${ctx.output_format}
    `
}
```

```baml
let trip = PlanTrip("2 weeks in Japan");
trip.days   // a typed int
```

`${ctx.output_format}` renders the return type's schema into the prompt.
The parser validates the model's output before your code sees it.

## Make it an agent

Add `tools:`. Tools are plain BAML functions; their signatures and
docstrings become the schema the model sees:

```baml
/// Search available flights between two cities.
function search_flights(origin: string, dest: string) -> Flight[] { /* ... */ }

/// Find hotels in a city under a nightly budget.
function search_hotels(city: string, max_nightly: float) -> Hotel[] { /* ... */ }

function PlanTrip(trip_request: string) -> Itinerary {
    client: "openai/gpt-5.2"
    tools: [search_flights, search_hotels]
    prompt: `
        You are a travel agent. Plan this trip: ${trip_request}
        ${ctx.transcript}
        ${ctx.output_format}
    `
}
```

The call site does not change:

```baml
let trip = PlanTrip("2 weeks in Japan");
```

The model now works in a loop — calling tools, reading results — until it
produces an `Itinerary`. This is a task: one call, one typed result.

## Make it a conversation

Open the same function as a session:

```baml
let s: Session<Itinerary> = PlanTrip@session(trip_request = "2 weeks in Japan");

match (s.run()) {
    let d: baml.session.Done<Itinerary> => print(d.result),
    let r: baml.session.Replied => print(r.message),   // the agent asked something
}

s.send("make it 10 days, skip Tokyo");
let turn2 = s.run();
```

Sessions persist. `s.snapshot()` returns a string you can store anywhere;
`PlanTrip@session($resume = snap)` continues it on any
machine.

## Run it from Python or TypeScript

```python
from baml_sdk import b

trip = b.PlanTrip("2 weeks in Japan")

s = b.session.PlanTrip.create(trip_request="2 weeks in Japan")
turn = s.run()
```

## Where to go next

- `02_why.md` — why agents are part of the language.
- `03_concepts.md` — the pieces and the vocabulary. Read before the guides.
- `../02_guides/01_agents.md` — agents and task mode in detail.
