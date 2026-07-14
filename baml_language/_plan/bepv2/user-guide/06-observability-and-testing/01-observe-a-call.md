# Observe one model call

Ask for `Response<T>` when application code needs metadata beside the typed
value.

## Use it

```baml
let response = ai.drivers.generate_with_meta(
  ResolveTicket.task(ticket, $provider = FastModel),
)

log.info(`provider=${response.meta.provider}`)
log.info(`request_id=${response.meta.request_id ?? "unknown"}`)
metrics.add(response.meta.usage)

let resolution = response.value
```

`Meta.raw` may help diagnostics, but business code should prefer stable fields
and typed provider blocks. Raw payload shape belongs to the provider adapter.

## Trace all attempts

```baml
let meter = ai.UsageMeter {}
let observed = FastModel.traced(meter).with_retry(retry_policy)

let result = ResolveTicket(ticket, $provider = observed)
log.info(`attempts=${meter.calls()}`)
```

Wrapper order matters to what is measured. The standard trace wrapper must
record failed attempts as well as the winning response.

## Transcript versus metadata

`response.transcript` is exact continuation state when supplied. Metadata is
observation. Neither should be reconstructed from `response.value`.

## Related design and scenarios

- [Reliability observability](../../pages/08-reliability-and-errors.md#observability)
- Scenarios 07 reasoning, 08 enriched outputs, 32 observability

