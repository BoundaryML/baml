# Policies

## What a policy is

A policy is pure logic that decides what a session does next. It receives
each event as it is appended and returns commands:

```baml
interface Policy {
    type Ev = baml.session.Event               // widen to add custom events

    function init(self) -> SessionState { SessionState.default() }

    // pure: no IO. The runner performs all effects.
    function update(self, st: SessionState, j: Journal<Self.Ev>, e: Self.Ev) -> Command[]
}
```

The policy decides; the runner acts. Because `update` is pure and every
effect result lands in the journal, policies are testable with literal
events and deterministic under replay.

Every session has a policy. The default, `baml.session.ToolLoop`,
implements the standard loop: user message → call the model; tool
requests → run them, recall the model when all complete; tool failure →
let the model react; final output → finish; budget exhausted → fail. If
you never think about policies, this is what you are using.

Write or compose a policy when the session's *behavior* must change:
approvals, budgets, steering, capability mounting. Do not use a policy
for ordinary sequencing — that is just code.

## Commands

A command is what a policy wants done:

```baml
class CallModel   { }
class RunTool     { call_id: string, tool: string, args_json: string }
class MountTools  { names: string[] }
class UnmountTools{ names: string[] }
class SpawnChild  { child_id: string, goal: string }
class RetryTool   { call_id: string, after_ms: int }   // runner owns the clock
class AwaitInput  { note: string? }              // end the turn as Replied / wait
class FinishTurn  { result_json: string }        // end the turn as Done
class CancelAll   { reason: string }
type Command = CallModel | RunTool | RetryTool | MountTools | UnmountTools
             | SpawnChild | AwaitInput | FinishTurn | CancelAll
```

Commands are data. A policy returning `[RunTool { ... }]` has run
nothing; it has decided something.

## The runner

The runner executes commands and is the only component that performs IO:
`CallModel` → render/invoke/ingest on the client and append the events;
`RunTool` → dispatch with validation, append `ToolCompleted` or
`ToolFailed`; `MountTools`/`UnmountTools` → update the toolbox, append
`ToolsChanged`; `CancelAll` → fire cancel tokens; `AwaitInput` /
`FinishTurn` → end the current `run()` as `Replied` or `Done`.

You do not implement or call the runner. It is documented so the policy
contract is precise: events in, commands out, effects elsewhere.

Rules for `update`:

- No IO. No model calls, no HTTP, no clocks, no randomness.
- Reading the journal is fine; it is immutable history.
- Appending custom events that record a decision is allowed. Appending
  built-in events is the runner's job.
- `SessionState` is scratch space and must be derivable from the journal:
  on resume, the runtime rebuilds it by re-folding the journal through
  `update`. State that cannot be rebuilt that way will differ after a
  resume — that is a bug in the policy.

## Middleware

A middleware is a policy that wraps another policy and delegates what it
does not handle. The stdlib ships the common ones:

| Middleware | Adds |
|---|---|
| `with_steering` | Buffer incoming messages; inject at turn boundaries. |
| `with_approval` | Hold selected tool calls until an approval event. |
| `with_budget` | Track `Usage`; stop the session at a cost cap. |
| `with_compaction` | Summarize old entries when context grows past a budget. |
| `with_retry` | Retry failed tool calls with backoff. |

Writing one — handle what you care about, delegate the rest:

```baml
class WithBudget {
    inner: baml.session.Policy,
    cap_usd: float,
    spent: float,

    implements baml.session.Policy {
        function update(self, st: SessionState, j: Journal, e: Event) -> Command[] {
            match (e) {
                let u: Usage => {
                    self.spent += cost_of(u);
                    if (self.spent > self.cap_usd) {
                        [CancelAll { reason: `budget exceeded: $${self.spent}` }]
                    } else { [] }
                },
                _ => self.inner.update(st, j, e),
            }
        }
    }
}
```

An approval gate intercepts commands on the way out, holds them, and
releases on an event:

```baml
class WithApproval {
    inner: baml.session.Policy,
    needs_ok: string[],
    held: map<string, RunTool>,

    implements baml.session.Policy {
        type Ev = ApprovalEvent   // baml.session.Event | PermissionRequested | PermissionGranted | PermissionDenied

        function update(self, st: SessionState, j: Journal<ApprovalEvent>, e: ApprovalEvent) -> Command[] {
            match (e) {
                let g: PermissionGranted =>
                    if let held: RunTool = self.held.get(g.call_id) { [held] } else { [] },
                let d: PermissionDenied => {
                    j.append(ToolFailed { call_id: d.call_id, error: "denied by operator" });
                    [CallModel {}]
                },
                _ => self.inner.update(st, j, e).map((cmd) -> {
                    match (cmd) {
                        let r: RunTool if self.needs_ok.includes(r.tool) => {
                            let _ = self.held.set(r.call_id, r);
                            j.append(PermissionRequested { call_id: r.call_id, tool: r.tool, why: r.args_json });
                            AwaitInput { note: `approve ${r.tool}?` }
                        },
                        _ => cmd,
                    }
                }),
            }
        }
    }
}
```

The journal shows the whole flow: `ToolRequested` → `PermissionRequested`
→ wait → `PermissionGranted` (via `s.send`) → the held `RunTool`
executes. A resumed session still has the request pending, because it is
an event, not a variable.

Capability changes go through commands, never through conditionals in the
function template:

```baml
let g: PermissionGranted => [MountTools { names: ["publish_release"] }, CallModel {}],
```

The runner appends `ToolsChanged`, so the journal records cause and
effect: approval granted, tool mounted, tool called.

## Composing a stack

Composition is construction; the result is one policy:

```baml
function PlanTrip(request: string) -> Itinerary {
    client: "openai/gpt-5.2"
    tools: [search_flights, search_hotels, book_hotel]
    prompt: `You are a travel agent. ${request} ${ctx.transcript} ${ctx.output_format}`
}

let policy = WithSteering { inner:
             WithApproval { needs_ok: ["book_hotel"], held: {}, inner:
             WithBudget   { cap_usd: 2.0, spent: 0.0, inner:
             baml.session.ToolLoop { max_steps: 12 } } } };

let s = PlanTrip@session(request = r) with baml.session.options(policy = policy);
```

Events flow outside-in; commands flow inside-out. Put steering outermost
so it sees raw user messages; put budgets inside approval so held calls
do not count until they run.

## Testing policies

`update` takes events and returns commands — no model, no network, no
clock. Tests are literal events in, command assertions out:

```baml
test "tool loop waits for all parallel tools before recalling the model" {
    let eng = baml.session.ToolLoop { max_steps: 8 };
    let st = eng.init();
    let j = Journal { entries: [] };

    let _ = eng.update(st, j, ToolRequested { call_id: "a", tool: "search_flights", args_json: "{}" });
    let _ = eng.update(st, j, ToolRequested { call_id: "b", tool: "search_hotels", args_json: "{}" });

    let after_first = eng.update(st, j, ToolCompleted { call_id: "a", result_json: "[]" });
    assert.equal(after_first.length(), 0);                       // one still in flight

    let after_both = eng.update(st, j, ToolCompleted { call_id: "b", result_json: "[]" });
    assert.is_true(after_both.some((c) -> { c is CallModel }))
}
```

Test middleware through the same interface, wrapping the real inner
policy it will wrap in production. For end-to-end behavior with a fake
model, see scripted clients in `../04_advanced/02_evals.md`.
