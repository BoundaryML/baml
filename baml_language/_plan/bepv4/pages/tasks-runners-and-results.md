# Tasks, runners, and results

An LLM function has two entry points: call it now, or create a task and choose
a runner.

## Utilities used

| Utility | Result |
| --- | --- |
| Direct call | The declared `T` |
| `ai.run.Completion` | `T` |
| `ai.run.CompletionWithMeta` | `ai.ResponseWithMetadata<T>` |
| `ai.run.Generation` | `T` after exactly one provider interaction |
| `ai.run.Stream` | Partial values followed by a final `T` |

## Example

The examples use the shared support-ticket models: `SupportTicket`,
`Resolution`, the `sample_ticket()` helper that builds one, and the
`fast_model()` provider value.

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

let resolution: Resolution = ResolveTicket(sample_ticket())
```

### Illustrative output

```console
[INFO] calling ResolveTicket with provider openai
[INFO] provider returned structured output
[INFO] validated Resolution { category: "billing", ... }
```

`ResolveTicket@task` is the derived companion of the function: same arguments,
but it returns the unexecuted `ai.Task<Resolution>` instead of running it. The
`@` postfix marks a compiler-created function — `ResolveTicket` itself stays an
ordinary call. A `Task<T>` carries the bound arguments, the provider, the
declared tools, and the prompt recipe; `.with_provider(...)`, `.with_tools(...)`,
and `.run(runner = ...)` are methods on that value. A direct call
`ResolveTicket(...)` is exactly
`ResolveTicket@task(...).run(runner = ai.run.Completion<Resolution>.new())`.

`$provider` on a direct call and `.with_provider(...)` on a task are the same
rebind at different moments: a direct call has no task value to method-chain,
so the call-site argument is the only place to say it.

Creating a task does not contact the provider:

```baml
let task = ResolveTicket@task(sample_ticket())
```

### Illustrative output

```console
[INFO] created Task<Resolution> for ResolveTicket
[INFO] provider requests made: 0
```

The task remembers the LLM function, arguments, provider, prompt recipe,
return type, and declared tools. You can run that same typed job in different
ways.

## Keep provider metadata

```baml
let response: ai.ResponseWithMetadata<Resolution> = task.run(
  runner = ai.run.CompletionWithMeta<Resolution>.new(),
);

log.info(response.metadata.request_id);
log.info(response.metadata.usage);
log.info(response.value)
```

### Illustrative output

```console
[INFO] request_id = "req_42"
[INFO] provider = "openai", model = "gpt-5.6-luna"
[INFO] usage = { input_tokens: 84, output_tokens: 38 }
```

`ResponseWithMetadata<T>` keeps the value and metadata together. Metadata may
include the provider, model, request ID, finish reason, token usage, and
reported cost.

## Override the provider

Use `$provider` on a direct call or `.with_provider(...)` on a task:

```baml
let careful = anthropic.Messages {
  ...anthropic.messages(),
  model: "claude-sonnet-4-6",
  api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
};

let direct = ResolveTicket(
  sample_ticket(),
  $provider = careful,
);

let response = task
  .with_provider(careful)
  .run(runner = ai.run.CompletionWithMeta<Resolution>.new());
```

The shared `careful_model()` helper builds exactly this `anthropic.Messages`
value when you do not need to spell out the configuration.

### Illustrative output

```console
[INFO] original task provider unchanged: openai
[INFO] rebound copy to anthropic/claude-sonnet-4-6
[INFO] CompletionWithMeta returned ResponseWithMetadata<Resolution>
```

Rebinding returns a new task and re-renders provider-sensitive prompt details.
The original task is unchanged.

## Provider or runner?

A provider answers **how BAML communicates with a model or remote AI
service**. A runner answers **how an existing task proceeds and what typed
result its lifecycle returns**.

| The extension changes... | Add... |
| --- | --- |
| Authentication protocol or signing, endpoint protocol, request rendering, response parsing, provider events, or provider-owned continuation state | A provider adapter |
| Only the model, base URL, credentials, headers, or supported options for an existing protocol | A provider value or configuration |
| How a task runs: once, as a stream, in a tool loop, with retry or fallback, in a batch, through a durable workflow, or through a harness | A runner |
| Policy around an existing lifecycle, such as auditing, rate limits, a circuit breaker, or replay rules | A runner that wraps another runner |
| A provider-owned resource with no immediate task result, such as a live session or managed cache | A provider capability and resource operation |
| Only the prompt, arguments, output type, or default tools | The LLM function or a copied task; neither a new provider nor a new runner |

When an integration introduces both a new wire protocol and a useful
lifecycle, implement the provider capability first. Add a runner when that
lifecycle can work with any provider that implements the capability. This
keeps vendor details below the capability boundary and reusable orchestration
above it.

For example:

- Another OpenAI-compatible base URL is usually another configured
  `openai.Chat` value.
- A vendor with its own authentication, message format, streaming events, and
  continuation handles needs a provider adapter.
- A durable human-review queue that can accept tasks from several providers is
  a runner.
- Creating or deleting one vendor's managed prompt cache is a provider-owned
  resource operation, even if a portable helper exposes the shared capability.

The shorthand is: **providers know how to communicate; runners know how work
proceeds**.

## A quick runner rule

Use a direct call until you need a different output shape or lifecycle. Then
create a task and pick the runner whose return type matches the work:

```text
Completion          → T
CompletionWithMeta  → ResponseWithMetadata<T>
Agent               → Done<T> | BudgetReached | Handoff
Stream              → baml.llm.Stream<TPartial, T>
Background          → ai.jobs.Job<T>
Batch               → ai.jobs.Batch<T>
```
