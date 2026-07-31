class EffectfulStepFailure {
}

implements ai.Failure for EffectfulStepFailure {
  function effects(self) -> ai.Effects throws never { ai.Effects.Committed }
}

class EffectfulProvider {
    inner: ai.testing.FakeProvider,
    implements ai.Provider {
        function name(self) -> string throws never {
            "effectful"
        }
        function render_shorthand(self) -> string throws never {
            self.inner.render_shorthand()
        }
    }
    implements ai.AgentProvider {
        function begin<T>(self, task: ai.Task<T>) -> ai.Conversation throws ai.Failure {
            self.inner.begin<T>(task.with_provider(self.inner))
        }

        function step<T>(
            self,
            conversation: ai.Conversation,
            tools: ai.tools.Tool[],
        ) -> ai.ModelStep<T> throws ai.Failure | baml.errors.UnknownError {
            self.inner.step<T>(conversation, tools) catch (e) {
                let failure: ai.Failure => throw EffectfulStepFailure {},
                _ => throw e,
            }
        }

        function submit(
            self,
            conversation: ai.Conversation,
            results: ai.tools.ToolResult[],
        ) -> ai.Conversation throws ai.Failure {
            self.inner.submit(conversation, results)
        }
    }
}
