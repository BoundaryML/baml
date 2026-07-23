// Cache creation is provider-first: there is no Task<T> until after the Cache
// resource exists. The provider is therefore the runner's honest input.
class CreateCache {
  messages: root.ai.Messages,
  options: root.ai.CacheOptions,

  function new(
    messages: root.ai.Messages,
    ttl: baml.time.Duration,
  ) -> CreateCache throws never {
    CreateCache { messages: messages, options: root.ai.CacheOptions { ttl: ttl } }
  }

  implements root.ai.Runner<root.ai.Provider> {
    type Output = root.ai.Cache
    type Error = baml.errors.UnknownError | baml.errors.Unsupported

    function run(self, provider: root.ai.Provider) -> root.ai.Cache
        throws baml.errors.UnknownError | baml.errors.Unsupported {
      root.ai.drivers.create_cache(provider, self.messages, self.options)
    }
  }
}
