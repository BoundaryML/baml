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
            task: ai.Task<T>,
        ) -> ai.ResponseWithMetadata<T> throws ai.Failure | baml.errors.UnknownError {
            //# Record one execution through this cache resource
            self.runs = self.runs + 1;
            //# Resolve the task's one-turn agent capability
            match (task.provider) {
                let provider: ai.AgentProvider => {
                    //## Execute one zero-tool turn while the cache is active
                    let conversation = provider.begin<T>(task.with_provider(provider));
                    let result = provider.step<T>(conversation, []);
                    match (result.outcome) {
                        let value: T => {
                            ai.ResponseWithMetadata<T> {
                                value: value,
                                metadata: result.metadata,
                                conversation: conversation,
                            }
                        },
                        let calls: ai.tools.ToolCalls => {
                            throw ai.InvalidRequest {
                                provider: provider.name(),
                                status_code: null,
                                detail: "fake cache zero-tool turn returned tool calls",
                            }
                        },
                    }
                },
                _ => {
                    //## Reject tasks whose providers cannot execute a model turn
                    throw baml.errors.UnknownError.with_message<never>(
                        task.provider_name(),
                        "fake cache task provider cannot run as an agent",
                    )
                },
            }
        }

        function delete(self) -> null throws never {
            //# Mark the cache resource deleted
            self.deleted = true;
            null
        }
    }

    implements ai.Resource {}
}

class FakeCacheProvider {
    inner: ai.AgentProvider,

    implements ai.Provider {
        function name(self) -> string throws never {
            "fake-cache"
        }
        function render_shorthand(self) -> string throws never {
            self.inner.render_shorthand()
        }
    }

    implements ai.AgentProvider {
        function begin<T>(
            self,
            task: ai.Task<T>,
        ) -> ai.Conversation throws ai.Failure | baml.errors.UnknownError {
            self.inner.begin<T>(task.with_provider(self.inner))
        }

        function step<T>(
            self,
            conversation: ai.Conversation,
            tools: ai.tools.Tool[],
        ) -> ai.ModelStep<T> throws ai.Failure | baml.errors.UnknownError {
            self.inner.step<T>(conversation, tools)
        }

        function submit(
            self,
            conversation: ai.Conversation,
            results: ai.tools.ToolResult[],
        ) -> ai.Conversation throws ai.Failure | baml.errors.UnknownError {
            self.inner.submit(conversation, results)
        }
    }

    implements ManagedCacheProvider {
        function create_cache(
            self,
            messages: ai.Messages,
            options: CacheOptions,
        ) -> Cache throws never {
            //# Allocate a provider-owned cache resource
            FakeCache { cache_key: "fake-cache-key", deleted: false, runs: 0 }
        }
    }
}
