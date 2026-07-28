class FakeCache {
    cache_key: string,
    deleted: bool,
    runs: int,

    implements Cache {
        function key(self) -> string throws never {
            self.cache_key
        }

        function run<T>(
            self,
            task: Task<T>,
        ) -> ResponseWithMetadata<T> throws baml.errors.UnknownError {
            //# Record one execution through this cache resource
            self.runs = self.runs + 1;
            //# Resolve the task's generation capability
            match (task.provider) {
                let provider: GenerationProvider => {
                    //## Generate while the cache resource is active
                    provider.generate<T>(task) catch (e) {
                        let unknown: baml.errors.UnknownError => throw unknown,
                        _ => {
                            throw baml.errors.UnknownError {
            data: e,
            message: ["fake cache generation failed"],
          }
                        },
                    }
                },
                _ => {
                    //## Reject tasks whose providers cannot generate
                    throw baml.errors.UnknownError {
          data: task.provider_name(),
          message: ["fake cache task provider cannot generate"],
        }
                },
            }
        }

        function delete(self) -> null throws never {
            //# Mark the cache resource deleted
            self.deleted = true;
            null
        }
    }

    implements Resource {}
}

class FakeCacheProvider {
    inner: GenerationProvider,

    implements Provider {
        function name(self) -> string throws never {
            "fake-cache"
        }
        function render_shorthand(self) -> string throws never {
            self.inner.render_shorthand()
        }
    }

    implements GenerationProvider {
        function generate<T>(
            self,
            task: Task<T>,
        ) -> ResponseWithMetadata<T> throws root.ai.CallError | baml.errors.UnknownError {
            self.inner.generate<T>(task.with_provider(self.inner))
        }
    }

    implements ManagedCacheProvider {
        function create_cache(
            self,
            messages: Messages,
            options: CacheOptions,
        ) -> Cache throws never {
            //# Allocate a provider-owned cache resource
            FakeCache { cache_key: "fake-cache-key", deleted: false, runs: 0 }
        }
    }
}
