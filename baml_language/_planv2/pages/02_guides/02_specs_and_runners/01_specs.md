# Specs

## What `@spec` creates

`MyFunc@spec(args)` binds the function's arguments and returns a
`FunctionSpec` value. No model call happens:

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

```baml
let spec: FunctionSpec<Itinerary> = PlanTrip@spec(trip_request = "2 weeks in Japan");
```

The type parameter is the function's return type, so a runner that
consumes the spec is typed by what the run produces.

## A spec is a recipe

A spec holds the function's identity, the bound arguments, the prompt
template, the initial toolbox, and the resolved default client. It
holds no journal, no conversation, and no wire state, and it can be
run any number of times; each run is independent.

The prompt is not pre-rendered into the spec. Rendering happens on
every model turn, inside the client, from the template plus the run's
journal. A spec therefore renders correctly under any client, and a
client switch between runs needs no conversion step.

## Reading a spec

```baml
spec.name()          // "PlanTrip"
spec.arguments()     // { "trip_request": "2 weeks in Japan" }
spec.output_type()   // the Itinerary type, as a runtime value
spec.prompt()        // a freshly rendered ai.Prompt (empty output-format text)
spec.prompt(output_format = schema) // render with schema text at ${ctx.output_format}
spec.tools()         // the initial Toolbox
spec.default_client  // the resolved default Client
```

`output_type()` returns a runtime type value, usable with
`baml.json.schema` and `baml.sap.parse`. `client` is a keyword
and cannot be a method name, so the resolved default client is the
`default_client` field.

## Specs are read-only

A spec has getters and nothing else. There is no `with_client`, no
`with_tools`, and no setter. The getters return the spec's actual
values, and BAML classes are reference values, so mutating a returned
value (`spec.tools().list().pop()`) is expressible — it is unsupported,
and a later revision may return copies or freeze the spec to close the
gap. Every override lives on the runner that consumes the spec:

```baml
let result: RunResult<Itinerary> = ai.Agent<Itinerary>
    .new(client = other_client, max_steps = 20)
    .run(spec);
```

One value describes the work and one value describes how to run it,
and only the second varies between runs.

## What specs are for

Ordinary code never sees a spec; the plain call form creates and
consumes one invisibly. Specs exist for the code around runs:

- Custom runners take a spec as input
  (`03_writing_a_runner.md`).
- Evals run one spec across several clients and compare results.
- Serving and registry layers, when they arrive in a later phase,
  enumerate specs as the deployable units.
