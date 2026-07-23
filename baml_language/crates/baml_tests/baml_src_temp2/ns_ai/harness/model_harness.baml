// A small in-process harness adapter backed by any CompletionProvider. It is useful
// both as a real OpenAI/Anthropic integration adapter and as the blueprint for
// an external coding-harness implementation.
class ModelHarness {
  inner: Provider,
  sessions: LocalHarnessSession[],
}

function model_harness_task<T>(
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
      [baml.llm.prompt_to_text(rebound.prompt), guidance],
    ),
  }
}

class ModelHarnessEventStream<TPartial, T> {
  owner: ModelHarness,
  session: LocalHarnessSession,
  task: Task<T>,
  inner: baml.llm.Stream<TPartial, T>,
  emitted: AgentEvent[],
  started_emitted: bool,
  is_closed: bool,
  cached_run: HarnessRun<T>?,

  function cleanup(self) -> void throws never {
    if (!self.is_closed) {
      self.inner._sse.close();
      self.is_closed = true;
    }
  }

  implements HarnessEventStream<T> {
    function next(self) -> AgentEvent | baml.stream.StreamFinished
        throws baml.errors.UnknownError {
      if (!self.started_emitted) {
        let started: AgentEvent = ModelStartedEvent {
          id: self.session.session_id,
          provider: self.owner.inner.name(),
        };
        self.started_emitted = true;
        self.emitted.push(started);
        return started;
      }
      if (self.is_closed) { return baml.stream.StreamFinished {}; }
      //# Pull one actual provider partial and normalize it as a harness event
      let next = self.inner.next() catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["harness provider stream failed"],
        },
      };
      match (next) {
        let done: baml.stream.StreamFinished => {
          self.is_closed = true;
          done
        },
        let partial: TPartial => {
          let text = baml.json.to_string<TPartial>(partial) catch (e) {
            _ => throw baml.errors.UnknownError {
              data: e,
              message: ["harness partial serialization failed"],
            },
          };
          let event: AgentEvent = TextDeltaEvent {
            id: self.session.session_id,
            text: text,
            metadata: { "source": "provider_stream" },
          };
          self.emitted.push(event);
          event
        },
      }
    }

    function final(self) -> HarnessRun<T> throws baml.errors.UnknownError {
      match (self.cached_run) {
        let run: HarnessRun<T> => return run,
        null => null,
      };
      //# Drain and validate the provider's authoritative typed final value
      let value = self.inner.final() catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["harness provider stream finalization failed"],
        },
      };
      self.is_closed = true;
      self.session.history = self.task.messages().to_history();
      let finished: AgentEvent = RunFinishedEvent {
        id: self.session.session_id,
        outcome: "done",
      };
      self.emitted.push(finished);
      let run = HarnessRun<T> {
        value: value,
        events: self.emitted,
        token: self.owner.save_session(self.session),
        history: self.session.history,
      };
      self.cached_run = run;
      run
    }
  }
  implements Resource {}
}

implements Provider for ModelHarness {
  function name(self) -> string throws never { "model-harness(" + self.inner.name() + ")" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements CompletionProvider for ModelHarness {
  function complete<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
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
    //# Resolve the backing completion provider
    let provider = match (self.inner) {
      let capability: CompletionProvider => capability,
      _ => throw baml.errors.UnknownError { data: null, message: ["harness inner cannot complete"] },
    };
    //# Run the model through the harness
    let effective_task = model_harness_task<T>(self.inner, local, task);
    let response = provider.complete<T>(effective_task) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["harness model run failed"] },
    };
    //# Save conversation and resumable coordinates
    local.history = effective_task.messages().to_history();
    let events: AgentEvent[] = [ModelStartedEvent {
      id: local.session_id,
      provider: self.inner.name(),
    }];
    match (response.meta.usage) {
      let usage: Usage => events.push(UsageEvent { id: local.session_id, usage: usage }),
      null => null,
    };
    events.push(RunFinishedEvent { id: local.session_id, outcome: "done" });
    HarnessRun<T> {
      value: response.value,
      events: events,
      token: self.save_session(local),
      history: local.history,
    }
  }

  function stream<TPartial, T>(self, session: HarnessSession, task: Task<T>)
      -> HarnessEventStream<T> throws baml.errors.UnknownError {
    let local = match (session) {
      let value: LocalHarnessSession => value,
      _ => throw baml.errors.UnknownError { data: null, message: ["unsupported harness session"] },
    };
    if (local.is_stopped) {
      throw baml.errors.UnknownError { data: null, message: ["harness session is stopped"] };
    }
    let provider = match (self.inner) {
      let capability: StreamingProvider => capability,
      _ => throw baml.errors.UnknownError { data: null, message: ["harness inner cannot stream"] },
    };
    let effective_task = model_harness_task<T>(self.inner, local, task);
    //# Retain the real provider stream behind the harness event resource
    let stream = provider.stream<TPartial, T>(effective_task) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["harness stream open failed"] },
    };
    ModelHarnessEventStream<TPartial, T> {
      owner: self,
      session: local,
      task: effective_task,
      inner: stream,
      emitted: [],
      started_emitted: false,
      is_closed: false,
      cached_run: null,
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
