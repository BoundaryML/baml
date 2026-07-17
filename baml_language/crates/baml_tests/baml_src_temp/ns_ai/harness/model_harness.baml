// A small in-process harness adapter backed by any DriveProvider. It is useful
// both as a real OpenAI/Anthropic integration adapter and as the blueprint for
// an external coding-harness implementation.
class ModelHarness {
  inner: Provider,
  sessions: LocalHarnessSession[],
}

implements Provider for ModelHarness {
  function name(self) -> string throws never { "model-harness(" + self.inner.name() + ")" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements DriveProvider for ModelHarness {
  function drive<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
    //# Open an isolated harness session
    let session = self.open(HarnessOptions {
      cwd: null,
      permission_mode: null,
      sandbox: null,
      attributes: {},
    });
    //# Run the task inside the session
    let run = self.run<T>(session, task);
    //# Return harness output as a provider response
    Response<T> {
      value: run.value,
      meta: Meta {
        provider: self.name(),
        model: null,
        request_id: session.id(),
        finish_reason: "harness_complete",
        usage: null,
        attributes: {},
        raw: null,
      },
      transcript: null,
    }
  }
  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy throws never {
    ReplayPolicy { kind: ReplayKind.Never, idempotency_key: null }
  }
}

implements Harness for ModelHarness {
  function label(self) -> string throws never { "model-harness(" + self.inner.name() + ")" }
  function open(self, options: HarnessOptions) -> HarnessSession throws never {
    let session = LocalHarnessSession {
      session_id: "session-" + (self.sessions.length() + 1).to_string(),
      history: Conversation.empty(),
      is_stopped: false,
      is_cleaned: false,
      steering: [],
      cleanup_count: 0,
    };
    self.sessions.push(session);
    session
  }

  function run<T>(self, session: HarnessSession, task: Task<T>) -> HarnessRun<T>
      throws baml.errors.UnknownError {
    //# Validate the harness session
    let local = match (session) {
      let value: LocalHarnessSession => value,
      _ => throw baml.errors.UnknownError { data: null, message: ["unsupported harness session"] },
    };
    if (local.is_stopped) {
      throw baml.errors.UnknownError { data: null, message: ["harness session is stopped"] };
    }
    //# Resolve the backing drive provider
    let provider = match (self.inner) {
      let capability: DriveProvider => capability,
      _ => throw baml.errors.UnknownError { data: null, message: ["harness inner cannot drive"] },
    };
    //# Run the model through the harness
    let response = provider.drive<T>(task.with_provider(self.inner)) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["harness model run failed"] },
    };
    //# Save conversation and resumable coordinates
    local.history = task.messages().to_conversation();
    let event: AgentEvent = RunFinishedEvent { id: local.session_id, outcome: "done" };
    HarnessRun<T> {
      value: response.value,
      events: [event],
      token: self.save_session(local),
      conversation: local.history,
    }
  }

  function save_session(self, session: HarnessSession) -> HarnessSessionToken throws never {
    HarnessSessionToken {
      provider: self.label(),
      runtime: "baml",
      session_id: session.id(),
      version: 1,
      sealed: baml.json.stringify({ "session_id": session.id() }),
    }
  }

  function restore_session(self, token: HarnessSessionToken) -> HarnessSession
      throws baml.errors.UnknownError {
    //# Validate harness token ownership
    if (token.provider != self.label() || token.runtime != "baml" || token.version != 1) {
      throw baml.errors.UnknownError {
        data: token,
        message: ["harness token does not belong to this harness/runtime/version"],
      };
    }
    //# Find the saved local session
    for (let session in self.sessions) {
      if (session.session_id == token.session_id) { return session; }
    }
    throw baml.errors.UnknownError { data: token, message: ["harness session not found"] }
  }

  function steer(self, session: HarnessSession, instruction: string) -> null
      throws baml.errors.UnknownError {
    let local = match (session) {
      let value: LocalHarnessSession => value,
      _ => throw baml.errors.UnknownError { data: null, message: ["unsupported harness session"] },
    };
    local.steering.push(instruction);
    null
  }

  function interrupt(self, session: HarnessSession) -> null throws baml.errors.UnknownError {
    let local = match (session) {
      let value: LocalHarnessSession => value,
      _ => throw baml.errors.UnknownError { data: null, message: ["unsupported harness session"] },
    };
    local.is_stopped = true;
    null
  }
}
