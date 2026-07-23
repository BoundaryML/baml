# Scenario Cookbook

This page organizes common AI scenarios by interaction shape. The point is not to memorize dozens of provider features. It is to choose the smallest honest capability and keep the task in an LLM function whenever possible.

## Quick chooser

| What changes?                                     | Use                                    | Typical result              |
| ------------------------------------------------- | -------------------------------------- | --------------------------- |
| Model, endpoint, gateway, headers, policy wrapper | `Generate` provider or wrapper         | `LlmResponse<T>`            |
| Values arrive incrementally                       | `Streaming`                            | `Stream<TPartial, T>`       |
| Model requests application actions                | `Tools`                                | typed tool-outcome union    |
| One task continues after the call returns         | `Background`                           | `Job<T>`                    |
| Many independent requests are submitted together  | `Batching`                             | `Batch<T>`                  |
| Provider retains conversation state               | `Sessions`                             | `Session`                   |
| Caller and model exchange live events             | `Realtime`                             | `LiveSession`               |
| Provider creates billable reusable context        | `ManagedCache`                         | `CacheResource`             |
| Caller needs a wire preview                       | `Inspectable`                          | `RequestPreview`            |
| Only one vendor/app needs the operation           | Direct provider method over `$request` | Whatever the vendor returns |

## 1. Single-turn text

```baml
function Haiku(topic: string) -> string {
  client: FastModel
  prompt: `Write a haiku about ${topic}`
}

let poem = Haiku("compilers")
```

Use a plain LLM function. The selected provider only needs `Generate`.

## 2. Structured output

```baml
class Product {
  name: string,
  price: float,
  currency: string,
}

function ExtractProduct(text: string) -> Product {
  client: AccurateModel
  prompt: `Extract the product. ${ctx.output_format}\n${text}`
}
```

The provider decides whether to use prompt-injected schema, native JSON Schema, constrained decoding, or another strategy. `Generate` remains the capability because the observable shape is unchanged.

If an application requires native constrained decoding specifically, accept a narrower `ConstrainedGeneration` capability or use a provider that guarantees it. Do not make “strict” a boolean that every provider silently approximates.

## 3. Multimodal input

```baml
function ReadReceipt(receipt: image) -> Receipt {
  client: VisionModel
  prompt: `
    ${role("user")}
    Read this receipt: ${receipt}
    ${ctx.output_format}
  `
}
```

`PromptAst` and `LlmRequest.messages()` preserve the image part. A text-only custom provider returns a typed unsupported-payload error. It must not flatten the image to a placeholder string.

Image, audio, PDF, and video acceptance is often model- and payload-dependent. The provider may implement `Generate` while a `describe(request)` probe reports that this particular request is unsupported.

## 4. Enriched output and reasoning metadata

```baml
let response = baml.ai.run_with_meta(
  SolveProblem$request(problem),
)

let answer = response.value
let reasoning = response.meta.attributes.get("reasoning")
let citations = response.meta.attributes.get("citations")
```

Common metadata is normalized. Provider-specific dimensions remain in typed provider helpers or the metadata attribute/raw escape hatch. Reading metadata never repeats the model call.

## 5. Structured streaming

```baml
let stream = BuildPlan$stream(goal)

while (true) {
  match (stream.next()) {
    null => break,
    let partial: Plan$stream => ui.render(partial),
  }
}

let plan: Plan = stream.final()
```

Use `Streaming`. Do not model a stream as repeated independent calls; doing so changes semantics, cost, and coherence.

## 6. Local tools

```baml
function ResearchQuestion(question: string) -> Answer {
  client: ToolModel
  prompt: `Research ${question}. ${ctx.output_format}`
}

let outcome = baml.ai.run_tools(
  ResearchQuestion$request(question),
  [SearchTool, CalculatorTool],
  dispatch,
  baml.ai.ToolBudget { max_steps: 12 },
)
```

Use `Tools` when the provider yields tool calls and the application executes them. Preserve the provider's call ID when returning each result.

The driver should support parallel calls when the provider emits them, but the application controls whether the tools themselves may run concurrently.

## 7. Server-executed tools

Some providers execute search, code, or retrieval on their own servers and return the final answer in one API operation.

```baml
let provider = GeminiWithSearch { ... }
let answer = ResearchQuestion(question, client = provider)
```

If there is no client-side tool turn, this may remain `Generate` with provider configuration. Use `Tools` only when the caller participates in the loop or needs tool lifecycle events.

This avoids forcing managed server tools into a fake local dispatcher.

## 8. Tool handoff and budget

```baml
match (baml.ai.run_tools(request, tools, dispatch, budget)) {
  ToolSucceeded<Answer> { value: let value } => value,
  ToolBudgetReached { transcript: let transcript } => queue_for_review(transcript),
  ToolHandoff { request: let request } => route(request),
}
```

Budget and handoff are expected control outcomes, not provider failures. Represent them in the return type.

## 9. Multi-agent delegation

An “agent” is usually an LLM function plus a tool roster and execution policy. A delegate can itself call another LLM function:

```baml
function delegate_to_billing(args: BillingQuestion) -> BillingAnswer {
  BillingAgent(args.question, client = BillingModel)
}
```

Use ordinary function calls for nested agents. Add a durable workflow only when the nested execution must suspend, resume, or survive process failure.

## 10. Conversation history kept by the application

```baml
function Chat(messages: baml.ai.ChatMessage[]) -> AssistantTurn {
  client: ChatModel
  prompt: prompt_from_messages(messages)
}
```

If the application sends the full history each time, no provider session capability is required. History is ordinary application data.

## 11. Provider-stored conversation

```baml
let session = baml.ai.open_session(SessionModel, baml.ai.SessionOptions {})
defer { session.cleanup() }

let greeting = session.run(Greet$request(name, client = SessionModel))
let followup = session.run(AskFollowup$request(question, client = SessionModel))
```

Use `Sessions` because the provider owns continuation state. `session.run` validates that the request targets the session's owner.

Forking and compaction are methods or narrower resource interfaces on `Session`, not unrelated methods on the provider marker.

## 12. Forking a conversation

```baml
let alternative = session.fork()
let a = session.run(Choose$request("conservative"))
let b = alternative.run(Choose$request("experimental"))
```

The returned session owns the new provider branch ID. If the provider cannot fork, the resource does not implement `ForkableSession`, or the method returns typed `Unsupported` if the session is existential.

## 13. Compaction

```baml
let result = session.compact(baml.ai.CompactionPolicy {
  target_tokens: 8_000,
  preserve_recent_turns: 4,
})
```

Use a session operation when compaction mutates provider state. Use an ordinary pure function when the application compacts its own message array.

## 14. One long-running response

```baml
let job = baml.ai.submit_background(
  DeepResearch$request(topic, client = BackgroundModel),
  baml.ai.BackgroundOptions { idempotency_key: research_id },
)
```

Use `Background` and `Job<T>`. Polling belongs to the job resource. See [Background jobs](./background-jobs.md).

## 15. A large batch of independent requests

```baml
let requests = documents.map((doc) -> {
  ExtractInvoice$request(doc, client = BatchModel)
})

let batch = baml.ai.submit_batch(
  BatchModel,
  requests,
  (request, index) -> { `invoice-${index}` },
)
```

Use `Batching`, not `Background`. Batch results may complete out of order, so each item has a caller-provided stable key:

```baml
for (let item in batch.results()) {
  match (item) {
    BatchSucceeded<Invoice> { key: let key, value: let invoice } => save(key, invoice),
    BatchFailed { key: let key, error: let error } => record_failure(key, error),
  }
}
```

## 16. Realtime voice

```baml
function VoiceAssistant(instructions: string) -> string {
  client: RealtimeModel
  prompt: `${role("system")}${instructions}`
}

let live = baml.ai.open_live(
  VoiceAssistant$request(instructions, client = RealtimeModel),
  audio_channel,
)
defer { live.cleanup() }

for (let event in live.events()) { handle_event(event) }
```

Use `Realtime` because the interaction is duplex and event-driven. `LiveSession` owns the socket/WebRTC session, event ordering, interruption controls, and close operation.

## 17. Barge-in

```baml
live.cancel_response()
live.truncate_assistant_audio(played_ms)
```

These methods belong to a controllable live resource. They should not take a bare channel and hope it corresponds to the right provider session.

## 18. Provider-managed prompt cache

```baml
let cache = baml.ai.create_cache(
  CacheModel,
  large_reference_material,
  baml.ai.CacheOptions { ttl: baml.time.Duration.from_hours(1) },
)
defer { cache.cleanup() }

let answer = cache.run(AskReference$request(question))
```

Use `ManagedCache` only when the provider exposes a real cache resource with a lifecycle. Implicit provider caching is metadata/configuration on ordinary generation, not a resource capability.

## 19. Retry

```baml
let provider = baml.ai.retry(
  primary,
  baml.ai.RetryPolicy {
    max_attempts: 3,
    base_delay: baml.time.Duration.from_milliseconds(200),
  },
)

let result = ExtractInvoice(document, client = provider)
```

The wrapper implements `Generate` if it can safely re-drive generation. It separately implements other capabilities only when their replay policies are safe. A background submit requires an idempotency key; a live session is normally never replayed automatically.

## 20. Fallback

```baml
let provider = baml.ai.fallback([primary, secondary])
```

The wrapper's `Generate` implementation tries members according to classified errors and operation replay policy. It does not catch a local metadata projection failure and send the prompt to another model.

For streaming, fallback is only allowed before the first observable chunk unless the API explicitly exposes a discontinuity event and the application opted in.

## 21. Round robin and routing

```baml
function route(tenant: Tenant, request_kind: string) -> baml.ai.Provider {
  if (tenant.region == "eu") { EuProvider } else { UsProvider }
}

let provider = route(tenant, "invoice")
let invoice = ExtractInvoice(document, client = provider)
```

Use ordinary BAML routing for business rules. Use a reusable provider wrapper for generic balancing algorithms. Dynamic routing is why LLM-function companions accept a provider override.

## 22. Observability

```baml
let traced = baml.ai.traced(provider, meter)
let response = baml.ai.run_with_meta(Task$request(input, client = traced))
```

Tracing is normally a wrapper over each capability. It records every attempt, including failures, rather than trying to reconstruct attempt history from the winning response metadata.

## 23. Evaluation

Evaluation is ordinary code around LLM functions:

```baml
testset "classifiers" {
  for (let case in cases) {
    test case.name {
      let actual = Classify(case.input, client = EvalModel)
      assert.equal(actual, case.expected)
    }
  }
}
```

Do not add an `Evaluation` provider capability unless a remote provider exposes a distinct evaluation job lifecycle. Local scoring and LLM-as-judge are functions.

## 24. Deployment gateway

A gateway that accepts the same semantic operation implements `Generate` and possibly `Streaming`:

```baml
class CompanyGateway {
  endpoint: string,
  token: string,

  implements baml.ai.Provider {}
  implements baml.ai.Generate { ... }
}
```

Tenant routing, headers, and audit tags are provider configuration. They are not separate capabilities.

## 25. OpenAI-compatible endpoint

Use a configurable standard provider when the endpoint genuinely follows that wire contract:

```baml
let LocalModel = baml.ai.OpenAiCompatible {
  base_url: "http://localhost:8000/v1",
  model: "local-model",
  auth: baml.ai.NoAuth {},
}
```

If the endpoint diverges materially, write a small custom `Generate` provider rather than accumulating ambiguous compatibility flags.

## 26. Coding-agent harness

A harness has permissions, filesystem/process tools, session state, and often a realtime event stream. Model it as a higher-level resource or application library built from provider capabilities, not as a pretend chat provider unless it truly satisfies `Generate`.

```baml
let run = CodingHarness.start(
  FixIssue$request(issue),
  permissions = RepoReadWrite,
)
defer { run.cleanup() }

for (let event in run.events()) { render(event) }
let result = run.final()
```

## 27. Durable workflow

A workflow coordinates requests and resources; it is not itself an LLM provider.

```baml
workflow ReviewAndNotify(repo: string) -> null {
  let job = baml.ai.submit_background(ReviewRepository$request(repo), options)
  let review = await_job(job)
  let message = DraftNotification(review)
  await approval(message)
  send_notification(message)
}
```

The workflow engine persists the `JobToken`, function identity, schema identity, and its own program counter. Provider resources supply durable external state; the workflow supplies orchestration durability.

## 28. Experimental vendor method

```baml
let result = Vendor.experimental_reasoning_tree(
  Solve$request(problem, client = Vendor),
  branches = 8,
)
```

Direct use is fine. Promote it to a capability only when a stable cross-provider contract exists.

## Coverage of the 47-scenario corpus

The repository scenario set is broader than the worked examples above. This table shows where every scenario lands in the proposed model.

|   # | Scenario                | Primary design surface                                                                                    |
| --: | ----------------------- | --------------------------------------------------------------------------------------------------------- |
|  01 | Single-turn text        | LLM function + `Generate`                                                                                 |
|  02 | Structured output       | LLM return type + `Generate`                                                                              |
|  03 | Constrained decoding    | `Generate` implementation or narrower constrained provider contract                                       |
|  04 | Streaming               | `$stream` + `Streaming`                                                                                   |
|  05 | Multimodal input        | `PromptAst`/`MessagePart` + payload descriptor                                                            |
|  06 | Non-text output         | Dedicated output capability when result is not parsed `T`; otherwise provider-specific direct API         |
|  07 | Reasoning               | `Generate` options + response metadata                                                                    |
|  08 | Enriched outputs        | `LlmResponse<T>.meta`                                                                                     |
|  09 | Tool calling            | `Tools` + `run_tools`                                                                                     |
|  10 | Agentic loop            | `Tools`, tool-loop wrapper, typed outcome union                                                           |
|  11 | Parallel tools          | `Tools` call IDs + dispatcher concurrency policy                                                          |
|  12 | Tool taxonomy           | Typed `Tool` definitions and explicit execution ownership                                                 |
|  13 | Searchable tools        | Tool registry/search library; no new provider primitive unless provider hosts search                      |
|  14 | Multi-agent             | Ordinary nested LLM functions + handoff result                                                            |
|  15 | Guardrails              | `Generate`/`Streaming` wrappers or explicit custom driver                                                 |
|  16 | Agent security          | Tool permissions and harness policy resources                                                             |
|  17 | History and sessions    | Application messages or `Sessions` resource, depending on owner                                           |
|  18 | Compaction              | Pure message function or `CompactableSession`                                                             |
|  19 | Fork/branch             | `ForkableSession` resource                                                                                |
|  20 | Server-stored chains    | `Sessions`/continuation resource                                                                          |
|  21 | Memory                  | Application `MemoryStore` library plus LLM functions; provider capability only for provider-hosted memory |
|  22 | Realtime voice          | `Realtime -> LiveSession`                                                                                 |
|  23 | Barge-in                | `LiveSession.cancel_response`/`truncate_assistant_audio`                                                  |
|  24 | Realtime tools          | Live-session tool events and dispatcher                                                                   |
|  25 | Voice pipelines         | Ordinary composition of speech, LLM, and audio functions/resources                                        |
|  26 | Transports              | Provider implementation helpers, not semantic capabilities                                                |
|  27 | Background jobs         | `Background -> Job<T>`                                                                                    |
|  28 | Provider diversity      | `Provider` + narrow capability interfaces                                                                 |
|  29 | Reliability             | Operation-aware retry/fallback wrappers                                                                   |
|  30 | Cascades/routing        | Ordinary routing + provider wrappers                                                                      |
|  31 | Caching                 | Framework cache wrapper, implicit metadata, or `ManagedCache` resource                                    |
|  32 | Observability           | `traced` wrappers + attempt/resource spans                                                                |
|  33 | Evaluation              | BAML tests/functions; remote eval job only if it has a distinct lifecycle                                 |
|  34 | Cost and batch          | Response metadata + separate `Batching -> Batch<T>`                                                       |
|  35 | Deployment shapes       | Custom `Generate` gateway/local provider implementations                                                  |
|  36 | Capability negotiation  | Interface match + optional graded descriptor                                                              |
|  37 | Harness basics          | Higher-level harness run resource around LLM requests/tools                                               |
|  38 | Harness permissions     | Typed permission policy owned by the harness                                                              |
|  39 | Harness extensibility   | Ordinary interfaces, tool libraries, and request drivers                                                  |
|  40 | Harness sessions        | Harness-owned session resource                                                                            |
|  41 | Harness deployment      | Provider/harness configuration; same semantic request types                                               |
|  42 | Harness abstraction     | Separate harness contract, not forced into `Generate`                                                     |
|  43 | Workflow graph          | Workflow library over LLM requests and resources                                                          |
|  44 | Workflow suspend/resume | Persisted program state + resource tokens                                                                 |
|  45 | Durable workflows       | Durable task envelope, schema identity, idempotency                                                       |
|  46 | Workflow observability  | Linked task/resource/workflow spans                                                                       |
|  47 | Workflow agent nesting  | Nested workflows calling named LLM functions/harnesses                                                    |

The recurring rule is visible in the table: provider capabilities model remote interaction protocols; application orchestration remains ordinary BAML functions, resources, and workflows.

## Anti-patterns

### A provider option masquerading as a capability

```baml
interface Temperature requires Provider { ... } // avoid
```

Temperature changes configuration, not interaction shape.

### Rewriting a prompt inside a capability

```baml
provider.submit<T>(`Review ${repo}`) // avoid when ReviewRepository already exists
```

Pass `ReviewRepository$request(repo)` so tests, schemas, roles, media, and traces remain consistent.

### Loose state identifiers

```baml
let id = provider.start(...)
provider.poll<T>(id)
```

Return `Job<T>` or `Session` so ownership and lifecycle remain attached.

### Claiming all capabilities on a wrapper

An inner provider's methods do not automatically preserve the wrapper's policy. Forward each capability intentionally.
