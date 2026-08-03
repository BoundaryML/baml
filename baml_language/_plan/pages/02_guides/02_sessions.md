# Sessions

## Creating a session

`Func@session(...)` creates a session from any LLM function. It takes the
same arguments as the function, type-checked the same way:

```baml
function PlanTrip(trip_request: string) -> Itinerary {
    client: "openai/gpt-5.2"
    tools: [search_flights, search_hotels]
    prompt: `
        You are a travel agent. The brief: ${trip_request}
        ${ctx.transcript}
        ${ctx.output_format}
    `
}

let s = PlanTrip@session(trip_request = "2 weeks in Japan, mid-range");
// s : Session<Itinerary>
```

## Arguments are session constants

The function's arguments are bound once, at creation, and render into the
prompt template on every turn. Use arguments for the standing brief: the
goal, the user profile, configuration.

Arguments are recorded in the journal's first event, `SessionStarted`, so
snapshots carry them and you never pass them again.

## Messages are events

Conversation arrives through `send`, not through arguments:

```baml
s.send("actually, skip Tokyo");
```

`send` appends a `UserMessage` event to the journal. It never blocks and
never calls the model. The string form is shorthand for
`s.send(baml.session.user("..."))`; the `Message` type exists for richer
content.

The two channels do not mix: arguments are constants rendered by the
template; messages are events rendered by `${ctx.transcript}`.

## Running turns

`run()` advances the session until the agent finishes or stops to wait
for input:

```baml
let s = PlanTrip@session(trip_request = "2 weeks in Japan");
match (s.run()) {
    let d: baml.session.Done<Itinerary> => print(d.result),
    let r: baml.session.Replied => print(r.message),
}

s.send("make it 10 days");
let turn2 = s.run();
```

The first `run()` needs no `send` if the arguments already give the model
something to do.

`run()` returns a union:

```baml
type Turn<T> = Done<T> | Replied

class Done<T> { result: T }        // the agent produced the final typed answer
class Replied    { message: string }  // the agent replied and is waiting for input
```

This is the difference between task mode and session mode: in a task,
`Replied` is not a legal stopping point; in a session, it is. Errors (step
budget, provider failure) throw, the same as task mode.

## Configuring a session

Configuration travels as `$`-prefixed parameters next to the function's
arguments, and behavior settings remain changeable mid-run through
setters:

```baml
let s = PlanTrip@session(trip_request = "2 weeks in Japan", $policy = my_policy);
s.set_client(cheap_client);        // takes effect on the next model call; journaled
```

Function parameters cannot start with `$`, so the namespaces never
collide. `03_configuration.md` covers the full set of `$` parameters,
setters, per-turn knobs on `run()`, and precedence.

## Snapshots

A session serializes to a single string:

```baml
let snap = s.snapshot();
```

The snapshot is the journal: the arguments, every event, everything needed
to continue on any machine with any provider. Resuming passes no function
arguments — they come from the snapshot:

```baml
let s = PlanTrip@session($resume = snap);
```

The stateless server pattern branches once, on the first turn:

```baml
function handle_turn(snap: string?, msg: string) -> string {
    let s = if let x: string = snap {
        PlanTrip@session($resume = x)
    } else {
        PlanTrip@session(trip_request = msg)          // first turn: args, no snapshot
    };
    s.send(msg);
    let reply = match (s.run()) {
        let d: baml.session.Done<Itinerary> => baml.json.to_string(d.result),
        let r: baml.session.Replied => r.message,
    };
    baml.json.to_string({ "reply": reply, "snapshot": s.snapshot() })
}
```

If your process stays alive, keep the session in memory and skip the
snapshot round-trip. In-memory and stateless are the same session.

## Named instances

Sessions can be addressed by ID instead of carried as values — one session
per ticket, per user, per order:

```baml
// get or create: args are used on create, checked against the journal on attach
let s = PlanTrip@session(trip_request = brief, $id = `issue-${n}`);

// create only: throws InstanceExists if the ID is taken
let s = PlanTrip@session(trip_request = brief, $id = `issue-${n}`, $new = true);
```

With `id`, the runtime loads the session from the configured journal store
(`11_journal.md`), or creates it with the given arguments. Use
`new = true` when the creator must be unique, such as a webhook that must
not double-create.

An ID names exactly one journal. Two calls with the same ID talk to the
same conversation.
