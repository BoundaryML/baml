# Tools, agents, and side-effect-only tasks

Tools are actions. A BAML return value is the final result after zero or more
actions. Keeping those concepts separate makes tool loops, lists, and `-> null`
predictable.

## Where tools belong

Ask who executes the tool:

| Executor | Owner in BEPv2 |
| --- | --- |
| Your BAML process | task `tools:` roster, call-time roster, or `root.ai.Agent` |
| OpenAI's servers | typed provider configuration on a Responses provider |

Task-owned tools should eventually look like:

```baml
function ResolveTicket(id: string) -> Resolution {
  client: StrictOpenAI
  tools: [lookup_ticket_tool(), update_ticket_tool()]
  prompt: `Resolve ticket ${id}. ${ctx.output_format}`
}
```

The roster stays attached when a caller overrides `client`.

For a dynamic roster, use the agent execution surface:

```baml
let outcome = ResolveTicket.agent(
  id,
  tools = tools_for(current_user),
  dispatch = dispatch_for(current_user),
  client = StrictOpenAI,
)
```

Or bundle dynamic policy into a client value:

```baml
let agent = root.ai.Agent {
  inner: StrictOpenAI,
  tools: tools_for(current_user),
  dispatch: dispatch_for(current_user),
  stop_when: null,
}
```

The client wrapper is convenient for runtime composition, but swapping it for
another client also swaps away its roster. Static task tools do not have that
footgun.

## The local tool loop

The `Tools` capability has three operations:

```text
begin(request) -> transcript

step(transcript)
  -> ToolCalls       dispatch in your process
  -> T               final typed value

submit(transcript, ToolResult[]) -> updated transcript
```

`root.ai.run_agent<T>` drives that loop. It preserves provider call IDs when it
submits results, validates tool arguments before dispatch, and returns one of:

```baml
root.ai.Done<T> | root.ai.BudgetReached | root.ai.Handoff
```

## What does `-> void` mean?

BAML functions always have a return type. The current language spelling for a
side-effect-only function is `-> null`, not `-> void`:

```baml
function MarkComplete(task_id: string) -> null {
  client: StrictOpenAI
  tools: [mark_complete_tool()]
  prompt: `Call mark_complete for ${task_id}. After it succeeds, finish.`
}
```

Its meaning is:

1. the model may request local tool calls;
2. your dispatcher executes them and returns `ToolResult`s;
3. the model eventually emits a turn with no more tool calls;
4. BAML returns `null` and intentionally discards any assistant text.

There is no structured model payload to decode after the loop. `null` means
"the requested orchestration completed successfully," not "OpenAI returned a
JSON document containing useful data."

The manual reference shape is:

```baml
function MarkComplete_agent_manual(
  task_id: string,
  provider: root.ai.Provider,
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
) -> root.ai.Done<null> | root.ai.BudgetReached | root.ai.Handoff {
  root.ai.run_agent<null>(
    root.ai.task_request<null>(
      provider,
      "MarkComplete",
      "my_package",
      { "task_id": task_id },
      [mark_complete_tool()],
      root.ai.RequestOptions { dispatch: dispatch },
      {},
      prompt_recipe,
    ),
  )
}
```

### Important: `null` does not require a tool call

The current local OpenAI tool loop sends `tool_choice: "auto"`. The model may
finish without calling a tool. Therefore, do not use a `null` return by itself
as proof that a side effect occurred.

Proof should come from your application boundary:

- the dispatcher recorded the successful operation;
- an idempotency key or database state confirms it;
- the result is represented as data, for example `ExecutionReceipt`;
- or a future request policy sets tool choice to `required` or a specific tool.

If the caller needs evidence, return it:

```baml
class ExecutionReceipt {
  task_id: string,
  completed: bool,
  operation_id: string,
}

function MarkComplete(task_id: string) -> ExecutionReceipt {
  // same tool loop, but finalization must produce a receipt
}
```

Use `-> null` only when the caller genuinely does not need a model-produced
value and the tool system itself is the source of truth.

## Tool arguments must be objects

Prefer an argument class and `Tool.typed<A>`:

```baml
class MarkCompleteArgs {
  task_id: string,
  completed: bool,
}

function mark_complete_tool() -> root.ai.Tool {
  root.ai.Tool.typed<MarkCompleteArgs>(
    "mark_complete",
    "Mark a task complete. Set completed to true.",
  )
}
```

OpenAI function parameters have an object root. `OpenAiStrict` closes that
object, marks its fields required, and rejects malformed arguments before the
handler runs.

## Parallel tool calls

Parallel tool calls apply to independent actions in one model turn:

```text
model turn
  -> search("Tokyo")
  -> search("Paris")

dispatcher
  -> may execute both concurrently
  -> returns both ToolResults with matching call IDs
```

They do not apply to a final type such as `(A | B)[]`. The return array remains
one value.

The provider controls whether the wire protocol permits multiple calls; the
dispatcher controls whether your process executes the received calls
concurrently. Those are separate policies.

OpenAI documents `tool_choice` and parallel calls in its
[function-calling guide](https://developers.openai.com/api/docs/guides/function-calling#tool-choice).

## Local tools plus a strict final value

For `-> Report` with local tools, the loop has two phases conceptually:

```text
tool phase
  tool_choice = auto/required
  model requests application actions
  application submits results

final phase
  model stops requesting tools
  provider constrains and decodes the final Report
```

The current Chat Completions implementation supplies strict schemas for local
tools and uses a strict final response schema for structured `T`. For `string`,
it parses the final text. For `null`, it treats the first non-tool assistant
turn as completion and discards the text.

For a plain `Generate<T>` with no application tools, `OpenAiStrict` instead
uses the single forced synthetic `__baml_return_output` tool described in the
[main guide](./README.md).

## Provider-hosted tools

OpenAI-hosted web search, file search, code execution, and similar tools are
not local `root.ai.Tool` values: there is no local dispatcher for them. They
belong on a provider that models the Responses API's server-side tools.

When both hosted tools and a final strict value are needed, the provider must
allow the hosted tool phase before forcing or constraining finalization. Forcing
`__baml_return_output` on the first turn would prevent the model from choosing
web search first.

See the official [Responses create API](https://developers.openai.com/api/reference/resources/responses/methods/create)
for the current hosted-tool and `parallel_tool_calls` request fields.

## Refusals and errors

A refusal is not a valid `T` and must not be coerced into one. Plain
`OpenAiStrict.generate<T>` detects OpenAI's refusal field and returns a provider
error. The multi-turn tool loop should ultimately normalize refusal, content
filtering, incomplete output, HTTP errors, and schema decode failures into
typed provider outcomes; that normalization is still incomplete in this
reference implementation.
