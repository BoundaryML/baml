# Subagents

## Calling an agent from an agent

A subagent is an agent function used by another agent. There is no special
declaration. Two patterns:

**As a tool.** An LLM function is a function, so it goes in `tools:`
directly. The parent's model decides when to delegate:

```baml
/// Research one city in depth for a traveler.
function ResearchCity(city: string) -> CityGuide {
    client: "anthropic/claude-sonnet-5"
    tools: [search_web]
    prompt: `Research ${city}. ${ctx.transcript} ${ctx.output_format}`
}

function PlanTrip(trip_request: string) -> Itinerary {
    client: "openai/gpt-5.2"
    tools: [search_flights, ResearchCity]      // an agent, listed like any tool
    prompt: `
        You are a travel agent. The brief: ${trip_request}
        ${ctx.transcript}
        ${ctx.output_format}
    `
}
```

The docstring is the delegation description, the signature is the
argument schema, and the return type is typed all the way through: the
parent's model is told the call returns a `CityGuide`, and the result
enters the parent's transcript as one, not as an opaque string.

Wrap an agent in a plain function only when you actually need to reshape
the interface — rename it, fix some arguments, or trim the result:

```baml
/// Research a city, keeping only what matters for a short trip.
function quick_research(city: string) -> string {
    ResearchCity(city).highlights.join("; ")
}
```

**As code.** Your program decides when to delegate:

```baml
let guides = await baml.future.all(cities.map((c) -> { spawn { ResearchCity(c) } }));
```

## Child sessions

A subagent runs in its own child session with its own journal. The child
starts fresh: it sees its arguments and its own conversation, not the
parent's. Only its final result returns to the parent.

The parent journal records the boundary: `ChildSpawned { child_id, goal }`
when the child starts, `ChildFinished { child_id, result_json }` when it
returns. The child's full journal is stored under its own ID and linked by
`child_id`, so a trace viewer can expand it in place.

This gives each delegation an isolated context window, and keeps every
journal bounded: a parent with fifty delegations records a hundred
boundary events, not fifty transcripts.

## Concurrency

Subagents are ordinary calls, so ordinary concurrency applies:

```baml
let g = baml.spawn.TaskGroup.new(4);                 // at most 4 at once
let futures = cities.map((c) -> {
    spawn with baml.spawn.options(group = g) { ResearchCity(c) }
});
let guides = await baml.future.all(futures);
```

Tool calls issued by the model in one turn also run concurrently,
including tool calls that are subagents.

## Cancellation

Cancellation flows down the session tree. `s.interrupt(...)` on the parent
cancels in-flight tools and every running child, recursively, through
their cancel tokens. Each cancelled session records `Interrupted` in its
own journal. A child cannot outlive its parent's interrupt.
