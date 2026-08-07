# Event reference

## The catalog

Only the runner produces events. All classes live under `ai`.

| Event | Fields | Producer appends it |
|---|---|---|
| `RunStarted` | `spec_name: string`, `arguments: map<string, unknown>` | once, when the journal is created |
| `UserMessage` | `content: string` | by custom runners; the repair loop uses it ephemerally, without committing |
| `AssistantMessage` | `content: ContentBlock[]`, `client_id: string` | in the turn batch, after a successful `invoke` |
| `ToolRequested` | `id: string`, `name: string`, `args: json` | in the turn batch, one per `ToolUse` block, in block order |
| `ToolCompleted` | `id: string`, `output: json` | when the correlated tool returns |
| `ToolFailed` | `id: string`, `message: string` | when the correlated tool throws or its arguments fail validation |
| `Usage` | `input_tokens: int`, `output_tokens: int`, `cached_input_tokens: int?`, `reasoning_tokens: int?` | in the turn batch, from the API-reported numbers |
| `FinalProduced` | `value: json` | once, when the final candidate parses as the return type |

## Ordering rules

- A model turn commits as one batch: `AssistantMessage`, then its
  `ToolRequested` projections in block order, then `Usage`. A failed
  `invoke` appends nothing.
- Tool results append in completion order, which may differ from
  request order. Correlation is by `id`.
- Every `ToolRequested.id` receives exactly one `ToolCompleted` or
  `ToolFailed` before the next turn batch.
- A `Raise`-mode failure appends its `ToolFailed` before
  `ToolFailedError` propagates, so an aborted run's journal is
  complete up to the abort.
- `FinalProduced` is the last event of a successful run.
- Ephemeral repair attempts commit nothing. A `UserMessage` appears in
  a journal only when a runner appends it deliberately.

## Rendering rules

Which events a client lowers into model input:

| Event | Lowers to |
|---|---|
| `RunStarted` | nothing; arguments reach the model through the template |
| `UserMessage` | a user turn in the wire API's role |
| `AssistantMessage` | an assistant turn: text from `Text` blocks, calls from `ToolUse` blocks, media from `Media` blocks, each in the wire API's shape |
| `ToolRequested` | nothing; it mirrors a block already lowered |
| `ToolCompleted` | the wire API's tool-result shape, correlated by id |
| `ToolFailed` | the wire API's tool-result shape, marked as an error |
| `Usage` | nothing |
| `FinalProduced` | nothing; the run is over |

The rendering rules are what make `ToolRequested` safe to append: it
exists for observers, and no client may turn it into a duplicate input
item.
