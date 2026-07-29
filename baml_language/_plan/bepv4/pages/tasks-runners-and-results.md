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
let outcome: ai.Done<Resolution> | ai.BudgetReached | ai.Handoff =
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
Agent                → Done<T> | BudgetReached | Handoff
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
