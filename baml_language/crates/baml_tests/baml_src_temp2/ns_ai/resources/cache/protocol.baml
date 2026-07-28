class CacheOptions {
    ttl: baml.time.Duration,
}

interface Cache requires Resource {
  function key(self) -> string throws never
  function run<T>(self, task: Task<T>) -> ResponseWithMetadata<T> throws baml.errors.UnknownError
  function delete(self) -> null throws baml.errors.UnknownError
}

interface ManagedCacheProvider requires Provider {
  function create_cache(self, messages: Messages, options: CacheOptions) -> Cache
    throws baml.errors.UnknownError
}
