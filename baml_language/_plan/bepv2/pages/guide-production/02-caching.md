# Use provider-managed and application caches

> **Status:** Implemented in the executable reference.

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

Production code should keep the explicit `defer`: automatic cleanup runs only
after the cache becomes unreachable and GC discovers it. A test that is
specifically verifying automatic deletion can force that boundary after a
helper drops its last reference:

```baml
let _ = run_with_managed_cache(CacheModel)
baml.sys.collect_garbage()
assert.equal(CacheModel.deleted_keys, CacheModel.created_keys)
```

`collect_garbage()` drains queued cleanup finalizers before returning; it is a
deterministic test and diagnostics hook, not the normal cache lifecycle API.
The instrumented provider fields above are test-only; application code should
use the resource's `cleanup()` contract.

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

## Related design


- [Managed caches](../specification/07-resources.md#managed-caches)
