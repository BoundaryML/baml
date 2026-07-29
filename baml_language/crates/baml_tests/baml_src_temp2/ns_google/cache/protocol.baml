class CacheOptions {
    ttl: baml.time.Duration,
}

interface Cache requires root.ai.Resource {
  function key(self) -> string throws never
  function run<T>(self, task: root.ai.Task<T>) -> root.ai.ResponseWithMetadata<T> throws root.ai.Failure | baml.errors.UnknownError
  function delete(self) -> null throws root.ai.Failure | baml.errors.UnknownError
}

interface ManagedCacheProvider requires root.ai.Provider {
  function create_cache(self, messages: root.ai.Messages, options: CacheOptions) -> Cache
    throws root.ai.Failure | baml.errors.UnknownError
}
