# Alternatives considered

Each section records one design decision: what was chosen, the options
that were considered, and the reasons. Guides state behavior; this page
records why the behavior is what it is.

## 1. One entry point; the runner option picks the handle type

Every run of an LLM function is created at one syntactic form. The
`runner` option selects the kind of run. The handle type follows the
runner through an associated type:

```baml
interface SessionRunner<Out> {
    type Handle extends baml.session.AnyHandle
    function name(self) -> string
    function open(self, t: Task<Out>, opts: OpenOptions) -> Self.Handle
}
```

```baml
let trip: Itinerary          = PlanTrip("2 weeks in Japan");            // sugar: Blocking runner
let s:    Session<Itinerary> = PlanTrip@session(trip_request = "2 weeks in Japan");
let job:  Job<Itinerary>     = PlanTrip@session(trip_request = "2 weeks in Japan",
                                                $runner = jobs, $id = "trip-9421");
```

| Option | Shape | Why not |
|---|---|---|
| A. One `@`-form per kind | `f@job(...)`, `f@live(...)` | Closed set. Every new kind of run requires a compiler change. |
| B. Free functions over a reified task | `submit(f@task(...))` | Run sites are no longer findable by searching for the `@` form. There is no registry of runners without additional machinery. The task is consumed before a journal exists, so attribution depends on each runner author. |
| C. Uniform handle, wrapped after creation | `background(open(f@task(...)))` | A wrapper can add methods but cannot remove them. A job runs detached, with no caller attached to converse: a message sent to it would never be read, so `Job` deliberately has no `send`, and a wrapper over `Session` cannot take `send` away. Some variation is also creation-time: `Background` needs create-only store semantics and a detached driver before a session exists to wrap. pi uses this shape; there it is a domain choice, since the SDK serves one product concept. |
| D. `$runner` parameter with associated `Handle` (chosen) | code above | Open set, one findable entry point, and the handle has the correct type from creation. |

Cost of D: the typing rule projects an associated type through an option
value, with `Out` inferred at the `@` site. This is standard in Rust and
Swift, and simpler than general generic associated types because the
desugar knows `Out`. It is the most advanced typing in this BEP and the
first thing the reference implementation must validate.

On TypeScript: option C is not forced by the language. TypeScript
expresses runner-dependent handles with conditional types and `infer`.
Its actual limitation is erasure: a TypeScript type cannot render a
schema, validate arguments, or register itself at runtime, so TypeScript
agent SDKs carry a parallel runtime schema system (TypeBox in pi,
Valibot in Flue). BAML types are runtime values, so one declaration
serves as the compile-time check, the `${ctx.output_format}` rendering,
the argument validator, and the registry entry.

## 2. Runners are infrastructure; identity is per-open

The runner, the options, and the task have different lifetimes, so they
are configured in different places. The runner is per-deployment: it is
stateful and shared. Options are per-run: `id`, `new`, `resume`, policy.
The task is per-call: the function and its arguments.

```baml
// rejected: per-run identity as a runner field
let r = Background { store: pg, id: "trip-9421" };

// chosen: one runner per application; identity at the open site
let jobs = Background { store: pg, workers: baml.spawn.TaskGroup.new(8) };
let j1 = PlanTrip@session(trip_request = a, $runner = jobs, $id = "trip-1");
let j2 = AuditAgent@session(scope = b, $runner = jobs, $id = "audit-9");
```

An `id` field on a shared runner has no coherent meaning, so per-run
values moved to the open site. Shared runner state is what several
features require: a concurrency cap across all jobs, one recovery scan
(`jobs.recover()`), re-attachment (`jobs.attach(id)`), and `cleanup()`
for leases and connection pools. A runner with no state and no lifecycle
should be a policy or an option instead; `Blocking` and `InMemory` are
kept as runners only for uniformity.

## 3. Passing function arguments versus run configuration

Configuration parameters share the call parentheses with the function's
arguments, distinguished by a `$` prefix. Function parameters cannot
start with `$`, so the namespaces cannot collide. Behavior settings also
have setters, usable mid-run; every setter appends a journal event.

```baml
let trip = PlanTrip("2 weeks in Japan", $max_steps = 20);
let s: Session<Itinerary> = PlanTrip@session(trip_request = r, $policy = approval_policy);
s.set_client(cheap_client);        // journaled as ClientChanged
```

| Option | Shape | Why not |
|---|---|---|
| A. Configuration as plain call arguments | `PlanTrip("...", max_steps = 20)` | Reserves parameter names. A function with its own `max_steps` parameter collides with the configuration. |
| B. A `with` clause | `PlanTrip("...") with baml.session.options(max_steps = 20)` | Considered at length; see below. |
| C. Builder chaining | `PlanTrip@session(trip_request = r).runner(jobs).open()` | Creation becomes two-phase: an unfinished builder is a value that can escape, the handle type is not visible until the chain ends, and every option needs a method on the builder. |
| D. A second argument list or dedicated separator | `PlanTrip@session(trip_request = r; max_steps = 20)` | New grammar with no precedent in the language. |
| E. `$`-prefixed parameters plus setters (chosen) | code above | One expression, no reserved names, the runner visible at the creation expression (required by the typing rule of section 1), and initial values and later changes share one journaled model. |

### The `with` clause, in full

The `with` clause was the working design for some time, modeled on the
clause `spawn` already has:

```baml
let trip = PlanTrip("2 weeks in Japan") with baml.session.options(max_steps = 20);

let s = PlanTrip@session(trip_request = r)
    with baml.session.options(policy = approval_policy, runner = jobs, id = "trip-1");
```

It separates the two namespaces cleanly and keeps the runner at the
creation expression. It was rejected for two reasons:

1. It introduces more syntax. `spawn with` exists, but attaching a
   clause to ordinary calls and `@session` expressions is a new grammar
   position, and every configured call site carries the
   `with baml.session.options(...)` weight for what a sigil expresses in
   one character.
2. It does not solve mid-run change. Settings such as the client and
   the policy must be changeable while a session runs, so
   `set_client` and `set_policy` have to exist regardless. With the
   clause, the same setting has two unrelated syntaxes — a clause at
   creation, a method afterward — and the clause form has no position
   in the journal, while setter events do. The `$` parameter form
   avoids the split: creation values and later changes are one model,
   initial value plus journaled updates.

Precedence for a changeable setting is journal order: function-block
defaults, then `$` parameters at creation, then setters, then `run()`
arguments for a single call. Block defaults (the way `client:` already
works) are complementary: they apply to every call and cannot vary per
call site.

## 4. Static templates; mid-session change is a journaled command

An LLM function is a fixed template: prompt shape, return type, initial
tools. Anything that changes during a session changes in the policy,
through commands, and is recorded in the journal.

```baml
// rejected: re-render the function against session state each turn
function ReleaseAgent(state: ReleaseState) -> Report {
    tools: if (state.approved) { [publish_release, run_bash] } else { [request_approval, run_bash] }
    ...
}
```

```baml
// chosen: the function declares only the initial toolset...
function ReleaseAgent(goal: string) -> Report {
    client: "anthropic/claude-sonnet-5"
    tools: [request_approval, run_bash]        // publish_release is not mounted yet
    prompt: `You are a release agent. ${goal} ${ctx.transcript} ${ctx.output_format}`
}

// ...and the change is a decision made in the policy. The runner executes
// MountTools and appends ToolsChanged, so the journal reads: approval
// granted, tool mounted, tool called.
class ReleasePolicy {
    inner: baml.session.Policy,
    implements baml.session.Policy {
        function update(self, st: SessionState, j: Journal, e: Event) -> Command[] {
            match (e) {
                let g: PermissionGranted => [MountTools { names: ["publish_release"] }, CallModel {}],
                _ => self.inner.update(st, j, e),
            }
        }
    }
}
```

Per-turn re-rendering (Flue's model) was rejected for four reasons. It
creates two sources of truth for capabilities, the expression and the
policy. The journal records the effect of a change but not its cause.
Arbitrary per-turn expressions must be deterministic for replay to work,
which nothing enforces. A function that reads session state no longer
works as a plain one-shot call.

## 5. The journal owns all state

Clients are stateless codecs. Policy state is a cache derivable from the
journal. The runner holds nothing. When reviewing new features against this
decision, a proposal that stores conversation state outside the journal
should instead record events and derive the state.

This decision does not settle how API-native replay data or optional remote
continuation checkpoints are represented. It also does not make a durable
remote conversation equivalent to a response-chain optimization. Those
questions are open for redesign in
`03_client_replay_and_continuations.md`.

Rejected: provider-message arrays as the persisted state, as in
Pydantic AI and the OpenAI SDK. Message arrays lose tool, usage, and child
structure; they tie state to one provider's wire format; and they make
cross-provider resume a lossy conversion rather than a rendering
decision.

## 6. Two lanes: data queues, control preempts

`send` queues, and the policy chooses injection timing. `interrupt`
preempts through cancel tokens and is recorded after taking effect.

Rejected: interrupts as ordinary queued events. An interrupt that waits
behind queued messages does not interrupt. The cost of the split is two
delivery paths to reason about.

## 7. Tools are plain functions; no context parameter

Tool schemas come from signatures and docstrings via reflection
(BEP-062); arguments are validated by `reflect.call_any`. Session
interaction from inside a tool is ambient.

```baml
// rejected: injected context parameter
function set_todos(ctx: baml.session.Ctx, items: string[]) -> string { ctx.emit(...) }

// chosen: plain signature; ambient emit is a no-op outside a session
function set_todos(items: string[]) -> string {
    baml.session.emit(TodoUpdated { items: items });
    "ok"
}
```

The context parameter puts the session into every tool signature, makes
tools unusable outside sessions, and requires reflection to hide the
parameter when presenting schemas to the model. The ambient form follows
the `log.info` precedent. The cost is one more dynamically scoped
facility.

## 8. `send` in, `emit` inside

From outside, a session has two verbs: `send` for data (strings,
messages, custom events) and `interrupt` for control. `emit` was
rejected as the external verb because `x.emit(e)` reads as x
broadcasting outward, which is the opposite of delivering input. `emit`
remains as the ambient form inside tools, where the session is emitting
onto its own journal and the reading is correct. Only custom events can
be sent in; built-in events are produced by the runner alone, so a
caller cannot construct false model history.

## 9. Arguments are constants; messages are events

`f@session(args)` binds the function's arguments once. They are recorded
in `SessionStarted` and restored by `resume`. Conversation arrives
through `send`. Arguments render through the template on every turn;
messages render through `${ctx.transcript}`.

Rejected: delivering the opening request through `send` while the
function also declares parameters. That form gives declared arguments no
defined meaning in session mode.

## 10. Event unions bind on the policy

Custom events widen the built-in union at module level. The policy binds
the union (`type Ev = CCEvent`), and the session infers it from the
policy, which types `send` and `journal()`.

The session's type parameter is the *extension* — the union of custom
events only — and the machinery runs on `Event | X`. Parameterizing by
the full union is unsound: the runtime must upcast built-in events into
the parameter, which requires a lower bound on the type variable, and
type bounds are upper bounds (`extends`). With the extension form,
built-ins are members by construction; `never` is the empty extension,
and `Event | never` collapses so plain sessions need no annotation
machinery.

Rejected: declaring the union at the `@session` call site, which creates
a second source of truth that can disagree with the policy; and
declaring events inside the LLM function, which couples the policy layer
to the prompt layer and prevents middleware reuse. An untyped
`Custom { kind, data_json }` fallback variant was also rejected: the
typed extension covers every use the reference implementation found, and
an escape hatch would let unvalidated events into typed journals.

## 11. Durability is tiered; tier 3 is out of scope

Tier 1, snapshot and resume, is the v1 baseline. Tier 2, admission with
receipts and settlement, targets named instances and jobs. Tier 3, full
deterministic replay of arbitrary agent code, is out of scope for v1:
it constrains all user code to journaled sources of nondeterminism, and
pure policies plus journaled effects provide most of the value. The
journal format is designed so a stricter tier can be added without
breaking existing journals. Details: `../02_guides/12_durability.md`.

## 12. Streaming is not history

Token deltas travel on an ephemeral channel and are not journaled. The
journal records final messages. Recording deltas as events was rejected:
journals grow by orders of magnitude and replay becomes re-streaming.
A UI built only on the journal tail renders correct state; the stream
adds liveness, not information.

## 13. One journal per session; sessions form a tree

Child sessions have their own journals, linked by `child_id`. A single
flat log with correlation IDs was rejected: per-session replay and
compaction become unbounded, and exporting one delegation requires
filtering everything. Stores may co-locate journals physically; the tree
is the semantic model, not a storage requirement.
