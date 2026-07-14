# Use provider-managed and application caches

Cache the right layer. Provider-managed prompt caches are resources; ordinary
application result caches are business data.

## Provider-managed context cache

```baml
let cache = ai.drivers.create_cache(
  CacheModel,
  policy_documents,
  ai.CacheOptions { ttl: baml.time.Duration.from_hours(1) },
)
defer { cache.cleanup() }

let response = cache.run(
  ResolveTicket.task(ticket),
)
```

The cache resource owns the remote identifier, provider, billing lifecycle,
and cleanup behavior.

## Application result cache

```baml
let key = stable_hash(ticket, model_version, prompt_version)

match (result_cache.get<Resolution>(key)) {
  let hit: Resolution => hit,
  null => {
    let value = ResolveTicket(ticket)
    result_cache.put(key, value)
    value
  },
}
```

Include every input that can affect the result. Do not cache effectful tool
loops as if they were pure generations.

## Implicit provider caching

Automatic prefix reuse has no resource lifecycle. Report it through metadata
and usage rather than fabricating a `Cache` object.

## Related design and scenarios

- [Managed caches](../../pages/06-resources.md#managed-caches)
- Scenario 31 caching

