# Configuration

This page covers how to configure a task, session, or job: at creation,
while it runs, and per turn.

## Three kinds of configuration

| Kind | When it applies | Mechanism |
|---|---|---|
| Identity and kind | Fixed at creation | `$` parameters: `$runner`, `$store`, `$id`, `$new`, `$resume` |
| Behavior | Any time, including mid-run | `$` parameters for the initial value; setters after |
| Per-turn knobs | One `run()` call | Arguments to `run()` |

## `$` parameters

Configuration parameters share the call parentheses with the function's
arguments and are distinguished by a `$` prefix. Bare names go to the
function; `$` names go to the runtime. Function parameters cannot start
with `$`, so the two namespaces never collide.

```baml
//# a plain call with a budget
let trip = PlanTrip("2 weeks in Japan", $max_steps = 20);

//# a session with a policy and a specific client
let s = PlanTrip@session(
    trip_request = "2 weeks in Japan",     // PlanTrip's own parameter
    $policy = approval_policy,
    $client = fallback_client,
);

//# a background job: the runner changes the handle type
let job: Job<Itinerary> = PlanTrip@session(
    trip_request = "3 weeks across South America",
    $runner = jobs,
    $id = "trip-9421",
);

//# resume: arguments come from the snapshot, so only $resume appears
let s2 = PlanTrip@session($resume = snap);
```

`$` parameters are always passed by name.

| Parameter | Meaning | Fixed at creation |
|---|---|---|
| `$runner` | The kind of run; determines the handle type (`13_serving.md`, `../05_appendix/02_alternatives_considered.md` §1) | yes |
| `$store` | The journal store for named instances | yes |
| `$id` / `$new` | Instance identity; `$new = true` is create-only | yes |
| `$resume` | Snapshot to continue from; `null` starts fresh | yes |
| `$client` | Initial client, overriding the function's `client:` | no |
| `$policy` | Initial policy, replacing the default `ToolLoop` | no |
| `$tools` | Initial toolbox, when the function declares none or to extend it | no |
| `$max_steps` | Default step budget for runs | no |

The first four are identity: they say what this run is and where it
lives, and they cannot change afterward. The rest are initial values for
settings that stay changeable.

## Changing configuration mid-run

Behavior settings have setters, usable before the first `run()` or
between runs:

```baml
let s = PlanTrip@session(trip_request = "2 weeks in Japan");
s.set_client(cheap_client);        // takes effect on the next model call
s.set_policy(approval_policy);     // policy state is rebuilt from the journal
let t1 = s.run();

s.set_client(strong_client);       // mid-conversation provider switch
let t2 = s.run();
```

Every setter appends an event: `ClientChanged` or `PolicyChanged`
(`11_journal.md`). Configuration history is part of the record, so
"which client produced turn 3" is a journal query, and a resumed session
reconstructs the same configuration by folding the same events.

Tool changes are not a setter. Tools change through policy commands
(`MountTools`, `UnmountTools`), which record cause as well as effect
(`10_policies.md`).

Setters live on the handle, not on the runner, because their scope is
one session. A runner is shared infrastructure and may be driving many
sessions in parallel; changing it changes all of them. The rule: a
per-session setting is set on the session and journaled in that
session's journal; a fleet setting — worker cap, store, lease timing —
is set on the runner and applies to every session it drives. A session's
runner itself cannot change after creation; it is identity, like `$id`.

## Per-turn knobs

`run()` accepts knobs that apply to that call only:

```baml
let turn = s.run(max_steps = 30);   // this turn may use up to 30 model calls
```

## Precedence

For a changeable setting, the latest write wins, in journal order:

1. The function block (`client:`, and defaults such as `max_steps: 20`).
2. `$` parameters at creation.
3. Setters, in the order they were called.
4. `run()` arguments, for that call only.

## Defaults in the function block

A function can declare defaults for its own runs next to `client:`:

```baml
function PlanTrip(trip_request: string) -> Itinerary {
    client: "openai/gpt-5.2"
    max_steps: 20
    tools: [search_flights, search_hotels]
    prompt: `...`
}
```

Block defaults apply to every call of the function and cannot vary per
call site; use `$` parameters or setters for that.
