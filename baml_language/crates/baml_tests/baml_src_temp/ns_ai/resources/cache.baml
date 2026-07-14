class CacheOptions {
  ttl: baml.time.Duration,
}

interface Cache {
  function key(self) -> string throws never
  function delete(self) -> null throws never
}

interface ManagedCacheProvider requires Provider {
  function create_cache(self, messages: Messages, options: CacheOptions) -> Cache
    throws baml.errors.UnknownError
}

class FakeCache {
  cache_key: string,
  deleted: bool,
  implements Cache {
    function key(self) -> string throws never { self.cache_key }
    function delete(self) -> null throws never { self.deleted = true; null }
  }
}

class FakeCacheProvider {
  implements Provider {
    function name(self) -> string throws never { "fake-cache" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }
  implements ManagedCacheProvider {
    function create_cache(self, messages: Messages, options: CacheOptions) -> Cache throws never {
      FakeCache { cache_key: "fake-cache-key", deleted: false }
    }
  }
}

