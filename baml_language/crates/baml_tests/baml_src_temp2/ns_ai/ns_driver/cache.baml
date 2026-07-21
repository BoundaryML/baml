// Cache creation is provider-first: there is no Task<T> until after the Cache
// resource exists. The provider is therefore the driver's honest input.
class CreateCache {
  messages: root.ai.Messages,
  options: root.ai.CacheOptions,
}

implements root.ai.Driver<root.ai.Provider> for CreateCache {
  type Output = root.ai.Cache
  type Error = baml.errors.UnknownError | baml.errors.Unsupported

  function drive(self, provider: root.ai.Provider) -> root.ai.Cache
      throws baml.errors.UnknownError | baml.errors.Unsupported {
    root.ai.drivers.create_cache(provider, self.messages, self.options)
  }
}

function create_cache(
  messages: root.ai.Messages,
  options: root.ai.CacheOptions,
) -> CreateCache throws never {
  CreateCache { messages: messages, options: options }
}
