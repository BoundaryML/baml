// =============================================================================
// 41 — Embedding & deployment of a harness    (implement.baml — LIBRARY AUTHOR)
// =============================================================================
//
// Background: background/06-harnesses.md -> ## 6. Embedding & deployment
//
// What this file must absorb:
//   1. THREE drive channels behind ONE Provider:
//        - subprocess + JSONL over stdio       (Claude SDK)
//        - JSON-RPC subprocess                  (Pi --mode rpc)
//        - in-process async generator           (OpenAI Agents Runner, Claude TS)
//      ...all reachable through `Provider.call` / `Realtime.run`.
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
// `Provider.call` flattens a whole harness run into one typed answer, and
// `Realtime.run` (canonical) gives the steerable duplex. But the BACKGROUND's
// real divergence is the *drive channel itself*: subprocess+JSONL vs JSON-RPC vs
// in-process generator. We make that an explicit associated-type capability so a
// host can pick a transport without knowing the harness brand.

interface Drivable requires Provider {
  // The concrete drive handle is provider-owned and opaque, exactly like
  // Tools.Transcript. A subprocess harness sets Drive = Subprocess; an
  // in-process one sets Drive = Generator.
  type Drive

  function open(self, prompt: baml.llm.PromptAst) -> Drive throws Error
  // pump one host<->harness exchange; null == the run finished.
  function pump(self, d: Drive, input: HostMsg?) -> AgentMsg? throws Error
  function close_drive(self, d: Drive) -> null
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
  function attach(self, instance_id: string) -> Instance throws Error
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
      -> string throws baml.errors.LlmClient {
    match (self.inner) {
      let d: Durable => {
        let inst = d.attach(instance_id);                 // single-writer lease
        let answer: string = self.inner.call<string>(prompt) catch (e) {
          _ => { release_instance(inst); throw baml.errors.LlmClient { message: "run failed" }; }
        };
        release_instance(inst);
        return answer;
      },
      // not durable: still serve, but the host owns idempotency (the hard case).
      _ => { return self.inner.call<string>(prompt); },
    }
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

  // ---- the irreducible base: flatten the whole run to one typed answer -------
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // call is just drive-to-completion with no host steering.
      let d = self.open(prompt);
      let final_blob: string = "";
      while (true) {
        match (self.pump(d, null) catch (e) { _ => { self.close_drive(d); throw baml.errors.LlmClient { message: "claude io" }; } }) {
          let msg: AgentMsg => {
            if (msg.kind == "final") { final_blob = msg.final!; }
            if (msg.kind == "error") { self.close_drive(d); throw baml.errors.LlmClient { message: msg.text! }; }
          },
          null => { break; },               // EOF: run finished
        }
      }
      self.close_drive(d);
      baml.sap.parse<T>(final_blob)         // SAP the final JSON into T
    }
  }

  // ---- the drive channel: JSONL over a supervised child process --------------
  implements Drivable {
    type Drive = Subprocess

    function open(self, prompt: baml.llm.PromptAst) -> Subprocess throws baml.errors.LlmClient {
      // late-bind provider config from the runtime registry (registerProvider).
      let cfg = resolve_provider(registry(), "anthropic");
      let env: map<string, string> = base_env(cfg);
      let p = spawn("claude", self.cli_args(), env) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "spawn claude failed" }; }
      };
      // first JSONL line = the system/user prompt frame.
      write_line(p, frame_user(prompt)) catch (e) {
        _ => { kill(p); throw baml.errors.LlmClient { message: "claude stdin closed" }; }
      };
      return p;
    }

    function pump(self, d: Subprocess, input: HostMsg?) -> AgentMsg? throws baml.errors.LlmClient {
      // host->harness: write the steering message, if any.
      match (input) {
        let m: HostMsg => {
          write_line(d, frame_host(m)) catch (e) { _ => { throw baml.errors.LlmClient { message: "stdin" }; } };
        },
        null => { },
      }
      // harness->host: read one JSONL line; EOF -> the run is over.
      match (read_line(d) catch (e) { _ => { throw baml.errors.LlmClient { message: "stdout" }; } }) {
        let line: string => { return parse_agent_msg(line); },
        null => { return null; },
      }
    }

    function close_drive(self, d: Subprocess) -> null { kill(d); }
  }

  // ---- the steerable duplex (canonical Realtime): bridge io <-> JSONL --------
  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws baml.errors.LlmClient {
      let d = self.open(prompt);
      // host steering (interrupt/extra-input) arrives on io and is forwarded as JSONL.
      io.on(ev => {
        write_line(d, frame_event(ev)) catch (e) { _ => { } };   // best-effort steer
      });
      let transcript: string = "";
      while (true) {
        match (self.pump(d, null) catch (e) { _ => { self.close_drive(d); throw baml.errors.LlmClient { message: "claude io" }; } }) {
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
    }
  }

  // ---- durable identity: persisted JSONL session under a single-writer lease -
  implements Durable {
    function attach(self, instance_id: string) -> Instance throws baml.errors.LlmClient {
      let lease = lease_acquire(instance_id) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "instance " + instance_id + " is busy (single-writer)" }; }
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

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      let d = self.open(prompt);
      // Pi RPC returns the final value on a single "agent.run" reply.
      let reply = rpc_call(d, "agent.run", frame_user(prompt)) catch (e) {
        _ => { self.close_drive(d); throw baml.errors.LlmClient { message: "pi rpc failed" }; }
      };
      self.close_drive(d);
      baml.sap.parse<T>(reply)
    }
  }

  implements Drivable {
    type Drive = Subprocess

    function open(self, prompt: baml.llm.PromptAst) -> Subprocess throws baml.errors.LlmClient {
      let cfg = resolve_provider(registry(), "anthropic");
      let args = self.no_session ? ["--mode", "rpc", "--no-session"] : ["--mode", "rpc"];
      spawn("pi", args, base_env(cfg)) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "spawn pi failed" }; }
      }
    }

    function pump(self, d: Subprocess, input: HostMsg?) -> AgentMsg? throws baml.errors.LlmClient {
      // JSON-RPC is request/reply, not a stream: every pump is one rpc_call whose
      // id the host never sees. A null input means "advance the run" (poll next).
      let method = match (input) { let m: HostMsg => host_rpc_method(m), null => "agent.next" };
      let params = match (input) { let m: HostMsg => frame_host(m), null => "{}" };
      let reply = rpc_call(d, method, params) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "pi rpc" }; }
      };
      // a sentinel reply ("__done__") maps to null, mirroring JSONL EOF.
      if (reply == "__done__") { return null; }
      return parse_agent_msg(reply);
    }

    function close_drive(self, d: Subprocess) -> null {
      rpc_notify(d, "shutdown", "{}") catch (e) { _ => { } };
      kill(d);
    }
  }

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws baml.errors.LlmClient {
      let d = self.open(prompt);
      io.on(ev => { rpc_notify(d, "interrupt", frame_event(ev)) catch (e) { _ => { } }; });
      let transcript: string = "";
      let started = rpc_call(d, "agent.start", frame_user(prompt)) catch (e) {
        _ => { self.close_drive(d); throw baml.errors.LlmClient { message: "pi start" }; }
      };
      while (true) {
        match (self.pump(d, null) catch (e) { _ => { self.close_drive(d); throw baml.errors.LlmClient { message: "pi io" }; } }) {
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

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      let d = self.open(prompt);
      let final_blob: string = "";
      while (true) {
        match (self.pump(d, null) catch (e) { _ => { throw baml.errors.LlmClient { message: "runner" }; } }) {
          let msg: AgentMsg => { if (msg.kind == "final") { final_blob = msg.final!; } },
          null => { break; },
        }
      }
      baml.sap.parse<T>(final_blob)
    }
  }

  implements Drivable {
    type Drive = Generator

    function open(self, prompt: baml.llm.PromptAst) -> Generator throws baml.errors.LlmClient {
      // no Registry read: an in-process runner uses its own bound model/api key
      // (the env-dispatch divergence — it doesn't go through registerProvider).
      gen_start(self.instructions, frame_user(prompt)) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "runner start failed" }; }
      }
    }

    function pump(self, d: Generator, input: HostMsg?) -> AgentMsg? throws baml.errors.LlmClient {
      // in-process generators are pull-only: there is no host->harness write
      // mid-run for the synchronous Runner. A non-null input is unsupported here.
      match (input) {
        let m: HostMsg => { throw baml.errors.LlmClient { message: "AgentsRunner.run_sync is not steerable" }; },
        null => { },
      }
      match (gen_next(d) catch (e) { _ => { throw baml.errors.LlmClient { message: "gen" }; } }) {
        let ev: string => { return parse_agent_msg(ev); },
        null => { return null; },
      }
    }

    function close_drive(self, d: Generator) -> null { }   // GC'd; nothing to kill
  }

  // AgentsRunner.run_streamed IS realtime-ish (event stream), but read-only:
  // you can iterate events, you cannot inject steering. So Realtime.run here is
  // a one-way bridge — io.on() handlers are accepted but never fire.
  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws baml.errors.LlmClient {
      let d = self.open(prompt);
      let transcript: string = "";
      while (true) {
        match (self.pump(d, null) catch (e) { _ => { throw baml.errors.LlmClient { message: "runner io" }; } }) {
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

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      let attempt: int = 0;
      while (true) {
        let r: T = self.inner.call<T>(prompt) catch (e) {
          _ => {
            attempt = attempt + 1;
            if (attempt > self.max_restarts) { throw baml.errors.LlmClient { message: "harness crashed; restarts exhausted" }; }
            continue;                                  // respawn-on-crash
          }
        };
        return r;
      }
      baml.sys.panic("unreachable")
    }
  }

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws baml.errors.LlmClient {
      // a duplex session that crashed mid-stream cannot be transparently resumed
      // without server-authoritative replay; we route once and surface the crash.
      match (self.inner) {
        let r: Realtime => r.run(prompt, io),
        _ => throw baml.errors.LlmClient { message: "supervised inner is not realtime" },
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
