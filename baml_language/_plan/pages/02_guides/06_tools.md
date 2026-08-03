# Tools

## A tool is a function

Any BAML function can be a tool. There is no decorator, no schema
definition, and no registration step. List the function in `tools:`:

```baml
/// Search available flights between two cities.
function search_flights(origin: string, dest: string) -> Flight[] {
    baml.json.from_string<Flight[]>(
        baml.http.fetch(`https://api.flights.dev/q?o=${origin}&d=${dest}`).text()
    )
}

function PlanTrip(trip_request: string) -> Itinerary {
    client: "openai/gpt-5.2"
    tools: [search_flights, search_hotels]
    prompt: `You are a travel agent. ${trip_request} ${ctx.transcript} ${ctx.output_format}`
}
```

Because tools are plain functions, they are reusable: the same function can
be a tool in three agents, a step in a workflow, and a unit under test.
Tools do not take any session-specific parameters. If a tool needs to
interact with the running session, it uses the ambient API described in
"Tools and the session" below — the signature stays clean.

## How the model sees tools

The runtime reads each tool's signature and docstring through reflection
(`reflect.signature`, BEP-062) and presents them to the model:

- The docstring is the tool description. Write it for the model.
- Parameter names and types become the argument schema.
- The return type tells the model what to expect back.

Tools listed statically in `tools:` additionally get typed call classes
generated into the function's output schema, which improves argument
accuracy. Tools added dynamically at runtime are described in the prompt
and validated at call time. Both forms can coexist.

## Argument validation

Model-supplied arguments are validated against the real signature before
the tool runs (`reflect.call_any`). If the model passes a wrong type or a
parameter that does not exist, the tool is never called. The validation
error is returned to the model as the tool result, and the model retries.

## Tool errors

A throw inside a tool does not crash the agent. The error becomes the tool
result, visible to the model, so it can retry or take another approach.
Throw or return a descriptive failure value; do not swallow errors the
model needs to see.

To handle total failure yourself, catch at the call site of the agent
function, not inside tools.

## Tools with state

Closures are function values, so a tool can capture what it needs. This is
the supported way to bind a tool to a user, a database handle, or any
per-session resource:

```baml
function booking_tools(user: User, db: Db) -> baml.session.Tool[] {
    [baml.session.tool(
        (hotel: string, nights: int) -> { db.reserve(user.id, hotel, nights) },
        name = "book_hotel",
    )]
}
```

Anonymous functions have no name of their own, so `baml.session.tool`
requires `name =` for closures. Tool names must be stable: they appear in
the journal, and replay depends on them.

A tools list accepts both forms. A named function goes in bare; its name,
schema, and description come from reflection. A `Tool` value — from
`baml.session.tool` for a closure, or from an MCP server — goes in as
is. The two mix in one list:

```baml
tools = [read_file, run_bash, skill_loader(skills)].concat(gh.tools())
```

Wrapping a named function in `baml.session.tool` is needed only to
rename it.

## Tools and the session

Tools sometimes need to record something on the session — progress, a
custom event, a checkpoint. Do not add parameters for this. Use the
ambient functions, which resolve the current session from the calling
context:

```baml
/// Replace the current todo list.
function set_todos(items: string[]) -> string {
    baml.session.emit(TodoUpdated { items: items });
    "ok"
}
```

- `baml.session.emit(event)` — append a custom event to the enclosing
  session's journal. Outside a session, this is a no-op.
- `baml.session.step(name, fn)` — run `fn` with a durable checkpoint (see
  `12_durability.md`). Outside a session, it just runs `fn`.

This keeps tool signatures clean and tools reusable in any context. The
same design is used by `log.info`, which also resolves its destination
ambiently.

## Dynamic toolboxes

The set of mounted tools can change during a session: an MCP server
connects, an approval unlocks a capability, a policy narrows what the
model may do. Tool changes are made by policies through commands
(`MountTools`, `UnmountTools` — see `10_policies.md`) and recorded in
the journal as `ToolsChanged` events. The `tools:` list on the function is
the initial toolbox, nothing more.
