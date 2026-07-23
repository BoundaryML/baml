// 42 — the shared root.ai.Harness contract over two deliberately different
// external runtimes. Only adapter-specific state and control verbs live here.

class S42HarnessSession {
  id: string,
  runtime: string,
  transcript: string[],
  status: string,
}

function s42_check_session(session: S42HarnessSession, want: string) -> null
    throws baml.errors.UnknownError {
  if (session.runtime != want) {
    throw baml.errors.UnknownError {
      data: session,
      message: ["stored session belongs to " + session.runtime + ", not " + want],
    };
  }
  if (session.status == "destroyed") {
    throw baml.errors.UnknownError {
      data: session.id,
      message: ["session expired or was destroyed"],
    };
  }
  null
}

function s42_meta(provider: string, runtime: string, session_id: string) -> root.ai.Meta throws never {
  root.ai.Meta {
    provider: provider,
    model: null,
    request_id: session_id,
    finish_reason: "harness_turn_done",
    usage: root.ai.Usage { input_tokens: 0, output_tokens: 0 },
    attributes: { "runtime": runtime },
    raw: null,
  }
}

function s42_run<T>(
  provider: string,
  runtime: string,
  reply: string,
  session: S42HarnessSession,
  request: root.ai.Request<T>,
) -> root.ai.AgentRun<T> throws baml.errors.UnknownError {
  s42_check_session(session, runtime);
  session.status = "attached";
  let prompt = baml.llm.prompt_to_text(request.prompt);
  session.transcript.push("user: " + prompt);
  session.transcript.push("assistant: " + reply);
  let value: T = baml.json.from_string<T>(reply) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: [runtime + " harness reply did not match the task output"],
    },
  };
  let meta = s42_meta(provider, runtime, session.id);
  let blocks: root.ai.ModelBlock[] = [
    root.ai.TextBlock { text: reply, provider_name: provider, metadata: null },
  ];
  let conversation = root.ai.conversation_from_messages(request.messages());
  conversation.messages.push(root.ai.ConversationMessage {
    role: "assistant",
    parts: [root.ai.MessagePart.from_text(reply)],
    provider_metadata: null,
  });
  root.ai.AgentRun<T> {
    done: root.ai.Done<T> { value: value, meta: meta },
    budget_reached: null,
    handoff: null,
    turns: [root.ai.AgentTurn<T> {
      index: 1,
      blocks: blocks,
      tool_calls: [],
      tool_results: [],
      value: value,
      meta: meta,
    }],
    meta: meta,
    session_token: null,
    conversation: conversation,
  }
}

function s42_save_session(
  provider: string,
  session: S42HarnessSession,
) -> root.ai.HarnessSessionToken throws baml.errors.UnknownError {
  let sealed = baml.json.to_string(session) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: ["failed to seal harness session"] },
  };
  root.ai.HarnessSessionToken {
    provider: provider,
    runtime: session.runtime,
    session_id: session.id,
    version: 1,
    sealed: sealed,
    expires_at: null,
  }
}

function s42_restore_session(
  provider: string,
  runtime: string,
  token: root.ai.HarnessSessionToken,
) -> S42HarnessSession throws baml.errors.UnknownError {
  if (token.provider != provider || token.runtime != runtime || token.version != 1) {
    throw baml.errors.UnknownError {
      data: token,
      message: ["session token does not belong to this harness adapter"],
    };
  }
  let session = baml.json.from_string<S42HarnessSession>(token.sealed) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: ["invalid sealed harness session"] },
  };
  s42_check_session(session, runtime);
  session
}

function s42_stream_result<T>(
  run: root.ai.AgentRun<T>,
  provider: string,
  include_tool: bool,
) -> root.ai.AgentEventStream<T> throws never {
  let events: root.ai.AgentEvent[] = [
    root.ai.ModelStartedEvent { id: run.meta.request_id, provider: provider },
    root.ai.TextDeltaEvent {
      id: run.meta.request_id,
      text: "inspecting repository",
      metadata: null,
    },
  ];
  if (include_tool) {
    let call = root.ai.ToolCall { id: "read-1", name: "Read", args: `{"path":"tests.baml"}` };
    events.push(root.ai.ToolCallProposedEvent { id: run.meta.request_id, call: call });
    events.push(root.ai.ToolCallStartedEvent { id: run.meta.request_id, call: call });
    events.push(root.ai.ToolCallFinishedEvent {
      id: run.meta.request_id,
      call: call,
      result: root.ai.ToolResult { id: call.id, output: "read ok" },
    });
  }
  events.push(root.ai.UsageUpdatedEvent {
    id: run.meta.request_id,
    usage: run.meta.usage ?? root.ai.Usage { input_tokens: 0, output_tokens: 0 },
  });
  events.push(root.ai.TurnFinishedEvent { id: run.meta.request_id, turn: 1 });
  events.push(root.ai.RunFinishedEvent { id: run.meta.request_id, outcome: "done" });
  root.ai.BufferedAgentEventStream<T> { events: events, cursor: 0, result: run }
}

class S42ClaudeHarness {
  reply: string,
  next_id: int,
  sandbox: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "claude-code-harness" }
  }

  implements root.ai.HarnessControlPlane {
    function supported_verbs(self) -> string[] throws never {
      if (self.sandbox == "local") {
        ["follow_up", "interrupt", "stop"]
      } else {
        ["follow_up", "interrupt", "stop", "rewind_files"]
      }
    }
  }

  implements root.ai.Harness {
    type HarnessSession = S42HarnessSession

    function open(self, options: root.ai.SessionOptions) -> S42HarnessSession
        throws baml.errors.UnknownError {
      self.next_id = self.next_id + 1;
      S42HarnessSession {
        id: "claude-" + self.next_id.to_string(),
        runtime: "claude-code",
        transcript: [],
        status: "parked",
      }
    }
    function run<T>(self, session: S42HarnessSession, request: root.ai.Request<T>)
        -> root.ai.AgentRun<T> throws baml.errors.UnknownError {
      s42_run<T>(self.display_name(), "claude-code", self.reply, session, request)
    }
    function stream<T>(self, session: S42HarnessSession, request: root.ai.Request<T>)
        -> root.ai.AgentEventStream<T> throws baml.errors.UnknownError {
      s42_stream_result<T>(self.run<T>(session, request), self.display_name(), true)
    }
    function save_session(self, session: S42HarnessSession) -> root.ai.HarnessSessionToken
        throws baml.errors.UnknownError {
      session.status = "parked";
      s42_save_session(self.display_name(), session)
    }
    function restore_session(self, token: root.ai.HarnessSessionToken) -> S42HarnessSession
        throws baml.errors.UnknownError {
      s42_restore_session(self.display_name(), "claude-code", token)
    }
    function stop(self, session: S42HarnessSession) -> null throws baml.errors.UnknownError {
      s42_check_session(session, "claude-code");
      session.status = "stopped";
      null
    }
    function destroy(self, session: S42HarnessSession) -> null throws baml.errors.UnknownError {
      s42_check_session(session, "claude-code");
      session.status = "destroyed";
      session.transcript = [];
      null
    }
  }
}

class S42PiHarness {
  reply: string,
  next_id: int,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "pi-harness" }
  }

  implements root.ai.HarnessControlPlane {
    function supported_verbs(self) -> string[] throws never { ["steer", "follow_up"] }
  }

  implements root.ai.Harness {
    type HarnessSession = S42HarnessSession

    function open(self, options: root.ai.SessionOptions) -> S42HarnessSession
        throws baml.errors.UnknownError {
      self.next_id = self.next_id + 1;
      S42HarnessSession {
        id: "pi-node-" + self.next_id.to_string(),
        runtime: "pi",
        transcript: match (options.resume_from) {
          null => [],
          let parent: string => ["forked-from: " + parent],
        },
        status: "parked",
      }
    }
    function run<T>(self, session: S42HarnessSession, request: root.ai.Request<T>)
        -> root.ai.AgentRun<T> throws baml.errors.UnknownError {
      s42_run<T>(self.display_name(), "pi", self.reply, session, request)
    }
    function stream<T>(self, session: S42HarnessSession, request: root.ai.Request<T>)
        -> root.ai.AgentEventStream<T> throws baml.errors.UnknownError {
      s42_stream_result<T>(self.run<T>(session, request), self.display_name(), false)
    }
    function save_session(self, session: S42HarnessSession) -> root.ai.HarnessSessionToken
        throws baml.errors.UnknownError {
      session.status = "parked";
      s42_save_session(self.display_name(), session)
    }
    function restore_session(self, token: root.ai.HarnessSessionToken) -> S42HarnessSession
        throws baml.errors.UnknownError {
      s42_restore_session(self.display_name(), "pi", token)
    }
    function stop(self, session: S42HarnessSession) -> null throws baml.errors.UnknownError {
      s42_check_session(session, "pi");
      session.status = "parked-after-stop";
      null
    }
    function destroy(self, session: S42HarnessSession) -> null throws baml.errors.UnknownError {
      s42_check_session(session, "pi");
      session.status = "destroyed";
      session.transcript = [];
      null
    }
  }
}

function s42_events<T>(request: root.ai.Request<T>) -> root.ai.AgentEventLog<T>
    throws baml.errors.CallError | baml.errors.UnknownError | baml.errors.Unsupported {
  let provider = match (request.provider) {
    let agent: root.ai.HarnessAgent => agent.raw(),
    _ => request.provider,
  };
  let rebound = request.for_provider(provider);
  match (provider) {
    let harness: root.ai.Harness => {
      root.ai.collect_agent_stream<T>(root.ai.stream_harness<T>(rebound))
    },
    let generate: root.ai.Generate => {
      let response = generate.generate<T>(rebound);
      let done = root.ai.Done<T> { value: response.value, meta: response.meta };
      let run = root.ai.AgentRun<T> {
        done: done,
        budget_reached: null,
        handoff: null,
        turns: [root.ai.AgentTurn<T> {
          index: 1,
          blocks: [],
          tool_calls: [],
          tool_results: [],
          value: response.value,
          meta: response.meta,
        }],
        meta: response.meta,
        session_token: null,
        conversation: root.ai.conversation_from_messages(request.messages()),
      };
      root.ai.AgentEventLog<T> {
        events: [root.ai.RunFinishedEvent { id: response.meta.request_id, outcome: "done" }],
        run: run,
      }
    },
    _ => throw baml.errors.Unsupported {
      message: "provider has neither Harness nor Generate",
    },
  }
}

// Server-authoritative storage persists only provider-controlled tokens.
class S42HandleStore {
  handles: root.ai.HarnessSessionToken[],

  function put(self, token: root.ai.HarnessSessionToken) -> null throws never {
    let updated: root.ai.HarnessSessionToken[] = [];
    let replaced = false;
    for (let saved in self.handles) {
      if (saved.session_id == token.session_id) {
        updated.push(token);
        replaced = true;
      } else {
        updated.push(saved);
      }
    }
    if (!replaced) { updated.push(token); }
    self.handles = updated;
    null
  }
  function get(self, id: string) -> root.ai.HarnessSessionToken throws baml.errors.UnknownError {
    for (let saved in self.handles) {
      if (saved.session_id == id) { return saved; }
    }
    throw baml.errors.UnknownError { data: id, message: ["session token expired or was not found"] }
  }
  function drop(self, id: string) -> null throws never {
    let kept: root.ai.HarnessSessionToken[] = [];
    for (let saved in self.handles) {
      if (saved.session_id != id) { kept.push(saved); }
    }
    self.handles = kept;
    null
  }
}

class S42Turn<T> {
  token: root.ai.HarnessSessionToken,
  value: T,
  run: root.ai.AgentRun<T>,
}

class S42ServerSession {
  agent: root.ai.HarnessAgent,
  store: S42HandleStore,

  function open<T>(self, request: root.ai.Request<T>, options: root.ai.SessionOptions)
      -> S42Turn<T> throws baml.errors.UnknownError | baml.errors.Unsupported {
    let run = root.ai.run_harness<T>(request.for_provider(self.agent.raw()), options = options);
    let token = run.session_token ?? baml.sys.panic("harness did not return a session token");
    self.store.put(token);
    S42Turn<T> { token: token, value: root.ai.agent_run_value(run), run: run }
  }
  function resume<T>(self, id: string, request: root.ai.Request<T>)
      -> S42Turn<T> throws baml.errors.UnknownError | baml.errors.Unsupported {
    let token = self.store.get(id);
    let run = root.ai.resume_harness<T>(request.for_provider(self.agent.raw()), token);
    let next = run.session_token ?? baml.sys.panic("harness did not return a session token");
    self.store.put(next);
    S42Turn<T> { token: next, value: root.ai.agent_run_value(run), run: run }
  }
  function close(self, id: string) -> null
      throws baml.errors.UnknownError | baml.errors.Unsupported {
    root.ai.destroy_harness(self.agent.raw(), self.store.get(id));
    self.store.drop(id);
    null
  }
}
