class CacheOptions { ttl: baml.time.Duration }
interface Cache {
  function run<T>(self, request: Request<T>) -> T
    throws baml.errors.CallError | baml.errors.UnknownError
  function delete(self) -> null throws baml.errors.UnknownError
  function cleanup(self) -> null throws never
}

interface ManagedCache requires Provider {
  function create_cache(self, content: ChatMessage[], options: CacheOptions) -> Cache
    throws baml.errors.UnknownError
}

function create_cache(provider: Provider, content: ChatMessage[], options: CacheOptions) -> Cache
    throws baml.errors.UnknownError {
  let managed = match (provider) {
    let capability: ManagedCache => capability,
    _ => throw baml.errors.UnknownError { data: provider, message: ["provider cannot create caches"] },
  };
  managed.create_cache(content, options)
}

function with_cache<R, E>(
  provider: Provider,
  content: ChatMessage[],
  options: CacheOptions,
  body: (Cache) -> R throws E,
) -> R throws baml.errors.UnknownError | E {
  let cache = create_cache(provider, content, options);
  defer { cache.cleanup() }
  body(cache)
}
