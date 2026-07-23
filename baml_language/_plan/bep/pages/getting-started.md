# Getting Started

This page assumes you have not worked in this project before. It starts with application code, then reveals the provider framework only when the application needs it.

All names on this page are proposed by BEP-063.

## 1. Define the data you want back

BAML treats an LLM's structured output like any other typed value.

```baml
enum Priority {
  Low,
  Medium,
  High,
}

class TicketSummary {
  title: string,
  priority: Priority,
  customer_request: string,
  next_steps: string[],
}
```

`TicketSummary` is both a BAML type and the output schema shown to the model through `${ctx.output_format}`.

## 2. Configure a provider

A provider is an ordinary object supplied by the standard library or your application.

```baml
let SupportModel = baml.ai.OpenAi {
  model: "gpt-5-mini",
  api_key: env.OPENAI_API_KEY,
}
```

The concrete class implements `baml.ai.Provider` and the capabilities supported by that endpoint. An ordinary chat provider usually implements at least `Generate`; many also implement `Streaming` and `Tools`.

## 3. Declare an LLM function

```baml
function SummarizeTicket(ticket: string) -> TicketSummary {
  client: SupportModel
  prompt: `
    ${role("system")}
    You summarize customer support tickets for the on-call engineer.

    ${role("user")}
    Ticket:
    ${ticket}

    ${ctx.output_format}
  `
}
```

This declaration gives the project:

- one named task for traces and tests;
- typed arguments;
- a typed result;
- a default provider;
- a lazy prompt template;
- generated request, stream, render, and parse helpers.

## 4. Call it

```baml
let summary = SummarizeTicket(raw_ticket)
log.info(summary.title)
```

The result is a `TicketSummary`, not a provider response object.

Conceptually, the compiler performs two steps:

```baml
let request = SummarizeTicket$request(raw_ticket, client = SupportModel)
let summary = baml.ai.run(request)
```

The first step renders the task. The second step executes it through the provider's `Generate` capability.

## 5. Override the provider

The LLM function owns the default, but a caller may supply another provider:

```baml
let summary = SummarizeTicket(
  raw_ticket,
  client = baml.ai.Anthropic {
    model: "claude-sonnet-4-6",
    api_key: env.ANTHROPIC_API_KEY,
  },
)
```

The function's prompt and result type do not change. The request is rendered for the selected provider, then dynamically dispatched to that provider's `Generate` implementation.

## 6. Stream partial values

```baml
let stream = SummarizeTicket$stream(raw_ticket)

while (true) {
  match (stream.next()) {
    null => break,
    let partial: TicketSummary$stream => show_partial_summary(partial),
  }
}

let final_summary = stream.final()
```

`TicketSummary$stream` is the compiler-derived partial shape. The final value is still `TicketSummary`.

Under the hood:

```baml
baml.ai.stream<TicketSummary$stream, TicketSummary>(
  SummarizeTicket$request(raw_ticket),
)
```

If the selected provider does not implement `Streaming`, this fails with a typed `baml.errors.Unsupported` error before pretending to provide a stream.

## 7. Read response metadata

The ordinary function returns only the value. Use the same request when metadata matters:

```baml
let response = baml.ai.run_with_meta(
  SummarizeTicket$request(raw_ticket),
)

log.info(`model=${response.meta.model ?? "unknown"}`)
log.info(`finish=${response.meta.finish_reason ?? "unknown"}`)

match (response.meta.usage) {
  null => null,
  let usage: baml.ai.Usage => {
    log.info(`input=${usage.input_tokens} output=${usage.output_tokens}`)
  },
}

let summary = response.value
```

This does not make a second model call. `Generate.generate<T>` always produces value and metadata together; `baml.ai.run` merely drops the metadata for the common case.

## 8. Run the same task in the background

The LLM function still defines the task. The background driver changes the lifecycle.

```baml
let job: baml.ai.Job<TicketSummary> = baml.ai.submit_background(
  SummarizeTicket$request(raw_ticket, client = LongRunningModel),
  baml.ai.BackgroundOptions {
    idempotency_key: `ticket-${ticket_id}-summary-v1`,
  },
)
```

Poll it later:

```baml
match (job.poll()) {
  baml.ai.JobPending { retry_after: let delay } => schedule_poll(delay),
  baml.ai.JobSucceeded<TicketSummary> { value: let summary } => save(summary),
  baml.ai.JobFailed { error: let error } => report(error),
  baml.ai.JobCancelled => log.info("cancelled"),
}
```

The job object knows the provider and provider-specific ID. The application does not call `provider.poll(id)` or manually remember which provider created the ID.

For a process boundary:

```baml
let token = job.token()
database.save("ticket-summary-job", baml.json.to_string(token))

// In another worker with the provider configured again:
let resumed = LongRunningModel.resume_job<TicketSummary>(token)
let state = resumed.poll()
```

## 9. Use tools

Start with ordinary BAML functions for the tools:

```baml
class AccountArgs {
  account_id: string,
}

function load_account(args: AccountArgs) -> Account {
  crm.get_account(args.account_id)
}

let tools = [
  baml.ai.Tool.from_function("load_account", load_account),
]
```

Then run the LLM task through the tool-loop driver:

```baml
let outcome = baml.ai.run_tools(
  SummarizeTicket$request(raw_ticket, client = ToolModel),
  tools,
  baml.ai.dispatch_functions(tools),
  baml.ai.ToolBudget { max_steps: 8 },
)

match (outcome) {
  baml.ai.ToolSucceeded<TicketSummary> { value: let summary } => save(summary),
  baml.ai.ToolBudgetReached { transcript: let t } => save_for_review(t),
  baml.ai.ToolHandoff { request: let h } => route_to_human(h),
}
```

The sum type is deliberate. Reaching a budget or requesting a handoff is not the same as returning `TicketSummary`, and neither needs to masquerade as an exception.

## 10. Use a provider wrapper

Suppose all support prompts and outputs must pass a policy check. The task is still prompt to typed answer, so use a provider wrapper rather than inventing a new calling convention:

```baml
let GuardedSupportModel = GuardedProvider {
  inner: SupportModel,
  policy: SupportDataPolicy,
}

let summary = SummarizeTicket(
  raw_ticket,
  client = GuardedSupportModel,
)
```

The LLM function and every caller stay typed. The wrapper implements `Generate` and delegates to its inner provider after checking input and output.

## 11. Build a request without declaring an LLM function

An LLM function is preferred when the task is reusable. For a genuinely local, one-off operation, build the request manually with the same prompt tag:

```baml
let request = baml.ai.request<TicketSummary>(
  SupportModel,
  prompt`
    ${role("system")}Summarize this ticket.
    ${role("user")}${raw_ticket}
    ${ctx.output_format}
  `,
)

let summary = baml.ai.run(request)
```

The `prompt` tag returns a lazy template. `baml.ai.request<T>` supplies the provider and `T`, builds `ctx`, evaluates the template to `PromptAst`, and stores that AST in the request.

Prefer an LLM function when any of these are true:

- the prompt will be reused;
- a host SDK should expose the task;
- the task needs named traces or evals;
- the task may later stream, use tools, or run in the background;
- another developer should discover the contract from source.

## 12. Call a provider-specific operation directly

Provider administration is ordinary code:

```baml
let models = SupportModel.list_models()
let uploaded = SupportModel.upload_file(document)
```

If a provider-specific operation executes an LLM task, pass the generated request rather than reimplementing the prompt:

```baml
let job = MyVendor.submit_priority(
  SummarizeTicket$request(raw_ticket, client = MyVendor),
  queue = "premium",
)
```

This is the escape hatch. The framework does not require a capability declaration for code that only one application and one provider will ever call. Add a capability when multiple providers or libraries need to share the contract.

## 13. Handle a missing capability

```baml
let result = baml.ai.submit_background(
  SummarizeTicket$request(raw_ticket, client = maybe_background_provider),
  options,
) catch (e) {
  let unsupported: baml.errors.Unsupported => {
    log.warn(`background is unavailable on ${unsupported.provider}`)
    baml.ai.run(SummarizeTicket$request(raw_ticket, client = fallback_provider))
  },
  _ => throw e,
}
```

If background support is mandatory, accept `baml.ai.Background` in your own function signature rather than a general `Provider`. That moves the error to type checking at the caller.

## What to remember

- The LLM function is the task.
- `$request` is the task rendered but not executed.
- The provider is the implementation object.
- The driver chooses an interaction shape.
- The capability says whether that shape exists.
- A stateful operation returns a resource that knows how to continue itself.
