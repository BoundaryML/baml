// Decorates an existing harness with an application-specific label while
// forwarding its execution and session behavior unchanged.
class NamedHarness {
    inner: root.ai.harness.ModelHarness,
}

implements root.ai.harness.Harness for NamedHarness {
  function label(self) -> string throws never { "custom-coding-harness" }
  function open(self, options: root.ai.harness.HarnessOptions) -> root.ai.harness.HarnessSession
      { self.inner.open(options) }
  function run<T>(
    self,
    session: root.ai.harness.HarnessSession,
    task: root.ai.Task<T>,
    on_event: ((root.ai.observe.AgentEvent) -> null throws never)?,
  ) -> root.ai.harness.HarnessRun<T> throws baml.errors.UnknownError {
    self.inner.run<T>(session, task, on_event = on_event)
  }
  function save_session(self, session: root.ai.harness.HarnessSession) -> root.ai.harness.HarnessSessionToken
      { self.inner.save_session(session) }
  function restore_session(self, token: root.ai.harness.HarnessSessionToken) -> root.ai.harness.HarnessSession
      throws baml.errors.UnknownError { self.inner.restore_session(token) }
  function steer(self, session: root.ai.harness.HarnessSession, instruction: string) -> null
      throws baml.errors.UnknownError { self.inner.steer(session, instruction) }
  function interrupt(self, session: root.ai.harness.HarnessSession) -> null
      throws baml.errors.UnknownError { self.inner.interrupt(session) }
}
