class NamedHarness {
  inner: root.ai.ModelHarness,
}

implements root.ai.Provider for NamedHarness {
  function name(self) -> string throws never { "custom-coding-harness" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements root.ai.HarnessProvider for NamedHarness {
  function open(self, options: root.ai.HarnessOptions) -> root.ai.HarnessSession
      { self.inner.open(options) }
  function run<T>(self, session: root.ai.HarnessSession, task: root.ai.Task<T>) -> root.ai.HarnessRun<T>
      throws baml.errors.UnknownError { self.inner.run<T>(session, task) }
  function save_session(self, session: root.ai.HarnessSession) -> root.ai.HarnessSessionToken
      { self.inner.save_session(session) }
  function restore_session(self, token: root.ai.HarnessSessionToken) -> root.ai.HarnessSession
      throws baml.errors.UnknownError { self.inner.restore_session(token) }
  function steer(self, session: root.ai.HarnessSession, instruction: string) -> null
      throws baml.errors.UnknownError { self.inner.steer(session, instruction) }
  function interrupt(self, session: root.ai.HarnessSession) -> null
      throws baml.errors.UnknownError { self.inner.interrupt(session) }
  function destroy(self, session: root.ai.HarnessSession) -> null
      throws baml.errors.UnknownError { self.inner.destroy(session) }
}

test "guide 08.06: a custom harness composes through out-of-body implements" {
  let harness = NamedHarness { inner: fake_model_harness() };
  let run = root.ai.drivers.run_harness<Resolution>(
    InvestigateRepository_task(sample_ticket(), harness), harness_options(),
  );
  assert.equal(harness.name(), "custom-coding-harness")
  assert.equal(run.value.category, "billing")
}
