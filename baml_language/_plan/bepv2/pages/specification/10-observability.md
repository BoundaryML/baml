# 10. Observability

Observability answers two questions: what happened during a run, and how much
did it cost? It must include failed attempts, tool calls, and resource actions,
not only the final successful response.

## One call

A response with metadata reports the selected provider, request ID, usage,
timing, and other stable facts:

```baml
let response = ai.drivers.drive_with_meta(ResolveTicket.task(ticket))
log.info(`provider=${response.meta.provider}`)
log.info(`input_tokens=${response.meta.usage?.input_tokens ?? 0}`)
```

Metadata describes the call. It is not conversation history and should not be
used to resume a provider session.

## Retries and fallback

A retry wrapper must record every attempt, including failures. The winning
response cannot tell us how many earlier requests failed.

```baml
let meter = ai.UsageMeter {}
let provider = Fast.traced(meter).with_retry(policy)
let result = ResolveTicket(ticket, $provider = provider)
log.info(`attempts=${meter.calls()}`)
```

If a wrapped provider returns a resource, its later operations must remain
observable. For example, job polls and session turns should appear under the
same trace context as resource creation.

## Agent events

Agent runs emit typed events for model starts, text deltas, tool calls, tool
results, roster changes, provider changes, usage updates, and the final run
outcome. Consumers can use the same event values in three ways:

- an observer records them;
- a UI renders them;
- a hook reads current state and returns a typed decision.

Observers do not change the run. Hooks may change the next step, so hook
decisions are part of execution policy and must also be recorded.

## Sensitive data

Logs should prefer stable IDs, counts, timings, and byte sizes. Prompts,
provider responses, tool arguments, and credentials may contain customer data
or secrets. Recording their full bodies must be an explicit application
choice.

See the [observability and testing guide](../guide-observability-and-testing/01-observe-a-call.md)
for usage examples.
