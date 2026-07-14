function create_cache(
  provider: root.ai.Provider,
  messages: root.ai.Messages,
  options: root.ai.CacheOptions,
) -> root.ai.Cache throws baml.errors.UnknownError | baml.errors.Unsupported {
  match (provider) {
    let managed: root.ai.ManagedCacheProvider => managed.create_cache(messages, options),
    _ => throw baml.errors.Unsupported { message: "provider cannot create managed caches" },
  }
}

