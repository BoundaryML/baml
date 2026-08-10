# Tools

## The `tools:` field

A `tools:` list turns an LLM function into an agent. There is no
separate agent declaration:

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

During a run the model may call the listed tools any number of times.
The run ends when a turn produces the return type instead of tool
calls.

`tools:` takes an expression producing the tool list — usually a
literal list of functions and `Tool` values, but any expression works,
such as an MCP server's catalog
(`../../03_how_to/06_use_mcp_tools_with_any_client.md`). It is
evaluated once, when the spec is created; the toolset is fixed for the
spec's runs.

## A tool is a function

Any BAML function can be a tool. Its name, docstring, and signature
become the schema the model sees:

```baml
/// Search available flights for a route and month.
function search_flights(origin: string, destination: string, month: string) -> Flight[] {
    flight_api.search(origin, destination, month)
}
```


The schema derives through reflection: `reflect.signature` reads the
name, docstring, parameters, and defaults; positional parameters are
required, defaulted parameters are optional, and parameter types lower
through `baml.json.schema`. The signature is simultaneously the
compile-time check, the schema, and the validator; nothing is declared
twice.

The explicit constructor covers the cases the signature cannot
express:

```baml
let renamed: Tool = ai.tools.tool(
    search_flights,
    name = "find_flights",
    description = "Search flights. Prefer direct routes.",
);
```

## Argument validation

The model's arguments are validated against the schema before the tool
runs. A call with missing, extra, or mistyped arguments does not reach
your function; it produces a tool error that the model sees and can
correct on its next turn. Validation failures are never application
exceptions. The reflection call boundary widens an exactly
representable integral JSON number into a `float`
parameter before dispatch, because models emit `150` for `150.0` and
JSON Schema `number` accepts integers.

## Tool errors are data

A tool that throws does not fail the run. The failure becomes a
`ToolFailed` event, and the model receives it as the call's result:

```baml
/// Search hotels in a city within a nightly budget.
function search_hotels(city: string, max_nightly_usd: float) -> Hotel[] {
    if (max_nightly_usd <= 0.0) {
        throw baml.errors.InvalidArgument { message: "budget must be positive" }
    }
    hotel_api.search(city, max_nightly_usd)
}
```

The model is the party that can adapt to a failed tool — retry with
different arguments, use another tool, or report the problem in its
answer — so the failure flows back into the conversation rather than
into the application's `throws` channel.

## Tool failure policy

Some failures are not recoverable by the model. The `on_error` option
selects what happens when a tool fails:

- `Report` (the default) — the failure becomes the call's result, as
  above.
- `Raise` — the failure is journaled as `ToolFailed`, then the run
  throws `ToolFailedError` carrying the call and the cause.

Set it per tool at declaration:

```baml
tools: [search_flights, ai.tools.tool(charge_card, on_error = Raise)]
```

Or for every tool in a run, at the call site:

```baml
let trip: Itinerary = PlanTrip(request, $tool_errors = Raise);
```

The `on_error` parameter is `ErrorMode?` and defaults to null. A
null value inherits the run's `$tool_errors` mode, and an explicitly
set per-tool value wins over the run-wide one. The journal records
the failure before the exception propagates, so the trace shows what
happened regardless of the policy.

## Parallel calls

A single model turn may request several tool calls. The runner
executes them concurrently and correlates each result to its call by
id. Results may complete in any order; the journal records completion
order, and the client lowers the set of results per its wire API's
rules. A tool that must not run concurrently with itself serializes in
its own body.

## Changing the toolbox

The `tools:` list is the function's initial toolbox and is static.
To run the same function with a different toolbox, use a custom runner
and construct the toolbox yourself
(`../02_specs_and_runners/03_writing_a_runner.md`). Mid-run toolbox
changes are out of scope for this BEP; the policy layer that owns them
arrives with sessions (`../../05_appendix/03_future_phases.md`).
