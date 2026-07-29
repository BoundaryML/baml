class EffectfulStepFailure {
}

implements root.ai.Failure for EffectfulStepFailure {
  function is_transient(self) -> bool throws never { true }
  function effects(self) -> root.ai.Effects throws never { root.ai.Effects.Committed }
}

class EffectfulProvider {
    inner: root.ai.testing.FakeProvider,
    implements root.ai.Provider {
        function name(self) -> string throws never {
            "effectful"
        }
        function render_shorthand(self) -> string throws never {
            self.inner.render_shorthand()
        }
    }
    implements root.ai.AgentProvider {
        function begin<T>(self, task: root.ai.Task<T>) -> root.ai.Conversation throws never {
            self.inner.begin<T>(task.with_provider(self.inner))
        }

        function step<T>(
            self,
            conversation: root.ai.Conversation,
            tools: root.ai.tools.Tool[],
        ) -> root.ai.ModelStep<T> throws root.ai.Failure | baml.errors.UnknownError {
            self.inner.step<T>(conversation, tools) catch (e) {
                let failure: root.ai.Failure => throw EffectfulStepFailure {},
                _ => throw e,
            }
        }

        function submit(
            self,
            conversation: root.ai.Conversation,
            results: root.ai.tools.ToolResult[],
        ) -> root.ai.Conversation throws root.ai.Failure {
            self.inner.submit(conversation, results)
        }
    }
}
