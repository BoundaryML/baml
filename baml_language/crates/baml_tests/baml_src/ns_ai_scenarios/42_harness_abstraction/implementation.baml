// 42 — one harness facade over several runtimes.
//
// The BEPv2 rule is kept explicit: a provider owns capabilities; stateful work
// returns a resource/coordinate. SessionHandle is serializable application data,
// while the configured provider remains process-local. A server persists the
// handle and routes it back to the runtime named by `runtime`.

class S42SessionOpts {
  cwd: string?,
  resume_from: string?,
  permission_mode: string?,
}

class S42SessionHandle {
  id: string,
  runtime: string,
  transcript: string[],
  status: string,
}

class S42AgentEvent {
  kind: string,
  text: string?,
  detail: string?,
}

class S42EventLog<T> {
  events: S42AgentEvent[],
  final_value: T,
  session: S42SessionHandle?,
}

// The portable intersection. Provider-specific commands are intentionally not
// added here; callers negotiate ControlPlane or use HarnessAgent.raw().
interface S42Harness requires root.ai.Provider {
  function create_session(self, options: S42SessionOpts) -> S42SessionHandle
    throws baml.errors.UnknownError
  function attach<T>(
    self,
    handle: S42SessionHandle,
    request: root.ai.Request<T>,
  ) -> T throws baml.errors.UnknownError
  function detach(self, handle: S42SessionHandle) -> S42SessionHandle
    throws baml.errors.UnknownError
  function stop(self, handle: S42SessionHandle) -> null throws baml.errors.UnknownError
  function destroy(self, handle: S42SessionHandle) -> null throws baml.errors.UnknownError
}

interface S42ControlPlane requires root.ai.Provider {
  function supported_verbs(self) -> string[] throws never
}

// Lossless rich events are a custom capability. The source design projects
// these into baml.llm.Stream at a host boundary; this all-BAML reference keeps
// them as a deterministic event log and separately forwards canonical Streaming
// whenever the wrapped provider genuinely implements root.ai.Streaming.
interface S42AgentStreaming requires root.ai.Provider {
  function events<T>(
    self,
    request: root.ai.Request<T>,
    handle: S42SessionHandle?,
  ) -> S42EventLog<T> throws baml.errors.UnknownError
}

function s42_parse_reply<T>(reply: string, runtime: string) -> T
    throws baml.errors.UnknownError {
  baml.sap.parse<T>(reply) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: [runtime + " harness reply did not match the task output"],
    },
  }
}

function s42_check_runtime(handle: S42SessionHandle, want: string) -> null
    throws baml.errors.UnknownError {
  if (handle.runtime != want) {
    throw baml.errors.UnknownError {
      data: handle,
      message: ["stored session belongs to " + handle.runtime + ", not " + want],
    };
  }
  if (handle.status == "destroyed") {
    throw baml.errors.UnknownError {
      data: handle.id,
      message: ["session expired or was destroyed"],
    };
  }
  null
}

class S42ClaudeHarness {
  reply: string,
  next_id: int,
  sandbox: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "claude-code-harness" }
  }

  implements S42ControlPlane {
    function supported_verbs(self) -> string[] throws never {
      if (self.sandbox == "local") {
        ["follow_up", "interrupt", "stop"]
      } else {
        ["follow_up", "interrupt", "stop", "rewind_files"]
      }
    }
  }

  implements S42Harness {
    function create_session(self, options: S42SessionOpts) -> S42SessionHandle
        throws baml.errors.UnknownError {
      self.next_id = self.next_id + 1;
      S42SessionHandle {
        id: "claude-" + self.next_id.to_string(),
        runtime: "claude-code",
        transcript: [],
        status: "parked",
      }
    }

    function attach<T>(
      self,
      handle: S42SessionHandle,
      request: root.ai.Request<T>,
    ) -> T throws baml.errors.UnknownError {
      s42_check_runtime(handle, "claude-code");
      handle.status = "attached";
      handle.transcript.push("user: " + baml.llm.prompt_to_text(request.prompt));
      handle.transcript.push("assistant: " + self.reply);
      s42_parse_reply<T>(self.reply, "claude-code")
    }

    function detach(self, handle: S42SessionHandle) -> S42SessionHandle
        throws baml.errors.UnknownError {
      s42_check_runtime(handle, "claude-code");
      handle.status = "parked";
      handle
    }

    function stop(self, handle: S42SessionHandle) -> null throws baml.errors.UnknownError {
      s42_check_runtime(handle, "claude-code");
      handle.status = "stopped";
      null
    }

    function destroy(self, handle: S42SessionHandle) -> null throws baml.errors.UnknownError {
      s42_check_runtime(handle, "claude-code");
      handle.status = "destroyed";
      handle.transcript = [];
      null
    }
  }

  implements S42AgentStreaming {
    function events<T>(
      self,
      request: root.ai.Request<T>,
      handle: S42SessionHandle?,
    ) -> S42EventLog<T> throws baml.errors.UnknownError {
      let value = s42_parse_reply<T>(self.reply, "claude-code");
      S42EventLog<T> {
        events: [
          S42AgentEvent { kind: "text_delta", text: "inspecting repository", detail: null },
          S42AgentEvent { kind: "tool_started", text: null, detail: "Read" },
          S42AgentEvent { kind: "turn_done", text: null, detail: null },
        ],
        final_value: value,
        session: handle,
      }
    }
  }
}

class S42PiHarness {
  reply: string,
  next_id: int,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "pi-harness" }
  }

  implements S42ControlPlane {
    function supported_verbs(self) -> string[] throws never {
      ["steer", "follow_up"]
    }
  }

  implements S42Harness {
    function create_session(self, options: S42SessionOpts) -> S42SessionHandle
        throws baml.errors.UnknownError {
      self.next_id = self.next_id + 1;
      S42SessionHandle {
        id: "pi-node-" + self.next_id.to_string(),
        runtime: "pi",
        transcript: match (options.resume_from) {
          null => [],
          let parent: string => ["forked-from: " + parent],
        },
        status: "parked",
      }
    }

    function attach<T>(
      self,
      handle: S42SessionHandle,
      request: root.ai.Request<T>,
    ) -> T throws baml.errors.UnknownError {
      s42_check_runtime(handle, "pi");
      handle.status = "attached";
      handle.transcript.push("user: " + baml.llm.prompt_to_text(request.prompt));
      handle.transcript.push("assistant: " + self.reply);
      s42_parse_reply<T>(self.reply, "pi")
    }

    function detach(self, handle: S42SessionHandle) -> S42SessionHandle
        throws baml.errors.UnknownError {
      s42_check_runtime(handle, "pi");
      handle.status = "parked";
      handle
    }

    // Pi cannot interrupt the server-side turn. Stop parks the subscription but
    // may not cancel work; the weaker semantic is explicit in the verb roster.
    function stop(self, handle: S42SessionHandle) -> null throws baml.errors.UnknownError {
      s42_check_runtime(handle, "pi");
      handle.status = "parked-after-stop";
      null
    }

    function destroy(self, handle: S42SessionHandle) -> null throws baml.errors.UnknownError {
      s42_check_runtime(handle, "pi");
      handle.status = "destroyed";
      handle.transcript = [];
      null
    }
  }

  implements S42AgentStreaming {
    function events<T>(
      self,
      request: root.ai.Request<T>,
      handle: S42SessionHandle?,
    ) -> S42EventLog<T> throws baml.errors.UnknownError {
      S42EventLog<T> {
        events: [
          S42AgentEvent { kind: "text_delta", text: "pi is working", detail: null },
          S42AgentEvent { kind: "turn_done", text: null, detail: null },
        ],
        final_value: s42_parse_reply<T>(self.reply, "pi"),
        session: handle,
      }
    }
  }
}

// The unifying facade is itself a Provider. It can wrap a harness or an ordinary
// model; it claims only interactions it can honestly forward or normalize.
class S42HarnessAgent {
  inner: root.ai.Provider,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "harness-agent:" + self.inner.display_name() }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      match (self.inner) {
        let generate: root.ai.Generate => generate.generate<T>(request.for_provider(self.inner)),
        let harness: S42Harness => {
          let handle = harness.create_session(S42SessionOpts {
            cwd: null, resume_from: null, permission_mode: null,
          });
          let value = harness.attach<T>(handle, request.for_provider(self.inner));
          let parked = harness.detach(handle);
          root.ai.Response<T> {
            value: value,
            meta: root.ai.Meta {
              provider: self.inner.display_name(),
              model: null,
              request_id: parked.id,
              finish_reason: "harness_turn_done",
              usage: null,
              attributes: { "runtime": parked.runtime },
              raw: null,
            },
          }
        },
        _ => throw baml.errors.Unsupported {
          message: "inner provider is neither Generate nor S42Harness",
        },
      }
    }
  }

  // Canonical stream forwarding makes an agent over a normal streaming model
  // fit exactly the same UI consumer. Harness event projection remains the
  // explicit S42AgentStreaming seam until a BAML stream constructor exists.
  implements root.ai.Streaming {
    function stream<TPartial, T>(
      self,
      request: root.ai.Request<T>,
    ) -> baml.llm.Stream<TPartial, T>
        throws baml.errors.StreamError | baml.errors.UnknownError {
      match (self.inner) {
        let streaming: root.ai.Streaming => streaming.stream<TPartial, T>(request.for_provider(self.inner)),
        _ => throw baml.errors.Unsupported {
          message: "inner harness exposes rich events but no canonical stream projection",
        },
      }
    }
  }

  implements S42Harness {
    function create_session(self, options: S42SessionOpts) -> S42SessionHandle
        throws baml.errors.UnknownError {
      match (self.inner) {
        let harness: S42Harness => harness.create_session(options),
        _ => throw baml.errors.UnknownError {
          data: self.inner,
          message: ["inner provider has no cross-request harness lifecycle"],
        },
      }
    }

    function attach<T>(
      self,
      handle: S42SessionHandle,
      request: root.ai.Request<T>,
    ) -> T throws baml.errors.UnknownError {
      match (self.inner) {
        let harness: S42Harness => harness.attach<T>(handle, request.for_provider(self.inner)),
        _ => throw baml.errors.UnknownError {
          data: self.inner,
          message: ["inner provider cannot attach a harness session"],
        },
      }
    }

    function detach(self, handle: S42SessionHandle) -> S42SessionHandle
        throws baml.errors.UnknownError {
      match (self.inner) {
        let harness: S42Harness => harness.detach(handle),
        _ => throw baml.errors.UnknownError { data: self.inner, message: ["cannot detach"] },
      }
    }

    function stop(self, handle: S42SessionHandle) -> null throws baml.errors.UnknownError {
      match (self.inner) {
        let harness: S42Harness => harness.stop(handle),
        _ => throw baml.errors.UnknownError { data: self.inner, message: ["cannot stop"] },
      }
    }

    function destroy(self, handle: S42SessionHandle) -> null throws baml.errors.UnknownError {
      match (self.inner) {
        let harness: S42Harness => harness.destroy(handle),
        _ => throw baml.errors.UnknownError { data: self.inner, message: ["cannot destroy"] },
      }
    }
  }

  implements S42ControlPlane {
    function supported_verbs(self) -> string[] throws never {
      match (self.inner) {
        let controls: S42ControlPlane => controls.supported_verbs(),
        _ => [],
      }
    }
  }

  implements S42AgentStreaming {
    function events<T>(
      self,
      request: root.ai.Request<T>,
      handle: S42SessionHandle?,
    ) -> S42EventLog<T> throws baml.errors.UnknownError {
      match (self.inner) {
        let rich: S42AgentStreaming => rich.events<T>(request.for_provider(self.inner), handle),
        _ => throw baml.errors.UnknownError {
          data: self.inner,
          message: ["inner provider has no rich agent event stream"],
        },
      }
    }
  }

  function raw(self) -> root.ai.Provider throws never { self.inner }
}

function s42_events<T>(request: root.ai.Request<T>) -> S42EventLog<T>
    throws baml.errors.CallError | baml.errors.UnknownError {
  match (request.provider) {
    let rich: S42AgentStreaming => rich.events<T>(request, null),
    let generate: root.ai.Generate => {
      let response = generate.generate<T>(request);
      S42EventLog<T> {
        events: [S42AgentEvent { kind: "turn_done", text: null, detail: "generate fallback" }],
        final_value: response.value,
        session: null,
      }
    },
    _ => throw baml.errors.Unsupported {
      message: "provider has neither rich agent events nor Generate",
    },
  }
}

// Server-authoritative storage owns handles, not providers or credentials.
class S42HandleStore {
  handles: S42SessionHandle[],

  function put(self, handle: S42SessionHandle) -> null throws never {
    for (let saved in self.handles) {
      if (saved.id == handle.id) {
        saved.runtime = handle.runtime;
        saved.transcript = handle.transcript;
        saved.status = handle.status;
        return null;
      }
    }
    self.handles.push(handle);
    null
  }

  function get(self, id: string) -> S42SessionHandle throws baml.errors.UnknownError {
    for (let saved in self.handles) {
      if (saved.id == id) { return saved; }
    }
    throw baml.errors.UnknownError {
      data: id,
      message: ["session handle expired or was not found"],
    }
  }

  function drop(self, id: string) -> null throws never {
    let kept: S42SessionHandle[] = [];
    for (let saved in self.handles) {
      if (saved.id != id) { kept.push(saved); }
    }
    self.handles = kept;
    null
  }
}

class S42Turn<T> {
  handle: S42SessionHandle,
  value: T,
}

class S42ServerSession {
  agent: S42HarnessAgent,
  store: S42HandleStore,

  function open<T>(
    self,
    request: root.ai.Request<T>,
    options: S42SessionOpts,
  ) -> S42Turn<T> throws baml.errors.UnknownError {
    let handle = self.agent.create_session(options);
    let value = self.agent.attach<T>(handle, request);
    let parked = self.agent.detach(handle);
    self.store.put(parked);
    S42Turn<T> { handle: parked, value: value }
  }

  function resume<T>(
    self,
    id: string,
    request: root.ai.Request<T>,
  ) -> S42Turn<T> throws baml.errors.UnknownError {
    let handle = self.store.get(id);
    let value = self.agent.attach<T>(handle, request);
    let parked = self.agent.detach(handle);
    self.store.put(parked);
    S42Turn<T> { handle: parked, value: value }
  }

  function close(self, id: string) -> null throws baml.errors.UnknownError {
    let handle = self.store.get(id);
    self.agent.destroy(handle);
    self.store.drop(id);
    null
  }
}
