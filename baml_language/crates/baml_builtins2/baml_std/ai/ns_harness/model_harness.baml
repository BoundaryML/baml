// A small in-process harness adapter backed by any ai.AgentProvider. It is
// useful both with a hosted model and as the blueprint for an external
// coding-harness implementation.
class ModelHarness {
  inner: ai.Provider,
  sessions: LocalHarnessSession[],
}

function _model_harness_task<T>(
  inner: ai.Provider,
  session: LocalHarnessSession,
  task: ai.Task<T>,
) -> ai.Task<T> throws ai.InvalidRequest {
  let rebound = task.with_provider(inner);
  if (session.steering.length() == 0) {
    return rebound;
  }
  let guidance = session.steering.join("\n");
  //## Apply session steering after provider rebinding so with_provider cannot
  // discard the control-plane instruction by re-rendering the original recipe.
  ai.Task<T> {
    ...rebound,
    prompt: baml.llm.assemble_prompt_ast(
      ["", "\nHarness steering (follow exactly):\n", ""],
      [ai.messages._prompt_text_content(rebound.prompt), guidance],
    ),
  }
}

implements ai.Provider for ModelHarness {
  function name(self) -> string throws never { "model-harness(" + self.inner.name() + ")" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements Harness for ModelHarness {
  function label(self) -> string throws never { "model-harness(" + self.inner.name() + ")" }
  function open(self, options: HarnessOptions) -> HarnessSession throws never {
    let session = LocalHarnessSession {
      session_id: "session-" + (self.sessions.length() + 1).to_string(),
      options: options,
      history: ai.MessageHistory.empty(),
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
    task: ai.Task<T>,
    on_event: ((ai.observe.AgentEvent) -> null throws never)?,
  ) -> HarnessRun<T> throws ai.InvalidRequest | baml.errors.UnknownError {
    //# Validate the harness session
    let local = match (session) {
      let value: LocalHarnessSession => value,
      _ => throw baml.errors.UnknownError.with_message<never>(null, "unsupported harness session"),
    };
    if (local.is_stopped) {
      throw baml.errors.UnknownError.with_message<never>(null, "harness session is stopped");
    }
    //# Resolve the backing Agent provider
    let provider = match (self.inner) {
      let capability: ai.AgentProvider => capability,
      _ => throw baml.errors.UnknownError.with_message<never>(null, "harness inner cannot run an Agent"),
    };
    let events: ai.observe.AgentEvent[] = [];
    //# Announce the harness run before starting blocking provider work
    _emit_harness_event(
      ai.observe.ModelStartedEvent {
        id: local.session_id,
        provider: self.inner.name(),
      },
      events,
      on_event,
    );
    //# Run the model through the harness
    let effective_task = _model_harness_task<T>(self.inner, local, task);
    let response = ai.internal._run_agent_to_response<T>(
      effective_task.with_provider(provider),
    ) catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(e, "harness model run failed"),
    };
    //# Save the completed conversation, including model and tool turns.
    local.history = match (response.conversation) {
      let conversation: ai.Conversation => conversation.messages().to_history(),
      null => effective_task.messages().to_history(),
    };
    match (response.metadata.usage) {
      let usage: ai.Usage => _emit_harness_event(
        ai.observe.UsageEvent { id: local.session_id, usage: usage },
        events,
        on_event,
      ),
      null => null,
    };
    _emit_harness_event(
      ai.observe.RunFinishedEvent { id: local.session_id, outcome: "done" },
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
      throw baml.errors.UnknownError.with_message<never>(token, "harness token does not belong to this harness/runtime/version");
    }
    //# Find the saved local session
    for (let session in self.sessions) {
      if (session.session_id == token.session_id) { return session; }
    }
    throw baml.errors.UnknownError.with_message<never>(token, "harness session not found")
  }

  function steer(self, session: HarnessSession, instruction: string) -> null
      throws baml.errors.UnknownError {
    let local = match (session) {
      let value: LocalHarnessSession => value,
      _ => throw baml.errors.UnknownError.with_message<never>(null, "unsupported harness session"),
    };
    local.steering.push(instruction);
    null
  }

  function interrupt(self, session: HarnessSession) -> null throws baml.errors.UnknownError {
    let local = match (session) {
      let value: LocalHarnessSession => value,
      _ => throw baml.errors.UnknownError.with_message<never>(null, "unsupported harness session"),
    };
    local.is_stopped = true;
    null
  }
}
