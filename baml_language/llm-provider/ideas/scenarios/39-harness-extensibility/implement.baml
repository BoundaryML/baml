// =============================================================================
// 39 — Harness extensibility: tools, MCP, sub-agents, skills, hooks, A2A
//
// STD-LIB / LIBRARY-AUTHOR code. Builds on the canonical primitives in
// ../_conventions.md (Provider, HttpProvider, Tools, Realtime, ToolCall/
// ToolResult/ToolCalls, Tool, ExecutionContext, Channel). Everything below
// either reuses those verbatim or is flagged net-new in evaluation.md.
//
// The five extension surfaces from background/06-harnesses.md §4 map onto the
// model like this:
//   - custom tools + external MCP   -> a `tools` field of type Tool[] (one map)
//   - sub-agents                    -> named Providers with restricted tools
//   - skills / slash commands       -> a Skills capability (named, typed call)
//   - hooks                         -> ExecutionContext.emit + a Hook seam
//   - A2A peer agents               -> A2aProvider implements HttpProvider
// =============================================================================


// -----------------------------------------------------------------------------
// 0. Net-new host primitives. Genuine boundaries only — everything that touches
//    a subprocess, an MCP server, or the network lives behind $rust_io_function.
// -----------------------------------------------------------------------------

// A handler is host-side code (a closure / function pointer captured at tool
// authoring time). It is OPAQUE to BAML — we never call it from BAML, the host
// dispatcher invokes it. `unknown` in / `unknown` out is the loose-output
// reality from §4 "Tool output typing is unsolved".
class ToolHandler { _fn: $rust_type }                 // host closure, (unknown) -> unknown

// A live connection to an external MCP server (stdio / SSE / HTTP). Opaque.
class McpConnection { _handle: $rust_type }

// A live connection to a spawned subprocess harness (the `claude` binary, etc).
class HarnessSession { _handle: $rust_type }


// -----------------------------------------------------------------------------
// 1. Tools as a uniform map. In-process tools and MCP tools are the SAME type.
//    The whole point of §4: "my function" and "a remote MCP server over HTTP"
//    are configured uniformly behind one Tool[] list.
// -----------------------------------------------------------------------------

// The canonical `Tool` (from _conventions) is { name, description, parameters: type }.
// We extend it with an OPTIONAL dispatch source so one Tool[] can carry both
// in-process closures AND remote MCP tools. This is net-new (flagged).
//
//   source == InProcess { handler }  -> the handler runs in this process
//   source == Mcp { conn }           -> the call is proxied to an MCP server
//   source == null                   -> a bare name the harness resolves itself
//                                        (built-in tools like "Read"/"Bash")
class HarnessTool {
  name: string
  description: string
  parameters: type                                    // first-class `type`; input infers
  // output: type?  -- INTENTIONALLY ABSENT. See evaluation (d): output is loose.
  source: ToolSource?
}

class ToolSource {
  in_process: ToolHandler?
  mcp: McpConnection?                                  // the server this tool lives on
  mcp_wire_name: string?                               // "mcp__weather__get_temperature"
}

// Authoring helpers — what an app author calls to make a Tool[]. Static funcs.
class Tools$factory {
  // In-process four-tuple, exactly the Claude SDK shape (name, desc, schema, handler).
  function in_process<TArgs>(name: string, description: string, handler: ToolHandler) -> HarnessTool {
    HarnessTool {
      name: name,
      description: description,
      parameters: reflect.type_of<TArgs>(),           // input schema from the arg type
      source: ToolSource { in_process: handler, mcp: null, mcp_wire_name: null },
    }
  }

  // Connect a remote MCP server and pull ALL of its tools into one flat list,
  // each pre-mangled to its wire name. This is the connectMcpServer / mcpServers
  // map collapsed to a Tool[].
  function from_mcp(server_name: string, url: string, headers: map<string, string>) -> HarnessTool[] {
    let conn = mcp_connect(server_name, url, headers);          // $rust_io_function below
    let specs = mcp_list_tools(conn);                           // [(local_name, desc, schema)]
    let out: HarnessTool[] = [];
    for (let s in specs) {
      out.push(HarnessTool {
        name: "mcp__{server_name}__{s.local_name}",            // wire-name mangling, §4 leakage
        description: s.description,
        parameters: s.schema,
        source: ToolSource {
          in_process: null,
          mcp: conn,
          mcp_wire_name: "mcp__{server_name}__{s.local_name}",
        },
      });
    }
    out
  }
}

function mcp_connect(name: string, url: string, headers: map<string, string>) -> McpConnection throws baml.errors.Io {
  $rust_io_function                                   // opens stdio/SSE/HTTP transport
}
function mcp_list_tools(conn: McpConnection) -> McpToolSpec[] throws baml.errors.Io {
  $rust_io_function
}
function mcp_call(conn: McpConnection, wire_name: string, args: map<string, unknown>) -> unknown throws baml.errors.Io {
  $rust_io_function                                   // output is `unknown` — loose by design
}
class McpToolSpec { local_name: string, description: string, schema: type }


// -----------------------------------------------------------------------------
// 2. Hooks. Lifecycle events ride ExecutionContext.emit (net-new on ExecutionContext,
//    already flagged by the proposal as "an event-emit surface"). A Hook observes,
//    mutates, or blocks. We model the dispatch seam so a hook can VETO a tool call.
// -----------------------------------------------------------------------------

// The event vocabulary the harness emits. Net-new.
class HookEvent {
  kind: string                  // "pre_tool" | "post_tool" | "stop" | "session_start"
  tool_name: string?            // present on pre_tool / post_tool
  payload: unknown              // args (pre) or output (post) — loose, like the SDK
}

// A hook's verdict. `block` short-circuits; `replace` rewrites the payload.
class HookVerdict {
  block: bool
  replace: unknown?             // if set, substitute for the tool's args/output
  reason: string?
}

interface Hook {
  function on_event(self, e: HookEvent) -> HookVerdict throws baml.errors.Io
}

// ExecutionContext is the seam the Tools loop already uses (ctx.dispatch). The
// proposal says it needs `dispatch` + an event-`emit` surface. We make `dispatch`
// hook-aware: it emits pre_tool, may be vetoed, runs the tool, emits post_tool.
//
// This is the ONE place in/out tools and MCP tools and hooks all converge.
class HookedExecutionContext {
  tools: HarnessTool[]
  hooks: Hook[]

  implements ExecutionContext {
    // The canonical Tools loop calls ctx.dispatch(calls) -> ToolResult[].
    function dispatch(self, calls: ToolCall[]) -> ToolResult[] throws baml.errors.Io {
      let results: ToolResult[] = [];
      for (let c in calls) {
        results.push(self.dispatch_one(c));
      }
      results
    }

    // emit is the hooks surface; a plain observer event with no veto.
    function emit(self, e: HookEvent) -> null throws baml.errors.Io {
      for (let h in self.hooks) {
        h.on_event(e);            // observer; verdict ignored for non-tool events
      }
      null
    }
  }

  // --- internal: the hook-gated single-tool path. Not part of any interface. ---
  function dispatch_one(self, c: ToolCall) -> ToolResult {
    // 1. find the tool by name (handles wire-mangled MCP names transparently)
    let tool = self.find(c.name);

    // 2. PRE hook — may block or rewrite args
    let pre = self.run_hooks(HookEvent { kind: "pre_tool", tool_name: c.name, payload: c.args });
    if (pre.block) {
      // a blocked tool still needs a correlated result — the loop never sees the id.
      return ToolResult { id: c.id, output: { "error": pre.reason } };
    }
    let args = match (pre.replace) { let r: unknown => coerce_args(r), _ => c.args };

    // 3. run it — in-process closure OR proxy to MCP OR a built-in name
    let raw = self.invoke(tool, args) catch (e) {
      _ => { return ToolResult { id: c.id, output: { "error": "tool failed" } }; }
    };

    // 4. POST hook — may rewrite output (e.g. redact, lint-after-edit)
    let post = self.run_hooks(HookEvent { kind: "post_tool", tool_name: c.name, payload: raw });
    let output = match (post.replace) { let r: unknown => r, _ => raw };

    // id is OPAQUE and threaded straight back — correlation belongs to the provider.
    ToolResult { id: c.id, output: output }
  }

  function invoke(self, tool: HarnessTool, args: map<string, unknown>) -> unknown throws baml.errors.Io {
    match (tool.source) {
      let s: ToolSource => {
        match (s.in_process) {
          let h: ToolHandler => { return run_handler(h, args); },         // host closure
          _ => {
            match (s.mcp) {
              let conn: McpConnection => { return mcp_call(conn, s.mcp_wire_name!, args); },
              _ => { throw baml.errors.Io { message: "tool '{tool.name}' has no dispatch source" }; },
            }
          },
        }
      },
      // null source => a built-in tool the *provider* resolves (Read/Bash/etc).
      // We have no closure for it; the harness handles it. Surface as unsupported here.
      _ => { throw baml.errors.Io { message: "built-in tool '{tool.name}' is provider-resolved" }; },
    }
  }

  function find(self, name: string) -> HarnessTool {
    for (let t in self.tools) {
      if (t.name == name) { return t; }
    }
    baml.sys.panic("model called unknown tool '{name}'")     // shouldn't happen: we send the list
  }

  // Fold all hooks; first BLOCK wins, last REPLACE wins.
  function run_hooks(self, e: HookEvent) -> HookVerdict {
    let acc = HookVerdict { block: false, replace: null, reason: null };
    for (let h in self.hooks) {
      let v = h.on_event(e) catch (_) { _ => { continue; } };
      if (v.block) { return v; }
      acc = match (v.replace) { let r: unknown => v, _ => acc };
    }
    acc
  }
}

function run_handler(h: ToolHandler, args: map<string, unknown>) -> unknown throws baml.errors.Io {
  $rust_io_function                                   // invoke the captured host closure
}
function coerce_args(v: unknown) -> map<string, unknown> { $rust_io_function }


// -----------------------------------------------------------------------------
// 3. Skills / slash commands. A named, typed procedure the harness runs. This is
//    a capability: not every provider can run a skill (a bare chat model can't).
//    Net-new interface.
// -----------------------------------------------------------------------------

// Skills are a NEW capability beyond call/stream/realtime/tools, so they earn a
// matching capability-error interface (flagged net-new in evaluation.md). Same
// shape as the core ones: classifiers callers triage on without the concrete class.
interface baml.errors.SkillError {
  function is_network_error(self) -> bool
  function is_unknown_skill(self) -> bool   // the harness has no such named procedure
}

interface Skills requires Provider {
  // typed output, args as a bag — mirrors Flue's session.skill("triage", {...}, schema)
  function skill<T>(self, name: string, args: map<string, unknown>) -> T
      throws baml.ExtendUnknownError<baml.errors.SkillError>
  // enumerate available slash commands over the control plane (Claude's supportedCommands)
  function commands(self) -> string[]
      throws baml.ExtendUnknownError<baml.errors.SkillError>
}


// -----------------------------------------------------------------------------
// 4. The concrete harness provider. NOT an HttpProvider — transport is a
//    subprocess (proposal §8). Per ../provider-as-marker.md, `Provider` is a bare
//    MARKER (no `call`), so the harness no longer fakes a degenerate one-shot
//    `call`. It `requires Provider` THROUGH the capabilities it implements (Tools
//    + Realtime + Skills) and carries tools + sub-agents + hooks as plain config
//    FIELDS. Its real interaction surfaces are the tool loop, the steerable live
//    session, and named skills — none of them request/response.
// -----------------------------------------------------------------------------

class ClaudeCode {
  model: string?
  permission_mode: string?
  builtin_tools: string[]?                 // resolved by the binary: ["Read","Bash",...]

  // The whole extensibility payload, dissolved into provider fields (§4 "options
  // dissolve into fields"). One Tool[] for in-process + MCP. A sub-agent registry.
  tools: HarnessTool[]?
  agents: map<string, Provider>?           // sub-agents are named PROVIDERS (see §5)
  hooks: Hook[]?

  // No `implements Provider { call }` — a subprocess harness has no honest
  // request/response `call`. The class is a Provider (marker) by virtue of
  // implementing Tools/Realtime/Skills, each of which `requires Provider`.

  implements Tools {
    // Transcript is provider-owned + opaque: it wraps the live subprocess session.
    type Transcript = HarnessSession

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> HarnessSession
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // NOTE: the loop hands us canonical `Tool[]`. But our dispatch needs the
      // RICHER HarnessTool[] (with handlers/MCP sources) we carry as a field.
      // We ignore the passed `tools` schema-list and use self.tools for dispatch.
      // <-- this is a real awkwardness; see evaluation (c).
      harness_spawn_session(self.model, self.permission_mode, self.flat_tool_names())
        catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "spawn failed"); }
    }

    function step<T>(self, t: HarnessSession) -> T | ToolCalls
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // pump the subprocess; either it asks for tools or it produces the final value.
      let ev = harness_pump(t) catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "pump failed"); };
      match (ev.tool_calls) {
        let calls: ToolCall[] => { ToolCalls { calls: calls } },           // id = parent_tool_use_id
        _ => { baml.sap.parse<T>(ev.final_text!) },
      }
    }

    function submit(self, t: HarnessSession, results: ToolResult[]) -> HarnessSession
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      harness_submit(t, results)                       // ids threaded straight back, opaque
        catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "submit failed"); }
    }
    // run_tools<T> inherited from Tools default — drives begin/step/submit + ctx.dispatch.
  }

  implements Realtime {
    // the long-lived steerable session: the control/event channel is handed in.
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      $rust_io_function                                // duplex over the spawned subprocess
    }
  }

  implements Skills {
    function skill<T>(self, name: string, args: map<string, unknown>) -> T
        throws baml.ExtendUnknownError<baml.errors.SkillError> {
      let raw = harness_run_skill(self.model, name, args)
        catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.SkillError>(e, "skill '{name}' failed"); };
      baml.sap.parse<T>(raw)                           // typed output via SAP, like Flue's schema
    }
    function commands(self) -> string[]
        throws baml.ExtendUnknownError<baml.errors.SkillError> {
      harness_supported_commands(self.model)
        catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.SkillError>(e, "commands failed"); }
    }
  }

  // --- helpers -------------------------------------------------------------
  // The binary only takes a flat list of NAMES; our HarnessTool[] carries more,
  // so the harness sees names and our HookedExecutionContext owns the dispatch.
  function flat_tool_names(self) -> string[] {
    let names = match (self.builtin_tools) { let b: string[] => b, _ => [] };
    let extra = match (self.tools) {
      let ts: HarnessTool[] => {
        let acc: string[] = [];
        for (let t in ts) { acc.push(t.name); }
        acc
      },
      _ => [],
    };
    // sub-agents surface to the model as a single built-in "Agent" tool (Claude SDK).
    let with_agent = match (self.agents) { let _a: map<string, Provider> => names.concat(["Agent"]), _ => names };
    with_agent.concat(extra)
  }
}

function harness_spawn(model: string?, mode: string?, tools: string[]) -> HarnessSession throws baml.errors.Io { $rust_io_function }
function harness_spawn_session(model: string?, mode: string?, tools: string[]) -> HarnessSession throws baml.errors.Io { $rust_io_function }
function harness_run_to_end(s: HarnessSession, prompt: baml.llm.PromptAst) -> string throws baml.errors.Io { $rust_io_function }
function harness_pump(s: HarnessSession) -> HarnessEvent throws baml.errors.Io { $rust_io_function }
function harness_submit(s: HarnessSession, results: ToolResult[]) -> HarnessSession throws baml.errors.Io { $rust_io_function }
function harness_run_skill(model: string?, name: string, args: map<string, unknown>) -> string throws baml.errors.Io { $rust_io_function }
function harness_supported_commands(model: string?) -> string[] throws baml.errors.Io { $rust_io_function }
class HarnessEvent { tool_calls: ToolCall[]?, final_text: string? }


// -----------------------------------------------------------------------------
// 5. Sub-agents are named Providers with restricted tools. A sub-agent IS a
//    provider — so the registry is map<string, Provider>, and a sub-agent can be
//    another ClaudeCode with a smaller `tools`, OR an A2aProvider (peer), OR even
//    a bare chat model. The "Agent" tool dispatches into it.
//
//    A SubAgent combinator wraps a Provider to enforce the §4 composition rules:
//    fresh conversation (one prompt in), final message out, no nested sub-agents.
// -----------------------------------------------------------------------------

class SubAgent {
  inner: Provider                          // the owned agent (often a restricted ClaudeCode)
  description: string                      // the PARENT reads this to decide when to delegate
  max_turns: int?

  // A sub-agent exposes ONLY request/response to its parent: one prompt in, final
  // message out. So it forwards `call`/`call_with` to its inner provider's
  // HttpProvider capability — `call` is an HttpProvider method now (marker spine).
  implements HttpProvider {
    type Body = unknown
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // fresh conversation: the parent hands ONE prompt; we return ONLY the final.
      // (No nested sub-agents: we expose HttpProvider.call, never Tools, to the parent.)
      match (self.inner) {
        let h: HttpProvider => h.call<T>(prompt),
        _ => throw baml.errors.Unsupported { message: "sub-agent inner is not request/response" },
      }
    }
    // Forward the value+sidecar projection so the parent's usage/cost is not dropped
    // (the P1 fix): route call_with straight to the inner HttpProvider's call_with.
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.inner) {
        let h: HttpProvider => h.call_with<T, U>(prompt, project),
        _ => throw baml.errors.Unsupported { message: "sub-agent inner is not request/response" },
      }
    }
  }
  // Deliberately does NOT implement Tools/Skills: a sub-agent is opaque to its
  // parent — it cannot spawn its own sub-agents (§4 composition rule, enforced
  // structurally by simply not forwarding those capabilities).
}


// -----------------------------------------------------------------------------
// 6. A2A peer agents are Providers reached over the wire. An A2aProvider is an
//    HttpProvider whose `build_request` is JSON-RPC 2.0 and whose `parse` reads
//    the task result. Discovery (Agent Card) is a static factory. This is the
//    payoff: a peer agent you DON'T own is just another Provider — drop it into
//    the same `agents` map as an owned sub-agent.
// -----------------------------------------------------------------------------

class AgentCard {
  name: string
  endpoint: string
  skills: string[]
  _raw: $rust_type
}

// Typed view of the JSON-RPC 2.0 "message/send" result envelope. `parse` decodes
// the response body into this (via baml.json.deserialize) and reads parts[0].text.
class A2aRpcResponse { result: A2aResult }
class A2aResult      { message: A2aMessage }
class A2aMessage     { parts: A2aPart[] }
class A2aPart        { text: string }

class A2aProvider {
  endpoint: string                         // JSON-RPC 2.0 over HTTP(S)
  headers: map<string, string>?
  card: AgentCard?                         // cached discovery doc

  // Discovery: fetch /.well-known/agent-card.json. Static factory (no self).
  function discover(base_url: string, headers: map<string, string>) -> A2aProvider throws baml.errors.Io {
    let card = a2a_fetch_card("{base_url}/.well-known/agent-card.json", headers);
    A2aProvider { endpoint: card.endpoint, headers: headers, card: card }
  }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // A2A "message/send" as a JSON-RPC 2.0 envelope. The peer's tools/prompts/
      // state are NOT exposed — we send a task, we get a result. T is irrelevant
      // to the wire shape here (peer decides its own output), only to `parse`.
      baml.http.Request {
        method: "POST",
        url: self.endpoint,
        headers: self.with_headers({ "Content-Type": "application/json" }),
        body: a2a_jsonrpc_envelope("message/send", prompt),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "a2a send failed"); }
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // pull result.message.parts[0].text out of the JSON-RPC result, then SAP into T.
      // Decode the envelope into a typed class (override-honoring), then SAP the
      // peer's loosely-typed text into T at the edge.
      let env: A2aRpcResponse = baml.json.deserialize<A2aRpcResponse>(from.text())
        catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "a2a parse failed"); };
      match (env.result.message.parts.at(0)) {
        let p: A2aPart => baml.sap.parse<T>(p.text),
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>("a2a result had no message parts", "a2a parse failed"),
      }
    }
    // call<T> inherited from HttpProvider default.
  }

  function with_headers(self, base: map<string, string>) -> map<string, string> {
    match (self.headers) {
      let h: map<string, string> => {
        // Map has no `merge`; copy the configured headers over the base in place.
        for (let k in h.keys()) {
          match (h.get(k)) { let v: string => { base.set(k, v); }, _ => {} }
        }
        base
      },
      _ => base,
    }
  }
}

function a2a_fetch_card(url: string, headers: map<string, string>) -> AgentCard throws baml.errors.Io { $rust_io_function }
function a2a_jsonrpc_envelope(method: string, prompt: baml.llm.PromptAst) -> string { $rust_io_function }
