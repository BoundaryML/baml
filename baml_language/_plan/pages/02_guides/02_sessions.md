# Sessions

## Creating a session

`Func@session(...)` creates a session from any LLM function. It takes the
same arguments as the function, type-checked the same way:

```baml
let s = PlanTrip@session(request = "2 weeks in Japan, mid-range");
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
let s = PlanTrip@session(request = "2 weeks in Japan");
match (s.run()) {
    let d: baml.session.Done<Itinerary> => print(d.result),
    let r: baml.session.Said => print(r.message),
}

s.send("make it 10 days");
let turn2 = s.run();
```

The first `run()` needs no `send` if the arguments already give the model
something to do.

`run()` returns a union:

```baml
type Turn<T> = Done<T> | Said

class Done<T> { result: T }        // the agent produced the final typed answer
class Said    { message: string }  // the agent replied and is waiting for input
```

This is the difference between task mode and session mode: in a task,
`Said` is not a legal stopping point; in a session, it is. Errors (step
budget, provider failure) throw, the same as task mode.

## Snapshots

A session serializes to a single string:

```baml
let snap = s.snapshot();
```

The snapshot is the journal: the arguments, every event, everything needed
to continue on any machine with any provider.

```baml
let s = PlanTrip@session(resume = snap);
```

`resume` accepts `string?`; `null` starts a fresh session. Arguments come
from the snapshot — passing arguments together with a non-null `resume`
is an error.

The stateless server pattern is three lines:

```baml
function handle_turn(snap: string?, msg: string) -> string {
    let s = PlanTrip@session(resume = snap);
    s.send(msg);
    let reply = match (s.run()) {
        let d: baml.session.Done<Itinerary> => baml.json.to_string(d.result),
        let r: baml.session.Said => r.message,
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
let s = PlanTrip@session(id = `issue-${n}`);              // get or create
let s = PlanTrip@session(id = `issue-${n}`, new = true);  // create only
```

With `id`, the runtime loads the session from the configured journal store
(`10_journal.md`), or creates it. With `new = true`, creation fails with
`baml.session.InstanceExists` if the ID is taken — use this when the
creator must be unique, such as a webhook that must not double-create.

An ID names exactly one journal. Two calls with the same ID talk to the
same conversation.
