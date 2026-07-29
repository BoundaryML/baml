// Custom runner that creates a provider-owned cache, runs one Task through it,
// and deletes the cache on every exit path.
class ManagedCacheRunner<T> {
    messages: root.ai.Messages,
    ttl: baml.time.Duration,

    function new(
        messages: root.ai.Messages,
        ttl: baml.time.Duration,
    ) -> ManagedCacheRunner<T> throws never {
        ManagedCacheRunner<T> { messages: messages, ttl: ttl }
    }

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = root.ai.ResponseWithMetadata<T>
        type Error = root.ai.Failure | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.ResponseWithMetadata<T> throws root.ai.Failure | baml.errors.UnknownError {
            //# Require the task's provider to own managed-cache resources
            let provider = match (task.provider) {
                let managed: root.google.ManagedCacheProvider => managed,
                _ => {
                    throw baml.errors.Unsupported {
                        message: "task provider cannot create managed caches: " + task.provider_name(),
                    }
                },
            };
            //# Create the cache and guarantee remote cleanup
            let cache = provider.create_cache(self.messages, root.google.CacheOptions { ttl: self.ttl });
            defer {
                cache.delete()
            }
            //# Execute this same task through the provider-owned cache
            cache.run<T>(task)
        }
    }
}
