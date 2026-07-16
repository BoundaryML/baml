// =============================================================================
// 41 — Embedding & deployment of a harness    (implement.baml — LIBRARY AUTHOR)
// =============================================================================
//
// Background: background/06-harnesses.md -> ## 6. Embedding & deployment
//
// What this file must absorb:
//   1. THREE drive channels behind ONE Provider marker:
//        - subprocess + JSONL over stdio       (Claude SDK)
//        - JSON-RPC subprocess                  (Pi --mode rpc)
//        - in-process async generator           (OpenAI Agents Runner, Claude TS)
//      ...all reachable through `Drivable` (drive-to-value / pump) and `Realtime.run`.
//      A harness is NOT request/response, so it carries NO `HttpProvider.call`;
//      `Provider` is a bare marker (see ../../provider-as-marker.md) and the real
//      value path is `Drivable.drive_to_value`, owned by the transport capability.
//   2. The agent as a DEPLOYABLE UNIT with declarative TRIGGERS (webhook / cron).
//   3. The AUTHOR vs CONSUME split (runtime SDK vs client SDK).
//   4. Provider config as RUNTIME config (registerProvider) vs a declarative
//      `client` block.
//   5. DURABLE INSTANCE IDENTITY (the `<id>` segment / session id).
//
// We build only ON the canonical primitives in _conventions.md:
//   Provider, Realtime, Tools, Channel, Transcript, ToolCall/ToolResult, KV.
// Everything else here is flagged net-new in evaluation.md.

// -----------------------------------------------------------------------------
// NET-NEW CAPABILITY ERROR INTERFACES (flagged in evaluation.md)
// -----------------------------------------------------------------------------
// The base error model (_conventions.md "Error model") ships CallError /
// StreamError / RealtimeError / ToolError. This scenario introduces capabilities
// the base model does not have — the drive channel, durable instances, and the
// deployment control-plane — so each gets its OWN capability-error interface with
// classifier methods, exactly like the core four. Their channel is the canonical
// `baml.ExtendUnknownError<baml.errors.<Cap>Error>`.

interface baml.errors.DriveError {
  // is the failure transport (child stdio / generator) vs an agent-side decline?
  function is_transport_error(self) -> bool
  function is_unsteerable(self) -> bool        // the drive channel refused host input
}

interface baml.errors.DurableError {
  // could not take the single-writer lease because another writer holds it.
  function is_lease_contention(self) -> bool
}

interface baml.errors.DeployError {
  // the deployed run failed (any reason) — the substrate's coarse classifier.
  function is_run_failure(self) -> bool
}

// ---- concrete typed errors these capabilities raise on purpose --------------

// The agent itself emitted an "error" AgentMsg (kind == "error"). This is a
// DOMAIN failure of the run, not transport — it surfaces inside `pump` /
// drive-to-value (DriveError) AND mid-Realtime (RealtimeError), and it rides the
// value path of a combinator over a harness (SubprocessSupervisor.call) as a
// CallError. It implements all three so it stays a CONCRETE type across each.
class HarnessRunError {
  reason: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.RealtimeError { function is_network_error(self) -> bool { return false; } }
  implements baml.errors.DriveError {
    function is_transport_error(self) -> bool { return false; }
    function is_unsteerable(self)     -> bool { return false; }
  }
}

// A pull-only drive channel was handed host input it cannot deliver (the
// in-process synchronous Runner). A typed DriveError so the host can tell
// "unsteerable" apart from a real transport fault.
class UnsteerableDriveError {
  message: string
  implements baml.errors.DriveError {
    function is_transport_error(self) -> bool { return false; }
    function is_unsteerable(self)     -> bool { return true; }
  }
}

// Another writer holds the single-writer lease for this instance id. It arises on
// the Durable channel (attach) but also reaches the deployment substrate through
// DeployedAgent.handle, so it implements DeployError too — that dual membership is
// what lets handle()'s `with_message<DeployError>` pass it through as a CONCRETE
// type instead of collapsing it into UnknownError.
class InstanceBusyError {
  instance_id: string
  implements baml.errors.DurableError {
    function is_lease_contention(self) -> bool { return true; }
  }
  implements baml.errors.DeployError {
    function is_run_failure(self) -> bool { return true; }
  }
}

// The deployed run failed end-to-end (handle() surfaces this to the substrate).
class DeployedRunError {
  message: string
  implements baml.errors.DeployError {
    function is_run_failure(self) -> bool { return true; }
  }
}

// -----------------------------------------------------------------------------
// 0. Host boundaries we lean on (genuine $rust_io seams).
// -----------------------------------------------------------------------------
// A harness's transport is NOT baml.http — it is a child process or an
// in-process generator. So these are the real native bridges (OQ #7: an
// `implements` block with native bodies is the supported boundary).

class Subprocess { _handle: $rust_type }          // a supervised child process
function spawn(cmd: string, args: string[], env: map<string, string>)
    -> Subprocess throws baml.errors.Io
// JSONL framing over the child's stdio:
function write_line(p: Subprocess, line: string) -> null throws baml.errors.Io
function read_line(p: Subprocess) -> string? throws baml.errors.Io   // null at EOF
function kill(p: Subprocess) -> null

// JSON-RPC framing over the same stdio (Pi). One request id correlates one reply.
function rpc_call(p: Subprocess, method: string, params: string)
    -> string throws baml.errors.Io                                   // params/return are JSON text
function rpc_notify(p: Subprocess, method: string, params: string)
    -> null throws baml.errors.Io

// In-process generator (Agents Runner / Claude TS query()): no child process.
class Generator { _handle: $rust_type }
function gen_start(entry: string, input: string) -> Generator throws baml.errors.Io
function gen_next(g: Generator) -> string? throws baml.errors.Io      // null when the run ends

// -----------------------------------------------------------------------------
// 1. THE DRIVE-CHANNEL CAPABILITY  (net-new)
// -----------------------------------------------------------------------------
// `Drivable.drive_to_value` flattens a whole harness run into one typed answer
// (a harness has no `Provider.call` — `Provider` is a bare marker), and
// `Realtime.run` (canonical) gives the steerable duplex. But the BACKGROUND's
// real divergence is the *drive channel itself*: subprocess+JSONL vs JSON-RPC vs
// in-process generator. We make that an explicit associated-type capability so a
// host can pick a transport without knowing the harness brand.

interface Drivable requires Provider {
  // The concrete drive handle is provider-owned and opaque, exactly like
  // Tools.Transcript. A subprocess harness sets Drive = Subprocess; an
  // in-process one sets Drive = Generator.
  type Drive

  function open(self, prompt: baml.llm.PromptAst) -> Drive
      throws baml.ExtendUnknownError<baml.errors.DriveError>
  // pump one host<->harness exchange; null == the run finished.
  function pump(self, d: Drive, input: HostMsg?) -> AgentMsg?
      throws baml.ExtendUnknownError<baml.errors.DriveError>
  function close_drive(self, d: Drive) -> null

  // drive-to-completion: the harness's REAL value-producing interaction. This is
  // NOT HttpProvider.call (a harness is not request/response); it is `Drivable`'s
  // own DEFAULT method — open, pump with no host steering until EOF/final, SAP the
  // last blob into T. Because it lives ON the capability that owns the transport,
  // there is no degenerate `call` faked onto Provider (provider-as-marker, OQ1).
  function drive_to_value<T>(self, prompt: baml.llm.PromptAst) -> T
      throws baml.ExtendUnknownError<baml.errors.DriveError> {
    let d = self.open(prompt);
    let final_blob: string = "";
    while (true) {
      match (self.pump(d, null)) {
        let msg: AgentMsg => {
          if (msg.kind == "final") { final_blob = msg.final!; }
          if (msg.kind == "error") { self.close_drive(d); throw HarnessRunError { reason: msg.text! }; }
        },
        null => { break; },                  // EOF: run finished
      }
    }
    self.close_drive(d);
    baml.sap.parse<T>(final_blob)             // SAP the final JSON into T
  } catch (e) {
    _ => throw baml.UnknownError.with_message<baml.errors.DriveError>(e, "drive-to-value failed");
  }
}

// The wire vocabulary the HOST speaks, independent of brand. The provider body
// translates these to JSONL / JSON-RPC / generator-yields. (net-new)
class HostMsg  { kind: string, text: string?, tool_results: ToolResult[]? }
class AgentMsg { kind: string, text: string?, tool_calls: ToolCalls?, final: string? }
//   kind in: "user" | "tool_result" | "interrupt"   (HostMsg)
//   kind in: "delta" | "tool_calls" | "final" | "error"  (AgentMsg)

// -----------------------------------------------------------------------------
// 2. RUNTIME PROVIDER REGISTRY  (registerProvider, net-new)
// -----------------------------------------------------------------------------
// The background: provider config in a harness world is RUNTIME config, not
// source. Flue's registerProvider("anthropic", { baseUrl, apiKey }) applies to
// every harness/session. That is the *anti*-declarative-client surface: you do
// NOT write a `client` block; you mutate a process-global table at boot.
//
// We model it as a host KV-like registry keyed by provider name. A harness body
// READS it at open()-time (late binding), which is exactly why registerProvider
// can override a harness that was constructed earlier.

class ProviderSettings { base_url: string?, api_key: string?, headers: map<string, string>? }

class Registry { _handle: $rust_type }
function registry() -> Registry                                       // process-global singleton
function register_provider(r: Registry, name: string, s: ProviderSettings) -> null
function resolve_provider(r: Registry, name: string) -> ProviderSettings?

// -----------------------------------------------------------------------------
// 3. DURABLE INSTANCE IDENTITY  (net-new)
// -----------------------------------------------------------------------------
// "Same <id> continues the same agent instance." The deployment substrate must
// guarantee single-writer semantics under concurrent triggers (the hard part).
// We surface that as an explicit capability the host can interrogate: a harness
// is `Durable` iff it can rehydrate by instance id under a lease.

interface Durable requires Provider {
  // bind/lease an instance; throws if another writer holds the lease (single-writer).
  function attach(self, instance_id: string) -> Instance
      throws baml.ExtendUnknownError<baml.errors.DurableError>
}

class Instance { id: string, _lease: $rust_type }   // _lease = the substrate lock (DO / DB row)
function lease_acquire(id: string) -> $rust_type throws baml.errors.Io   // single-writer guard
function lease_release(h: $rust_type) -> null
function session_load(id: string) -> string?         // persisted transcript blob
function session_save(id: string, blob: string) -> null throws baml.errors.Io

// -----------------------------------------------------------------------------
// 4. TRIGGERS — the agent as a deployable unit  (net-new)
// -----------------------------------------------------------------------------
// Only Flue lifts webhook/cron framing INTO the agent declaration. Everywhere
// else the host re-invents serving/scheduling/idempotency. We model a trigger
// set as a value an agent declares; the deployment substrate reads it.

class Trigger {
  kind: string            // "webhook" | "cron"
  schedule: string?       // cron expression when kind == "cron"
}

// A deployable agent = a Provider + its triggers + a durable-id resolver.
// This is the unit the substrate registers and routes /agents/<name>/<id> to.
class DeployedAgent {
  name: string                 // the file id (Flue: filename is the id)
  inner: Provider              // the harness (any Drivable+Durable provider)
  triggers: Trigger[]

  // Resolve the <id> path segment to a durable, single-writer instance, then
  // drive one prompt to completion. This is what a webhook handler calls.
  function handle(self, instance_id: string, prompt: baml.llm.PromptAst)
      -> string throws baml.ExtendUnknownError<baml.errors.DeployError> {
    match (self.inner) {
      let d: Durable => {
        let inst = d.attach(instance_id);                 // single-writer lease (DurableError)
        let answer: string = self.drive_inner<string>(prompt) catch (e) {
          _ => { release_instance(inst); throw DeployedRunError { message: "run failed" }; }
        };
        release_instance(inst);
        return answer;
      },
      // not durable: still serve, but the host owns idempotency (the hard case).
      _ => { return self.drive_inner<string>(prompt); },
    }
  } catch (e) {
    // a contended lease (DurableError) or a drive-path failure bubbling up the
    // non-durable arm is normalized into the deployment channel.
    _ => throw baml.UnknownError.with_message<baml.errors.DeployError>(e, "deployed handle failed");
  }

  // Drive the inner provider to ONE value. There is no privileged `Provider.call`
  // anymore, so this is a capability `match`: a harness produces a value through
  // `Drivable.drive_to_value`; a request/response inner (a supervisor combinator,
  // a remote FlueSdkClient) through `HttpProvider.call`. No honest value path -> error.
  function drive_inner<T>(self, prompt: baml.llm.PromptAst) -> T
      throws baml.ExtendUnknownError<baml.errors.DeployError> {
    match (self.inner) {
      let d: Drivable     => d.drive_to_value<T>(prompt),
      let h: HttpProvider => h.call<T>(prompt),
      _ => throw baml.errors.Unsupported { message: "deployed inner cannot produce a value" },
    }
  } catch (e) {
    _ => throw baml.UnknownError.with_message<baml.errors.DeployError>(e, "deployed inner drive failed");
  }
}

function release_instance(inst: Instance) -> null { lease_release(inst._lease); }

// =============================================================================
// 5. CONCRETE HARNESS #1 — ClaudeCode: subprocess + JSONL over stdio
// =============================================================================
// Drive = Subprocess. Each exchange is one newline-delimited JSON object. The
// agent loop runs in the child; we wrap it. Provider config is read from the
// runtime Registry at open() time (registerProvider can override us).

class ClaudeCode {
  model: string?
  sandbox: string              // "local" | "container" | "virtual"
  allowed_tools: string[]?
  permission_mode: string?

  // NO `implements Provider { call }`. A harness is NOT request/response — its real
  // value-producing interaction is drive-to-completion, which lives on `Drivable`
  // (drive_to_value below). ClaudeCode is still a Provider (the marker) because each
  // capability it implements `requires Provider`; nothing forces a degenerate `call`.

  // ---- the drive channel: JSONL over a supervised child process --------------
  implements Drivable {
    type Drive = Subprocess

    function open(self, prompt: baml.llm.PromptAst) -> Subprocess
        throws baml.ExtendUnknownError<baml.errors.DriveError> {
      // late-bind provider config from the runtime registry (registerProvider).
      let cfg = resolve_provider(registry(), "anthropic");
      let env: map<string, string> = base_env(cfg);
      let p = spawn("claude", self.cli_args(), env);
      // first JSONL line = the system/user prompt frame.
      write_line(p, frame_user(prompt)) catch (e) {
        _ => { kill(p); throw baml.UnknownError.with_message<baml.errors.DriveError>(e, "claude stdin closed"); }
      };
      return p;
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.DriveError>(e, "spawn claude failed");
    }

    function pump(self, d: Subprocess, input: HostMsg?) -> AgentMsg?
        throws baml.ExtendUnknownError<baml.errors.DriveError> {
      // host->harness: write the steering message, if any.
      match (input) {
        let m: HostMsg => { write_line(d, frame_host(m)); },
        null => { },
      }
      // harness->host: read one JSONL line; EOF -> the run is over.
      match (read_line(d)) {
        let line: string => { return parse_agent_msg(line); },
        null => { return null; },
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.DriveError>(e, "claude jsonl io failed");
    }

    function close_drive(self, d: Subprocess) -> null { kill(d); }
  }

  // ---- the steerable duplex (canonical Realtime): bridge io <-> JSONL --------
  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      let d = self.open(prompt);
      // host steering (interrupt/extra-input) arrives on io and is forwarded as JSONL.
      io.on(ev => {
        write_line(d, frame_event(ev)) catch (e) { _ => { } };   // best-effort steer
      });
      let transcript: string = "";
      while (true) {
        match (self.pump(d, null) catch (e) { _ => { self.close_drive(d); throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "claude realtime io failed"); } }) {
          let msg: AgentMsg => {
            io.send(out_event(msg));                  // stream deltas/tool-calls to the host
            transcript = transcript + msg_to_line(msg);
            if (msg.kind == "final") { break; }
          },
          null => { break; },
        }
      }
      self.close_drive(d);
      io.close();
      make_transcript(transcript)
    } catch (e) {
      // open() / io.send DriveError or foreign io becomes the realtime channel.
      _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "claude realtime failed");
    }
  }

  // ---- durable identity: persisted JSONL session under a single-writer lease -
  implements Durable {
    function attach(self, instance_id: string) -> Instance
        throws baml.ExtendUnknownError<baml.errors.DurableError> {
      let lease = lease_acquire(instance_id) catch (e) {
        // a held lease is meaningful classification: a typed DurableError the host branches on.
        _ => { throw InstanceBusyError { instance_id: instance_id }; }
      };
      return Instance { id: instance_id, _lease: lease };
    }
  }

  function cli_args(self) -> string[] { $rust_io_function }   // sandbox/permission/tools -> argv
}

// =============================================================================
// 6. CONCRETE HARNESS #2 — Pi: JSON-RPC subprocess
// =============================================================================
// SAME capabilities, DIFFERENT wire: `pi --mode rpc`. Each exchange is a
// JSON-RPC request id <-> reply. This is the divergence that proves portability:
// the host's HostMsg/AgentMsg vocabulary is identical; only the framing inside
// pump() differs (id-correlated request/reply vs newline-delimited stream).

class Pi {
  model: string?
  no_session: bool

  // NO `implements Provider { call }`. Pi's drive-to-completion is `Drivable`'s
  // default `drive_to_value` draining pump (the "agent.run" optimization is just an
  // inlined pump loop; the generic default covers it). Pi is a Provider via the
  // marker that every capability below `requires`.

  implements Drivable {
    type Drive = Subprocess

    function open(self, prompt: baml.llm.PromptAst) -> Subprocess
        throws baml.ExtendUnknownError<baml.errors.DriveError> {
      let cfg = resolve_provider(registry(), "anthropic");
      let args = self.no_session ? ["--mode", "rpc", "--no-session"] : ["--mode", "rpc"];
      spawn("pi", args, base_env(cfg))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.DriveError>(e, "spawn pi failed");
    }

    function pump(self, d: Subprocess, input: HostMsg?) -> AgentMsg?
        throws baml.ExtendUnknownError<baml.errors.DriveError> {
      // JSON-RPC is request/reply, not a stream: every pump is one rpc_call whose
      // id the host never sees. A null input means "advance the run" (poll next).
      let method = match (input) { let m: HostMsg => host_rpc_method(m), null => "agent.next" };
      let params = match (input) { let m: HostMsg => frame_host(m), null => "{}" };
      let reply = rpc_call(d, method, params);
      // a sentinel reply ("__done__") maps to null, mirroring JSONL EOF.
      if (reply == "__done__") { return null; }
      return parse_agent_msg(reply);
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.DriveError>(e, "pi rpc io failed");
    }

    function close_drive(self, d: Subprocess) -> null {
      rpc_notify(d, "shutdown", "{}") catch (e) { _ => { } };
      kill(d);
    }
  }

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      let d = self.open(prompt);
      io.on(ev => { rpc_notify(d, "interrupt", frame_event(ev)) catch (e) { _ => { } }; });
      let transcript: string = "";
      let started = rpc_call(d, "agent.start", frame_user(prompt)) catch (e) {
        _ => { self.close_drive(d); throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "pi start"); }
      };
      while (true) {
        match (self.pump(d, null) catch (e) { _ => { self.close_drive(d); throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "pi realtime io failed"); } }) {
          let msg: AgentMsg => {
            io.send(out_event(msg));
            transcript = transcript + msg_to_line(msg);
            if (msg.kind == "final") { break; }
          },
          null => { break; },
        }
      }
      self.close_drive(d);
      io.close();
      make_transcript(transcript)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "pi realtime failed");
    }
  }

  // NOTE: Pi with --no-session has NO durable identity. It deliberately does NOT
  // `implements Durable`. The host (DeployedAgent.handle) `match`es and degrades:
  // every trigger is a fresh process, idempotency is the host's problem.
}

// =============================================================================
// 7. CONCRETE HARNESS #3 — AgentsRunner: in-process async generator
// =============================================================================
// No child process at all. Drive = Generator. This is the OpenAI Agents Runner /
// Claude TS query() shape: you iterate events directly. Same capability surface;
// the transport is a generator, so `pump` is `gen_next`.

class AgentsRunner {
  model: string
  instructions: string

  // NO `implements Provider { call }`. The in-process generator's drive-to-completion
  // is `Drivable.drive_to_value`. AgentsRunner is a Provider via the marker every
  // capability below `requires`.

  implements Drivable {
    type Drive = Generator

    function open(self, prompt: baml.llm.PromptAst) -> Generator
        throws baml.ExtendUnknownError<baml.errors.DriveError> {
      // no Registry read: an in-process runner uses its own bound model/api key
      // (the env-dispatch divergence — it doesn't go through registerProvider).
      gen_start(self.instructions, frame_user(prompt))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.DriveError>(e, "runner start failed");
    }

    function pump(self, d: Generator, input: HostMsg?) -> AgentMsg?
        throws baml.ExtendUnknownError<baml.errors.DriveError> {
      // in-process generators are pull-only: there is no host->harness write
      // mid-run for the synchronous Runner. A non-null input is unsupported here —
      // a typed DriveError (is_unsteerable) so the host can tell it from a fault.
      match (input) {
        let m: HostMsg => { throw UnsteerableDriveError { message: "AgentsRunner.run_sync is not steerable" }; },
        null => { },
      }
      match (gen_next(d)) {
        let ev: string => { return parse_agent_msg(ev); },
        null => { return null; },
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.DriveError>(e, "runner generator io failed");
    }

    function close_drive(self, d: Generator) -> null { }   // GC'd; nothing to kill
  }

  // AgentsRunner.run_streamed IS realtime-ish (event stream), but read-only:
  // you can iterate events, you cannot inject steering. So Realtime.run here is
  // a one-way bridge — io.on() handlers are accepted but never fire.
  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      let d = self.open(prompt);
      let transcript: string = "";
      while (true) {
        match (self.pump(d, null) catch (e) { _ => { throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "runner realtime io failed"); } }) {
          let msg: AgentMsg => {
            io.send(out_event(msg));                 // one-way: stream out only
            transcript = transcript + msg_to_line(msg);
            if (msg.kind == "final") { break; }
          },
          null => { break; },
        }
      }
      io.close();
      make_transcript(transcript)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "runner realtime failed");
    }
  }

  // No Durable: the Agents default instance is an in-memory object, gone on
  // process exit. It does NOT implements Durable.
}

// =============================================================================
// 8. A COMBINATOR THAT MATTERS HERE — SubprocessSupervisor
// =============================================================================
// The background's "what's hard": subprocess lifecycle (crashes, zombies,
// version skew). That is orchestration, so per the rules it WRAPS a provider
// rather than living in its fields. It claims Provider/Drivable/Realtime and
// adds restart-on-crash. It is a plain non-generic class, like Retry/Fallback.

class SubprocessSupervisor {
  inner: Provider
  max_restarts: int

  // A combinator's value path is `HttpProvider` (provider-as-marker rule 1): it KEEPS
  // `call` and ADDS `call_with`, which FORWARDS the projection to the member it routes
  // to so usage/sidecar is not dropped. The inner here is a harness, which produces a
  // value through `Drivable.drive_to_value` (no sidecar) — so `call` drives that under
  // respawn-on-crash, while `call_with` can only honor a request/response inner that
  // actually carries a ResponseMeta.
  implements HttpProvider {
    type Body = unknown

    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let attempt: int = 0;
      while (true) {
        // the inner failure (HarnessRunError, normalized DriveError/CallError) is
        // swallowed and retried; only exhaustion re-surfaces on the call channel.
        let r: T = self.drive_once<T>(prompt) catch (e) {
          _ => {
            attempt = attempt + 1;
            if (attempt > self.max_restarts) { throw baml.UnknownError.with_message<baml.errors.CallError>(e, "harness crashed; restarts exhausted"); }
            continue;                                  // respawn-on-crash
          }
        };
        return r;
      }
      baml.sys.panic("unreachable")
    }

    // forward the value+sidecar projection to a request/response member. A harness
    // inner has no ResponseMeta to project, so call_with is only honest over an
    // HttpProvider member; the `_` arm errors rather than fabricating a sidecar.
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.inner) {
        let h: HttpProvider => { return h.call_with<T, U>(prompt, project); },
        _ => throw baml.errors.Unsupported { message: "supervised harness inner has no request/response sidecar" },
      }
    }
  }

  // one supervised attempt: produce a value from the inner via whichever value path
  // it actually has (a harness through Drivable, a request/response inner through call).
  function drive_once<T>(self, prompt: baml.llm.PromptAst) -> T
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    match (self.inner) {
      let dr: Drivable     => { return dr.drive_to_value<T>(prompt); },
      let h: HttpProvider  => { return h.call<T>(prompt); },
      _ => throw baml.errors.Unsupported { message: "supervised inner cannot produce a value" },
    }
  } catch (e) {
    _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "supervised inner drive failed");
  }

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      // a duplex session that crashed mid-stream cannot be transparently resumed
      // without server-authoritative replay; we route once and surface the crash.
      match (self.inner) {
        let r: Realtime => r.run(prompt, io),
        // capability negotiation: inner can't do realtime -> Unsupported (legal on any channel).
        _ => throw baml.errors.Unsupported { message: "supervised inner is not realtime" },
      }
    }
  }
}

// Provider gains a default combinator so the supervisor chains fluently, exactly
// like with_retry/cached. (Extends the canonical Provider interface's combinator set.)
//   self.supervised(3)  ==  SubprocessSupervisor { inner: self, max_restarts: 3 }

// -----------------------------------------------------------------------------
// 9. small pure helpers + host-codec boundaries
// -----------------------------------------------------------------------------
function base_env(cfg: ProviderSettings?) -> map<string, string> { $rust_io_function }
function frame_user(prompt: baml.llm.PromptAst) -> string { $rust_io_function }   // -> JSON line
function frame_host(m: HostMsg) -> string { $rust_io_function }
function frame_event(ev: InEvent) -> string { $rust_io_function }
function parse_agent_msg(line: string) -> AgentMsg { $rust_io_function }
function out_event(m: AgentMsg) -> OutEvent { $rust_io_function }
function msg_to_line(m: AgentMsg) -> string { $rust_io_function }
function make_transcript(blob: string) -> Transcript { $rust_io_function }
function host_rpc_method(m: HostMsg) -> string {
  match (m.kind) { "interrupt" => "interrupt", _ => "agent.submit" }
}
