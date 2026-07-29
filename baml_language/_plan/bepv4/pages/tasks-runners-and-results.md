# Tasks, runners, and results

An LLM function can run directly or produce an unexecuted task. Both forms use
the Agent lifecycle for ordinary model work.

## The two entry points

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

let direct: Resolution = ResolveTicket(sample_ticket())

let task: ai.Task<Resolution> =
  ResolveTicket@task(sample_ticket())
```

Creating a task does not make a provider request. It binds:

- the function arguments;
- the declared output type;
- the provider;
- the prompt recipe;
- the declared tools.

`Task.with_provider(...)` and `Task.with_tools(...)` return a modified task.
`Task.run(runner = ...)` consumes the task using the selected lifecycle.

## Ordinary execution is Agent

```baml
let outcome:
    ai.Done<Resolution>
    | ai.BudgetReached
    | ai.Handoff
    | ai.Interrupted =
  task.run(
    runner = ai.run.Agent<Resolution>.new(),
  )
```

The Agent selects the task's `ai.AgentProvider`, calls `begin`, and repeatedly
calls `step`. If a step returns `ai.tools.ToolCalls`, the Agent executes the
application functions and passes their correlated results to `submit`. If a
step returns `Resolution`, the run returns `Done<Resolution>`.

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
  reason: string, // currently "cancelled"
}
```

Use a fresh, uncancelled token when resuming `Interrupted.conversation`;
`CancelToken` is one-shot.

### A limit violation is a resumable failure

An oversized batch throws `ai.ToolCallLimitExceeded`, a non-transient
`ai.Failure` with `Effects.None`. It includes the configured and actual
counts, the exact pending `ToolCalls`, the provider-owned `Conversation`, and
`steps_taken`.

This differs from `BudgetReached`:

| Condition | Representation | Continuation state |
| --- | --- | --- |
| Step or cost budget reached between steps | `BudgetReached` outcome | Conversation has no newly rejected tool batch |
| Too many calls returned by one step | `ToolCallLimitExceeded` failure | Conversation and exact pending calls are retained |

To resume a limit failure, the caller submits one correlated
`ToolResult`—success or error—for every retained call, then constructs an
Agent with the returned conversation. It must not simply call `step` again
while the retained calls are unresolved. See
[Approvals, limits, and handoffs](approvals-limits-and-handoffs.md) for the
complete example.

## Direct-call lowering

Conceptually, the internal lowering is:

```baml
function _run_agent_to_response<T>(
  task: ai.Task<T>,
) -> ai.ResponseWithMetadata<T>
    throws ai.Failure | baml.errors.UnknownError {
  let outcome = task.run(runner = ai.run.Agent<T>.new());
  match (outcome) {
    let done: ai.Done<T> => ai.ResponseWithMetadata<T> {
      value: done.value,
      metadata: done.metadata,
      conversation: done.conversation,
    },
    let stopped: ai.BudgetReached => throw baml.errors.UnknownError {
      data: {
        "steps_taken": stopped.steps_taken,
        "reason": stopped.reason,
      },
      message: ["default Agent stopped before producing the task value"],
    },
    let handoff: ai.Handoff => throw baml.errors.UnknownError {
      data: {
        "tool": handoff.call.name,
        "args": handoff.call.args,
        "call_id": handoff.call.id,
        "steps_taken": handoff.steps_taken,
      },
      message: ["default Agent reached a handoff; run the task with an explicit Agent"],
    },
    let interrupted: ai.Interrupted => throw baml.errors.UnknownError {
      data: {
        "steps_taken": interrupted.steps_taken,
        "reason": interrupted.reason,
      },
      message: ["default Agent was interrupted before producing the task value"],
    },
  }
}
```

The generated `ResolveTicket(...)` call then projects `.value`. This is
compiler/runtime lowering, not a public helper applications should call. It
explains why a direct generated call returns `Resolution`, while an explicit
Agent returns the full outcome union and retains its conversation.

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
  let stopped: ai.BudgetReached => log.info(stopped.reason),
  let handoff: ai.Handoff => log.info(handoff.call.name),
  let interrupted: ai.Interrupted => log.info(interrupted.reason),
}
```

The implementation uses internal unwrapping helpers in scenario code where a
scenario intentionally expects `Done`. Public application code should still
handle all outcomes that are normal for its domain.

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

For a direct generated call, `$provider` is the equivalent call-site
override:

```baml
let value = ResolveTicket(
  sample_ticket(),
  $provider = provider,
)
```

## Provider versus runner

| Change | Extension point |
| --- | --- |
| Model name, credentials, base URL, or supported wire options | Configure a provider value |
| Authentication, request wire format, response parsing, or exact conversation state | Implement a provider adapter |
| Normal model turns and application tools | `ai.AgentProvider` plus `ai.run.Agent` |
| Tool-call count, approval, budgets, and handoff execution policy | Configure `ai.run.Agent` |
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
direct generated call → T
Agent                → Done<T> | BudgetReached | Handoff | Interrupted
Stream               → baml.llm.Stream<TPartial, T>
Background           → ai.jobs.Job<T>
Batch                → ai.jobs.Batch<T>
Transcribe           → string or ResponseWithMetadata<string>
VoiceAgent           → null
Harness              → ai.harness.HarnessRun<T>
```

The runnable scenario is:

```console
baml run --from crates/baml_tests/baml_src_temp2 \
  ai_scenarios.task_and_runners
```
