# Tasks, runners, and results

An LLM function can run directly or produce an unexecuted task. Both forms use
the Agent lifecycle for ordinary model work.

## The three entry points

```baml
function ResolveTicket(ticket: SupportTicket) -> Resolution {
  provider: fast_model()
  prompt: `
    Resolve this support ticket.
    Subject: ${ticket.subject}
    Body: ${ticket.body}
    ${ctx.output_format}
  `
}

let direct: Resolution = ResolveTicket(sample_ticket());

let task: ai.Task<Resolution> =
  ResolveTicket@task(sample_ticket());

let completed: Resolution = task.complete();

let outcome = task.run(runner = ai.run.Agent<Resolution>.new())
```

- The direct generated call returns `T` and treats anything else as an error.
- `task.complete(runner?)` returns `T` with configuration — use it after
  `with_provider`/`with_tools`, or with a configured Agent runner. It throws
  `ai.IncompleteRun` when the run stops at a policy stop, handoff, or
  interruption.
- `task.run(runner)` returns the full five-outcome union:
  `ai.Done<T> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed`.

Creating a task does not make a provider request. It binds:

- the function arguments;
- the declared output type;
- the provider;
- the prompt recipe;
- the declared tools.

`Task.with_provider(...)` and `Task.with_tools(...)` return a modified task.
`Task.run(runner = ...)` consumes the task using the selected lifecycle and
is always a fresh start: tasks are stateless recipes, so a task can be run or
completed any number of times. Every continuation of an existing run — a new
user turn, a resume, or a tool-result submission — goes through
`ai.run.AgentSession<T>` instead.

## Ordinary execution is Agent

```baml
let outcome:
    ai.Done<Resolution>
    | ai.Stopped
    | ai.Handoff
    | ai.Interrupted
    | ai.Failed =
  task.run(
    runner = ai.run.Agent<Resolution>.new(),
  )
```

The Agent selects the task's `ai.AgentProvider`, calls `begin`, and repeatedly
calls `step`. If a step returns `ai.tools.ToolCalls`, the Agent executes the
application functions and passes their correlated results to `submit`. If a
step returns `Resolution`, the run returns `Done<Resolution>`.

A classified failure after the run has committed progress — at least one
completed provider step, or a continuation's entry append or submit — returns
`ai.Failed { cause, conversation, steps_taken, usage }` at the last committed
boundary. A failure before any progress still throws, having changed
nothing, so ordinary catch patterns and step-level `ai.retry` semantics are
unchanged.

A task with no application tools uses this same state machine. It will usually
finish after its first `step`; that is an optimization of the same protocol,
not a separate execution model.

## Runner policy versus provider preference

Provider configuration controls the request sent to a model. Runner
configuration controls what the application is willing to execute after a
provider responds. For example:

```baml
let outcome = task.run(
  runner = ai.run.Agent<Resolution>.new(
    max_tool_calls_per_step = 1,
  ),
)
```

`max_tool_calls_per_step` belongs to `Agent`, not `Provider`, because only the
runner owns application tool execution, approval callbacks, and handoff
delivery. A provider's `parallel_tool_calls = false` option is still useful:
it asks the model for one call and avoids unnecessary rejected turns. The
runner limit is the enforcement boundary if the response nevertheless
contains more calls.

The default is `null`, which preserves the current unlimited batch behavior.
`0` forbids all tool calls, and a positive value bounds each individual
provider step. The declared result type does not affect this setting: `T[]`
is one final array value, not permission for parallel tools.

Handoffs count toward the same limit. The Agent validates call IDs and then
checks the complete batch before any tool callback, handler, tool-call event,
or handoff can observe it.

## Cooperative interruption

Pass a `baml.spawn.CancelToken` to the Agent when the caller needs a resumable
interruption:

```baml
let cancel = baml.spawn.CancelToken.new();
let future = spawn {
  task.run(
    runner = ai.run.Agent<Resolution>.new(cancel = cancel),
  )
};

// An input handler, supervisor, or another task requests the stop.
let _ = cancel.cancel();

let outcome = await future;
match (outcome) {
  let interrupted: ai.Interrupted => {
    log.info(interrupted.steps_taken);
    log.info(interrupted.conversation);
  },
  _ => {},
}
```

This token is a passive signal owned by the Agent policy. It is deliberately
different from `future.cancel()` and from passing the token to
`spawn with baml.spawn.options(cancel = ...)`. Those operations hard-cancel
the spawned Future: `await` throws `baml.panics.Cancelled`, and there is no
Agent outcome or resumable checkpoint.

The Agent checks its token only at committed boundaries: before another model
request and after every result in a tool batch has been submitted to the
provider conversation. If interruption arrives during a request or parallel
tool batch, the Agent finishes that transaction before returning
`Interrupted`. It never returns a conversation with unresolved calls, but it
also does not roll back a shell command, file write, or other effect that has
already started. A slow operation may therefore delay cooperative
interruption.

If a model step that races cancellation returns a final `T` or a handoff,
`Done<T>` or `Handoff` wins. Those are already terminal outcomes. Otherwise
the next committed boundary returns:

```baml
class Interrupted {
  conversation: ai.Conversation,
  steps_taken: int,
  usage: ai.Usage,
  reason: string, // currently "cancelled"
}
```

Resume through a session with a fresh, uncancelled token — `CancelToken` is
one-shot:

```baml
let resumed = ai.run.AgentSession<Resolution>
  .of(task, outcome)
  .resume(
    runner = ai.run.Agent<Resolution>.new(cancel = baml.spawn.CancelToken.new()),
  )
```

### A limit violation is a Failed outcome

An oversized batch is returned as
`ai.Failed { cause: ai.ToolCallLimitExceeded, ... }` rather than thrown: the
provider step committed, so the run stops at the committed boundary and
carries the fault. The cause includes the configured and actual counts and
the exact pending `ToolCalls`; the outcome carries the provider-owned
`Conversation`, `steps_taken`, and cumulative `usage`.

This differs from `Stopped`:

| Condition | Representation | Continuation state |
| --- | --- | --- |
| `max_steps` limit, `stop_when` policy, or `StepPlan` stop between steps | `Stopped` outcome (with the matching `reason`) | Conversation has no newly rejected tool batch |
| Too many calls returned by one step | `ai.Failed` outcome with `ToolCallLimitExceeded` cause | Conversation and exact pending calls are retained |

To continue after a limit violation, pair the task and outcome in a session,
build one correlated result — `ai.tools.ToolOk.of(call, output)` or
`ai.tools.ToolError.of(call, message)` — for every retained call, and call
`session.submit_tool_results(results, runner?)`. The session validates that
each pending call receives exactly one result before the next model step. See
[Approvals, limits, and handoffs](approvals-limits-and-handoffs.md) for the
complete example.

## Direct-call lowering

Conceptually, the direct generated call and `task.complete` share one
unwrap:

```baml
function _complete<T>(
  task: ai.Task<T>,
) -> T throws ai.IncompleteRun | ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
  let outcome = task.run(runner = ai.run.Agent<T>.new());
  match (outcome) {
    let done: ai.Done<T> => done.value,
    let stopped: ai.Stopped => throw ai.IncompleteRun { outcome: stopped },
    let handoff: ai.Handoff => throw ai.IncompleteRun { outcome: handoff },
    let interrupted: ai.Interrupted => throw ai.IncompleteRun { outcome: interrupted },
    let failed: ai.Failed => match (failed.cause) {
      let failure: ai.Failure => throw failure,
      let unknown: baml.errors.UnknownError => throw unknown,
    },
  }
}
```

The three stop states become `ai.IncompleteRun { outcome }` — a lossless
channel conversion, not a fifth outcome. The carried outcome still holds the
committed conversation (`incomplete.conversation()` and
`incomplete.steps_taken()` read it uniformly across the three variants), so a
catch site can continue via
`ai.run.AgentSession.of(task, incomplete.outcome)`. A `Failed` outcome
rethrows its cause: the caller demanded completion, so a fault is a fault.
This explains why a direct generated call returns `Resolution`, while an
explicit Agent returns the full outcome union and retains its conversation.

`ai.IncompleteRun` deliberately does NOT implement `ai.Failure`. A demanded
completion that stopped at a resumable boundary is control flow, not a
fault, so it is its own term in the `throws` union — `throws
ai.IncompleteRun | ai.Failure | baml.errors.UnknownError` — and a generic
`ai.Failure` catch arm never silently absorbs it. Catch sites are forced to
decide what a stop means for them.

## Keep metadata

Explicit Agent execution retains metadata on `Done<T>`:

```baml
let outcome = ResolveTicket@task(sample_ticket()).run(
  runner = ai.run.Agent<Resolution>.new(),
);

match (outcome) {
  let done: ai.Done<Resolution> => {
    log.info(done.metadata.request_id);
    log.info(done.metadata.usage);
    log.info(done.value);
  },
  let stopped: ai.Stopped => log.info(stopped.reason),
  let handoff: ai.Handoff => log.info(handoff.call.name),
  let interrupted: ai.Interrupted => log.info(interrupted.reason),
  let failed: ai.Failed => log.info(failed.cause),
}
```

When only the final value matters, call `task.complete(runner?)` — or
`session.complete(message, runner?)` on a continuation — and catch
`ai.IncompleteRun` where a stop is exceptional. Public application code that
treats stops, handoffs, or resumable failures as normal control flow should
match the full outcome union instead.

## Override a provider

```baml
let provider = anthropic.messages(
  api_key = baml.env.get_or_panic("ANTHROPIC_API_KEY"),
);

let outcome = ResolveTicket@task(sample_ticket())
  .with_provider(provider)
  .run(runner = ai.run.Agent<Resolution>.new())
```

Rebinding a task re-renders provider-sensitive prompt material. The original
task is unchanged.

The proposed direct-call equivalent is a reserved `$provider` argument:

```baml
let value = ResolveTicket(
  sample_ticket(),
  $provider = provider,
)
```

That reserved argument is not implemented in the executable corpus yet. Use
the `F@task(...).with_provider(provider)` form above when running these
examples.

## Provider versus runner

| Change | Extension point |
| --- | --- |
| Model name, credentials, base URL, or supported wire options | Configure a provider value |
| Authentication, request wire format, response parsing, or exact conversation state | Implement a provider adapter |
| Normal model turns and application tools | `ai.AgentProvider` plus `ai.run.Agent` |
| Tool-call count, approval, step limits (`max_steps`, `stop_when`), and handoff execution policy | Configure `ai.run.Agent` |
| Partial output | `ai.StreamingProvider` plus `ai.run.Stream` |
| Background or batch submission | `ai.jobs` capability plus the matching runner |
| Realtime audio and events | `ai.realtime` |
| Coding/research sandbox with permissions and steering | `ai.harness.Harness` plus `ai.run.Harness` |
| Reusable lifecycle not supplied by BAML | Implement `ai.Runner<Input>` |

`Provider` alone is identity and prompt-rendering configuration. It does not
promise normal model execution. A normal provider also implements
`AgentProvider`; other lifecycles opt into their own capability interfaces.

## Result map

```text
direct generated call  → T
task.complete(runner?) → T (throws ai.IncompleteRun on a stop)
Agent                  → Done<T> | Stopped | Handoff | Interrupted | Failed
Stream                 → baml.llm.Stream<TPartial, T>
Background             → ai.jobs.Job<T>
Batch                  → ai.jobs.Batch<T>
Transcribe             → string or ResponseWithMetadata<string>
VoiceAgent             → null
Harness                → ai.harness.HarnessRun<T>
```

Continuations never re-enter this map through `task.run` — they go through
`ai.run.AgentSession<T>`, whose `send`, `resume`, and `submit_tool_results`
return the same five-outcome union and whose `complete` returns `T`.

The runnable scenario is:

```console
baml run --from crates/baml_tests/baml_src_temp2 \
  ai_scenarios.task_and_runners
```
