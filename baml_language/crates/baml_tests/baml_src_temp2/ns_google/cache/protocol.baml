class CacheOptions {
    ttl: baml.time.Duration,
}

interface Cache requires ai.Resource {
  function key(self) -> string throws never
  function run<T>(self, task: ai.Task<T>) -> ai.ResponseWithMetadata<T> throws ai.Failure | baml.errors.UnknownError
  function delete(self) -> null throws ai.Failure | baml.errors.UnknownError
}

interface ManagedCacheProvider requires ai.Provider {
  function create_cache(self, messages: ai.Messages, options: CacheOptions) -> Cache
    throws ai.Failure | baml.errors.UnknownError
}
