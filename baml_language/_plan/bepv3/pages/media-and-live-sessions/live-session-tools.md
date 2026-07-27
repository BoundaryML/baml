# Use tools during a live session

Application tools in a managed live session remain ordinary BAML functions.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `LiveToolCalls` | Provider requests application functions |
| `LiveToolResults` | Observable correlated results |
| `VoiceAgent` | Executes tools automatically |

## Example

```baml
function lookup_account(customer_id: string) -> string {
  "active, pro tier"
}

function VoiceSupport(customer_id: string) -> null {
  provider: "openai/gpt-realtime"
  prompt: `
    Help customer ${customer_id}. Use account data when useful.
  `
  tools: [lookup_account]
}

VoiceSupport.task("customer-7").run(
  runner = ai.run.VoiceAgent.new(
    audio = audio_device,
    channel = trace_channel,
  ),
)
```

The provider emits a call with a stable ID. The runner validates and executes
`lookup_account`, submits the result, and makes both call and result observable
as live events.

A raw `ai.open_live` session does not silently execute tools. Applications
using the raw API either submit results themselves or opt into an explicit
automatic-tool wrapper.

[Back to media and live sessions](../media-and-live-sessions.md)
