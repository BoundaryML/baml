// Cache creation is provider-first: there is no root.ai.Task<T> until after the Cache
// resource exists. The provider is therefore the runner's honest input.
class CreateCache {
    messages: root.ai.Messages,
    options: CacheOptions,

    function new(messages: root.ai.Messages, ttl: baml.time.Duration) -> CreateCache throws never {
        CreateCache { messages: messages, options: CacheOptions { ttl: ttl } }
    }

    implements root.ai.Runner<root.ai.Provider> {
        type Output = Cache
        type Error = root.ai.Failure | baml.errors.UnknownError

        function run(
            self,
            provider: root.ai.Provider,
        ) -> Cache throws root.ai.Failure | baml.errors.UnknownError {
            //# Select a provider that can own a remote cache
            match (provider) {
                let managed: ManagedCacheProvider => {
                    //## Create the cache resource from reusable messages
                    managed.create_cache(self.messages, self.options)
                },
                _ => {
                    //## Reject providers without managed-cache support
                    throw baml.errors.Unsupported {
          message: "provider cannot create managed caches",
        }
                },
            }
        }
    }
}
