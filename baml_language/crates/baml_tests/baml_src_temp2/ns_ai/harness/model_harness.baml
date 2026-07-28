// A small in-process harness adapter backed by any CompletionProvider. It is
// useful both with a hosted model and as the blueprint for an external
// coding-harness implementation.
class ModelHarness {
  inner: Provider,
  sessions: LocalHarnessSession[],
}

function _model_harness_task<T>(
  inner: Provider,
  session: LocalHarnessSession,
  task: Task<T>,
) -> Task<T> throws never {
  let rebound = task.with_provider(inner);
  if (session.steering.length() == 0) {
    return rebound;
  }
  let guidance = session.steering.join("\n");
  //## Apply session steering after provider rebinding so with_provider cannot
  // discard the control-plane instruction by re-rendering the original recipe.
  Task<T> {
    ...rebound,
    prompt: baml.llm.assemble_prompt_ast(
      ["", "\nHarness steering (follow exactly):\n", ""],
      [root.ai._prompt_text_content(rebound.prompt), guidance],
    ),
  }
}

implements Provider for ModelHarness {
  function name(self) -> string throws never { "model-harness(" + self.inner.name() + ")" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements CompletionProvider for ModelHarness {
  function complete<T>(self, task: Task<T>) -> ResponseWithMetadata<T> throws baml.errors.UnknownError {
    //# Open an isolated harness session
    let session = self.open(HarnessOptions {
      cwd: null,
      permission_mode: null,
      sandbox: null,
      attributes: {},
    });
    //# Run the task inside the session
    let run = self.run<T>(session, task, null);
    //# Return harness output as a provider response
    ResponseWithMetadata<T> {
      value: run.value,
      meta: ResponseMetadata {
        provider: self.name(),
        model: null,
        request_id: session.id(),
        finish_reason: "harness_complete",
        usage: null,
        attributes: {},
        raw: null,
      },
      conversation: null,
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
      options: options,
      history: MessageHistory.empty(),
      is_stopped: false,
      is_cleaned: false,
      steering: [],
      cleanup_count: 0,
    };
    self.sessions.push(session);
    session
  }

  function run<T>(
    self,
    session: HarnessSession,
    task: Task<T>,
    on_event: ((AgentEvent) -> null throws never)?,
  ) -> HarnessRun<T> throws baml.errors.UnknownError {
    //# Validate the harness session
    let local = match (session) {
      let value: LocalHarnessSession => value,
      _ => throw baml.errors.UnknownError { data: null, message: ["unsupported harness session"] },
    };
    if (local.is_stopped) {
      throw baml.errors.UnknownError { data: null, message: ["harness session is stopped"] };
    }
    //# Resolve the backing completion provider
    let provider = match (self.inner) {
      let capability: CompletionProvider => capability,
      _ => throw baml.errors.UnknownError { data: null, message: ["harness inner cannot complete"] },
    };
    let events: AgentEvent[] = [];
    //# Announce the harness run before starting blocking provider work
    _emit_harness_event(
      ModelStartedEvent {
        id: local.session_id,
        provider: self.inner.name(),
      },
      events,
      on_event,
    );
    //# Run the model through the harness
    let effective_task = _model_harness_task<T>(self.inner, local, task);
    let response = provider.complete<T>(effective_task) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["harness model run failed"] },
    };
    //# Save conversation and resumable coordinates
    local.history = effective_task.messages().to_history();
    match (response.meta.usage) {
      let usage: Usage => _emit_harness_event(
        UsageEvent { id: local.session_id, usage: usage },
        events,
        on_event,
      ),
      null => null,
    };
    _emit_harness_event(
      RunFinishedEvent { id: local.session_id, outcome: "done" },
      events,
      on_event,
    );
    HarnessRun<T> {
      value: response.value,
      events: events,
      token: self.save_session(local),
      history: local.history,
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
