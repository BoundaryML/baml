// 41 — harness deployment, written against the BEP-064 v2 request seam.
//
// The source scenario has three deliberately different host protocols:
// Claude JSONL, Pi JSON-RPC, and a Python generator. They are normalized by a
// user-authored Drivable capability. This reference keeps the protocol boundary
// visible and deterministic: each adapter opens a drive, pumps framed events,
// and settles one typed Request<T>. No subprocess or integration test is needed
// to prove the provider/capability/resource composition.

class S41HostMsg {
  kind: string,
  text: string?,
  tool_results: root.ai.ToolResult[]?,
}

class S41AgentMsg {
  kind: string,
  text: string?,
  tool_calls: root.ai.ToolCalls?,
  final_json: string?,
}

class S41Drive {
  transport: string,
  rendered_prompt: string,
  final_json: string,
  cursor: int,
  closed: bool,
}

function s41_meta(provider: string, transport: string) -> root.ai.Meta throws never {
  root.ai.Meta {
    provider: provider,
    model: "scripted-harness",
    request_id: null,
    finish_reason: "drive_complete",
    usage: null,
    attributes: { "transport": transport },
    raw: null,
  }
}

// A custom capability is ordinary BAML: Provider marker + interface + driver.
// open/pump/close expose the actual host protocol; drive is shared library glue.
interface S41Drivable requires root.ai.Provider {
  function open<T>(self, request: root.ai.Request<T>) -> S41Drive
    throws baml.errors.UnknownError
  function pump(self, drive: S41Drive, input: S41HostMsg?) -> S41AgentMsg?
    throws baml.errors.UnknownError
  function close_drive(self, drive: S41Drive) -> null throws never

  function drive<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
      throws baml.errors.UnknownError {
    let drive = self.open<T>(request);
    while (true) {
      let event = self.pump(drive, null);
      match (event) {
        null => {
          self.close_drive(drive);
          throw baml.errors.UnknownError {
            data: drive,
            message: ["harness ended without a final value"],
          };
        },
        let message: S41AgentMsg => {
          match (message.final_json) {
            null => {},
            let raw: string => {
              let value: T = baml.sap.parse<T>(raw) catch (e) {
                _ => {
                  self.close_drive(drive);
                  throw baml.errors.UnknownError {
                    data: e,
                    message: ["harness final value did not match the task output"],
                  };
                },
              };
              self.close_drive(drive);
              return root.ai.Response<T> {
                value: value,
                meta: s41_meta(request.provider_name(), drive.transport),
              };
            },
          }
        },
      }
    }
    baml.sys.panic("unreachable")
  }
}

function s41_open_drive<T>(
  transport: string,
  final_json: string,
  request: root.ai.Request<T>,
) -> S41Drive throws never {
  S41Drive {
    transport: transport,
    rendered_prompt: baml.llm.prompt_to_text(request.prompt),
    final_json: final_json,
    cursor: 0,
    closed: false,
  }
}

function s41_pump_script(drive: S41Drive) -> S41AgentMsg? throws never {
  if (drive.closed) { return null; }
  if (drive.cursor == 0) {
    drive.cursor = 1;
    return S41AgentMsg {
      kind: "text_delta",
      text: "working over " + drive.transport,
      tool_calls: null,
      final_json: null,
    };
  }
  if (drive.cursor == 1) {
    drive.cursor = 2;
    return S41AgentMsg {
      kind: "final",
      text: null,
      tool_calls: null,
      final_json: drive.final_json,
    };
  }
  null
}

// JSONL transport: one JSON object per line on stdin/stdout.
class S41ClaudeJsonl {
  final_json: string,
  busy_ids: string[],

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "claude-jsonl" }
  }

  implements S41Drivable {
    function open<T>(self, request: root.ai.Request<T>) -> S41Drive throws never {
      s41_open_drive<T>("jsonl", self.final_json, request)
    }
    function pump(self, drive: S41Drive, input: S41HostMsg?) -> S41AgentMsg? throws never {
      s41_pump_script(drive)
    }
    function close_drive(self, drive: S41Drive) -> null throws never {
      drive.closed = true;
      null
    }
  }
}

// JSON-RPC transport: request/response envelopes and notifications, same
// semantic Drivable capability.
class S41PiRpc {
  final_json: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "pi-json-rpc" }
  }

  implements S41Drivable {
    function open<T>(self, request: root.ai.Request<T>) -> S41Drive throws never {
      s41_open_drive<T>("json-rpc", self.final_json, request)
    }
    function pump(self, drive: S41Drive, input: S41HostMsg?) -> S41AgentMsg? throws never {
      s41_pump_script(drive)
    }
    function close_drive(self, drive: S41Drive) -> null throws never {
      drive.closed = true;
      null
    }
  }
}

// Generator transport: the host repeatedly advances a yielded event stream.
class S41AgentsGenerator {
  final_json: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "agents-generator" }
  }

  implements S41Drivable {
    function open<T>(self, request: root.ai.Request<T>) -> S41Drive throws never {
      s41_open_drive<T>("generator", self.final_json, request)
    }
    function pump(self, drive: S41Drive, input: S41HostMsg?) -> S41AgentMsg? throws never {
      s41_pump_script(drive)
    }
    function close_drive(self, drive: S41Drive) -> null throws never {
      drive.closed = true;
      null
    }
  }
}

// The custom negotiation driver: an ordinary Generate provider remains usable,
// while harness-only providers take the explicit Drivable arm.
function s41_run_with_meta<T>(request: root.ai.Request<T>) -> root.ai.Response<T>
    throws baml.errors.CallError | baml.errors.UnknownError {
  match (request.provider) {
    let generate: root.ai.Generate => generate.generate<T>(request),
    let harness: S41Drivable => harness.drive<T>(request),
    _ => throw baml.errors.Unsupported {
      message: "provider supports neither Generate nor S41Drivable: " + request.provider_name(),
    },
  }
}

function s41_run<T>(request: root.ai.Request<T>) -> T
    throws baml.errors.CallError | baml.errors.UnknownError {
  s41_run_with_meta<T>(request).value
}

// Runtime provider configuration belongs to application boot data, not to a
// static task declaration. This simple registry makes the seam inspectable.
class S41ProviderSettings {
  base_url: string?,
  api_key: string?,
  headers: map<string, string>?,
}

class S41RegistryEntry {
  name: string,
  settings: S41ProviderSettings,
}

class S41Registry {
  entries: S41RegistryEntry[],

  function register(self, name: string, settings: S41ProviderSettings) -> null throws never {
    for (let entry in self.entries) {
      if (entry.name == name) {
        entry.settings = settings;
        return null;
      }
    }
    self.entries.push(S41RegistryEntry { name: name, settings: settings });
    null
  }

  function resolve(self, name: string) -> S41ProviderSettings? throws never {
    for (let entry in self.entries) {
      if (entry.name == name) { return entry.settings; }
    }
    null
  }
}

// Durable is a second custom capability. The instance id is the serializable
// coordinate; the provider owns the single-writer lease and transcript state.
class S41Instance {
  id: string,
  runtime: string,
  transcript: string,
}

interface S41Durable requires root.ai.Provider {
  function attach(self, instance_id: string) -> S41Instance throws baml.errors.UnknownError
  function release(self, instance: S41Instance) -> null throws never
}

// Claude JSONL is durable in this reference because its transcript can be
// parked. Pi/generator deliberately do not claim the capability.
implements S41Durable for S41ClaudeJsonl {
  function attach(self, instance_id: string) -> S41Instance throws baml.errors.UnknownError {
    if (self.busy_ids.includes(instance_id)) {
      throw baml.errors.UnknownError {
        data: instance_id,
        message: ["durable instance already has a writer"],
      };
    }
    self.busy_ids.push(instance_id);
    S41Instance { id: instance_id, runtime: "claude-jsonl", transcript: "" }
  }

  function release(self, instance: S41Instance) -> null throws never {
    let kept: string[] = [];
    for (let id in self.busy_ids) {
      if (id != instance.id) { kept.push(id); }
    }
    self.busy_ids = kept;
    null
  }
}

class S41Trigger {
  kind: string,
  route: string?,
  schedule: string?,
}

class S41DeployedAgent {
  name: string,
  inner: root.ai.Provider,
  triggers: S41Trigger[],

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "deployment:" + self.name }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      s41_run_with_meta<T>(request.for_provider(self.inner))
    }
  }

  // Trigger handlers supply a stable instance key. Durable providers lease it;
  // stateless providers simply execute, which keeps ephemeral deployment honest.
  function invoke<T>(
    self,
    instance_id: string,
    request: root.ai.Request<T>,
  ) -> T throws baml.errors.CallError | baml.errors.UnknownError {
    match (self.inner) {
      let durable: S41Durable => {
        let instance = durable.attach(instance_id);
        let value = s41_run<T>(request.for_provider(self.inner)) catch (e) {
          _ => {
            durable.release(instance);
            throw e;
          },
        };
        durable.release(instance);
        value
      },
      _ => s41_run<T>(request.for_provider(self.inner)),
    }
  }
}

// Supervision is a provider wrapper, not a compiler mode. It preserves the
// Drivable interaction and records restart policy as ordinary typed fields.
class S41SubprocessSupervisor {
  inner: S41Drivable,
  max_restarts: int,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "supervised:" + self.inner.display_name() }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements S41Drivable {
    function open<T>(self, request: root.ai.Request<T>) -> S41Drive throws baml.errors.UnknownError {
      self.inner.open<T>(request.for_provider(self.inner))
    }
    function pump(self, drive: S41Drive, input: S41HostMsg?) -> S41AgentMsg?
        throws baml.errors.UnknownError {
      self.inner.pump(drive, input)
    }
    function close_drive(self, drive: S41Drive) -> null throws never {
      self.inner.close_drive(drive)
    }
  }
}

// Consumer-side deployment facade. The offline version behaves like an HTTP
// SDK provider; a real implementation would send the Request messages/schema to
// the deployment URL and SAP-parse its response.
class S41RemoteAgent {
  endpoint: string,
  deployment: string,
  reply: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "remote:" + self.deployment }
  }

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      let value: T = baml.sap.parse<T>(self.reply) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["remote deployment response did not match the task output"],
        },
      };
      root.ai.Response<T> {
        value: value,
        meta: root.ai.Meta {
          provider: "remote-deployment",
          model: self.deployment,
          request_id: null,
          finish_reason: "stop",
          usage: null,
          attributes: { "endpoint": self.endpoint },
          raw: null,
        },
      }
    }
  }
}
