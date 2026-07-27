# Provider-managed caches

A provider cache is a remote resource with a key, lifetime, and explicit
deletion.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.create_cache` | Creates provider-managed context |
| `ai.Cache` | Runs tasks against cached content |
| `Cache.delete()` | Deletes the remote cache |
| `cleanup()` | GC fallback for an abandoned cache |

## Example

```baml
class Answer {
  text: string,
}

function AnswerPolicyQuestion(question: string) -> Answer {
  provider: CachedModel
  prompt: `
    Answer this policy question.

    ${question}

    ${ctx.output_format}
  `
}

let cache = ai.create_cache(
  provider = CachedModel,
  messages = policy_corpus,
  ttl = baml.time.Duration.from_minutes(30),
);

defer { cache.delete() }

let response = cache.run(
  AnswerPolicyQuestion.task("When can a late order be replaced?"),
)
```

The cache owns its provider ID and deletion state. The task does not pretend
that cached content is ordinary prompt text resent on every call.

A live integration test must verify creation, use, and deletion. A cleanup
fallback test must trigger garbage collection and check that the remote key is
gone.

[Back to production resources](../production-resources.md)
