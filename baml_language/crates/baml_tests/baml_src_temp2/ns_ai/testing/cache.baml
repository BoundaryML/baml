class FakeCache {
    cache_key: string,
    deleted: bool,
    runs: int,

    implements Cache {
        function key(self) -> string throws never {
            self.cache_key
        }

        function run<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
            self.runs = self.runs + 1;
            match (task.provider) {
                let provider: GenerationProvider => {
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
                    throw baml.errors.UnknownError {
          data: task.provider_name(),
          message: ["fake cache task provider cannot generate"],
        }
                },
            }
        }

        function delete(self) -> null throws never {
            self.deleted = true;
            null
        }
    }

    implements Resource {}
}

class FakeCacheProvider {
    implements Provider {
        function name(self) -> string throws never {
            "fake-cache"
        }
        function render_shorthand(self) -> string throws never {
            fake_render_shorthand()
        }
    }

    implements ManagedCacheProvider {
        function create_cache(
            self,
            messages: Messages,
            options: CacheOptions,
        ) -> Cache throws never {
            FakeCache { cache_key: "fake-cache-key", deleted: false, runs: 0 }
        }
    }
}
