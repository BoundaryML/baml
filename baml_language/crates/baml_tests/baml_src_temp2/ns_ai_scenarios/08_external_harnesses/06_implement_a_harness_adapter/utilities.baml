// Decorates an existing harness with an application-specific label while
// forwarding its execution and session behavior unchanged.
class NamedHarness {
    inner: ai.harness.ModelHarness,
}

implements ai.harness.Harness for NamedHarness {
  function label(self) -> string throws never { "custom-coding-harness" }
  function open(self, options: ai.harness.HarnessOptions) -> ai.harness.HarnessSession
      { self.inner.open(options) }
  function run<T>(
    self,
    session: ai.harness.HarnessSession,
    task: ai.Task<T>,
    on_event: ((ai.observe.AgentEvent) -> null throws never)?,
  ) -> ai.harness.HarnessRun<T> throws baml.errors.UnknownError {
    self.inner.run<T>(session, task, on_event = on_event)
  }
  function save_session(self, session: ai.harness.HarnessSession) -> ai.harness.HarnessSessionToken
      { self.inner.save_session(session) }
  function restore_session(self, token: ai.harness.HarnessSessionToken) -> ai.harness.HarnessSession
      throws baml.errors.UnknownError { self.inner.restore_session(token) }
  function steer(self, session: ai.harness.HarnessSession, instruction: string) -> null
      throws baml.errors.UnknownError { self.inner.steer(session, instruction) }
  function interrupt(self, session: ai.harness.HarnessSession) -> null
      throws baml.errors.UnknownError { self.inner.interrupt(session) }
}
