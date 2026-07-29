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
            task: root.ai.Task<T>,
        ) -> root.ai.ResponseWithMetadata<T> throws root.ai.Failure | baml.errors.UnknownError {
            //# Record one execution through this cache resource
            self.runs = self.runs + 1;
            //# Resolve the task's one-turn agent capability
            match (task.provider) {
                let provider: root.ai.AgentProvider => {
                    //## Execute one zero-tool turn while the cache is active
                    let conversation = provider.begin<T>(task.with_provider(provider));
                    let result = provider.step<T>(conversation, []);
                    match (result.outcome) {
                        let value: T => {
                            root.ai.ResponseWithMetadata<T> {
                                value: value,
                                metadata: result.metadata,
                                conversation: conversation,
                            }
                        },
                        let calls: root.ai.tools.ToolCalls => {
                            throw root.ai.InvalidRequest {
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

    implements root.ai.Resource {}
}

class FakeCacheProvider {
    inner: root.ai.AgentProvider,

    implements root.ai.Provider {
        function name(self) -> string throws never {
            "fake-cache"
        }
        function render_shorthand(self) -> string throws never {
            self.inner.render_shorthand()
        }
    }

    implements root.ai.AgentProvider {
        function begin<T>(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.Conversation throws root.ai.Failure | baml.errors.UnknownError {
            self.inner.begin<T>(task.with_provider(self.inner))
        }

        function step<T>(
            self,
            conversation: root.ai.Conversation,
            tools: root.ai.tools.Tool[],
        ) -> root.ai.ModelStep<T> throws root.ai.Failure | baml.errors.UnknownError {
            self.inner.step<T>(conversation, tools)
        }

        function submit(
            self,
            conversation: root.ai.Conversation,
            results: root.ai.tools.ToolResult[],
        ) -> root.ai.Conversation throws root.ai.Failure | baml.errors.UnknownError {
            self.inner.submit(conversation, results)
        }
    }

    implements ManagedCacheProvider {
        function create_cache(
            self,
            messages: root.ai.Messages,
            options: CacheOptions,
        ) -> Cache throws never {
            //# Allocate a provider-owned cache resource
            FakeCache { cache_key: "fake-cache-key", deleted: false, runs: 0 }
        }
    }
}
