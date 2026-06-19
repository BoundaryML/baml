// ============================================================================
// 12 · Tool taxonomy — hosted / client-executed / computer-use / MCP
//
// STD-LIB / library-author code. Builds on the canonical primitives in
// ../_conventions.md. The one teaching point: "a Tool" is FIVE different things
// that differ by WHO executes the call and WHERE. The background's "What's hard"
// says it plainly — "Anything unifying them needs to distinguish *execution
// location*, not just *name + schema*." So `Tool` (name + description +
// parameters) is insufficient. We introduce execution-location as a first-class
// `ToolKind` variant and split the dispatch seam in two: one for tools YOUR code
// drives, one for tools you can only OBSERVE.
//
// We reuse the canonical `Tools` capability (begin/step/submit/run_tools, the
// ToolCall/ToolResult seam) UNCHANGED for client-executed function tools. The
// other four kinds need net-new surface, flagged in evaluation.md.
// ============================================================================


// ----------------------------------------------------------------------------
// NET-NEW · Execution location as a Tool variant, not just name + schema.
//
// The canonical `class Tool { name, description, parameters }` only models the
// client-executed function case. Provider-hosted tools, computer-use, and MCP
// have no `parameters: type` the caller writes — the provider (or a remote
// server) owns the schema. So we lift `Tool` into a tagged union over the FIVE
// execution locations from the background's taxonomy table.
// ----------------------------------------------------------------------------

// Reused verbatim from _conventions.md — the client-executed function tool.
// (Repeated here so this file reads standalone; in-tree it is the canonical one.)
class FunctionTool { name: string, description: string, parameters: type }

// Provider runs it; you only DECLARE it and OBSERVE status. No `parameters` —
// the provider owns the tool's schema and execution. `config` is the raw
// per-provider knobs (`max_uses`, `vector_store_ids`, `container`, …) that the
// provider's build_request folds into the wire shape; opaque to the loop.
class HostedTool {
  // wire id the provider expects, e.g. "web_search" / "code_execution" / "file_search"
  kind: string
  config: map<string, unknown>
}

// Hybrid. The model proposes UI actions; YOUR code performs them on a live
// machine and returns a screenshot. Bound config is the environment hints the
// provider needs to emit coordinates the loop can act on.
class ComputerUseTool {
  display_width: int
  display_height: int
  environment: string            // "browser" | "mac" | "windows" | "ubuntu"
}

// A remote MCP server. Tools are DISCOVERED at connect time, not declared, and
// dispatched server-side by the provider (OpenAI `mcp`) or a harness. `auth` is
// how the provider obtains a token to talk to the server (see McpAuth below).
class McpServerTool {
  server_label: string
  server_url: string                    // Streamable HTTP endpoint
  auth: McpAuth
  require_approval: McpApproval          // never | always | per-tool
}

// Human-in-the-loop is not a fifth *location* — it is a gate layered on any of
// the above, so we model it as a wrapper, not a sibling. See ApprovalGate.

// The union the loop matches on. This is the lift the background demands:
// the type encodes execution location, so a provider's build_request and the
// dispatch seam can branch on WHO executes.
type ToolKind =
  | FunctionTool       // your code executes      (ToolResult flows back)
  | HostedTool         // provider executes        (you observe status only)
  | ComputerUseTool    // hybrid: model proposes, your code acts
  | McpServerTool      // remote server executes   (provider dispatches)


// ----------------------------------------------------------------------------
// NET-NEW · Remote-MCP transport + OAuth 2.1 audience-binding.
//
// The background is explicit: a static `Authorization: Bearer` is "the single
// biggest blocker to exposing a remote MCP server to clients you don't own."
// Production remote MCP is OAuth 2.1 with the MCP server as a Resource Server,
// `.well-known` discovery, optional Dynamic Client Registration, and — the part
// MCP makes mandatory — an RFC 8707 `resource` parameter that AUDIENCE-BINDS the
// token to one server. We model both, so the type tells you which you've got.
// ----------------------------------------------------------------------------

class StaticBearer { token: string }     // first-party only; NOT audience-bound

class OAuth21 {
  // The canonical URI of THIS MCP server. Goes in `resource` (RFC 8707) on BOTH
  // the authorization and token requests — this is what audience-binds the token
  // so a token minted for server A can't be replayed against server B.
  resource: string
  // PKCE is mandatory in OAuth 2.1; the verifier is generated per-flow, not stored.
  client_id: string?                     // null => Dynamic Client Registration (RFC 7591)
  client_secret: string?
}

type McpAuth = StaticBearer | OAuth21

class ApprovalNever  {}
class ApprovalAlways {}
class ApprovalPerTool { tool_names: string[] }
type McpApproval = ApprovalNever | ApprovalAlways | ApprovalPerTool


// ----------------------------------------------------------------------------
// NET-NEW · The observe-but-can't-drive seam.
//
// The canonical `Tools` loop assumes every ToolCall produces a ToolResult that
// YOUR ctx.dispatch returns. That is true ONLY for FunctionTool and the
// client-side leg of ComputerUseTool. For HostedTool and McpServerTool you get
// *status events* and annotated output — there is no callback, so there is
// nothing for `submit` to carry. The background: "You observe but don't drive
// hosted tools."
//
// So we split `step`'s sum type. The canonical `step -> T | ToolCalls` becomes
// `step -> T | ToolCalls | ToolActivity`, where ToolActivity is the read-only
// channel: it is emitted, never answered. The loop pattern-matches the third
// arm and just records/forwards it, then steps again.
// ----------------------------------------------------------------------------

// A status the provider produced about a tool IT (or a remote server) is running.
// You can log/trace/render it; you cannot intercept, cache, or mock the call.
class ToolActivity {
  // correlation id MINTED BY THE PROVIDER (e.g. a web_search_call id). Opaque —
  // and unlike a ToolCall.id, the loop never sends anything back keyed to it.
  id: string
  source: string            // "web_search" | "code_execution" | "mcp:github" | "computer_use"
  phase: string             // "in_progress" | "completed" | "failed"
  detail: map<string, unknown>     // annotations, URLs, the proposed computer action, ...
}

// The hybrid computer-use turn: a proposed action the loop MUST perform and
// answer with a screenshot. It is a ToolCalls-shaped thing (drives your code)
// but the result is image bytes, not arbitrary output — so it gets its own arm
// rather than overloading ToolResult.output: unknown.
class ComputerCall {
  call_id: string                 // provider-minted; you echo it on the screenshot
  action: map<string, unknown>    // {"type":"click","x":412,"y":233}, {"type":"screenshot"}, ...
}
class ComputerCalls { calls: ComputerCall[] }

class Screenshot { call_id: string, image_b64: string }      // the hybrid result


// ----------------------------------------------------------------------------
// NET-NEW · A wider Tools capability.
//
// `Tools` (canonical) handles FunctionTool. `MixedTools requires Tools` adds the
// observe channel and the computer-use leg WITHOUT changing the base seam, so a
// provider that only does function tools is unaffected. A provider that hosts
// tools / bridges MCP / does computer-use implements this richer interface.
// `step`'s return type widens; `submit_screenshot` is the computer-use answer.
// ----------------------------------------------------------------------------

interface MixedTools requires Tools {
  // Same Transcript associated type as Tools — provider-owned, opaque.
  function begin_mixed<T>(self, prompt: baml.llm.PromptAst, tools: ToolKind[]) -> Transcript throws Error

  // The widened next(): a value, function calls you drive, a computer action you
  // drive, or a read-only activity you can only observe. Mirror of Iterator.next
  // with two extra "Item" shapes.
  function step_mixed<T>(self, t: Transcript)
      -> T | ToolCalls | ComputerCalls | ToolActivity throws Error

  // Answer the computer-use leg. Distinct from `submit(results: ToolResult[])`
  // because the payload is screenshots correlated by provider call_id, and the
  // wire encoding (`computer_call_output` / `input_image`) is provider-specific.
  function submit_screenshots(self, t: Transcript, shots: Screenshot[]) -> Transcript throws Error

  // The full mixed loop. Reuses the canonical run_tools dispatch policy but adds
  // arms for the new locations. ctx gains `perform` (computer-use) and `observe`
  // (hosted/MCP status sink) — see ExecutionContext extension below.
  function run_mixed<T>(self, prompt: baml.llm.PromptAst, tools: ToolKind[], ctx: ExecutionContext) -> T throws Error {
    let t = self.begin_mixed<T>(prompt, tools);
    while (true) {
      match (self.step_mixed<T>(t)) {
        let value: T => { return value; },
        let calls: ToolCalls => {                       // FunctionTool — your code
          t = self.submit(t, ctx.dispatch(calls.calls));
        },
        let cu: ComputerCalls => {                      // ComputerUseTool — hybrid
          let shots: Screenshot[] = [];
          for (let c in cu.calls) {
            shots = shots + [ctx.perform(c)];           // YOUR code drives the machine
          }
          t = self.submit_screenshots(t, shots);
        },
        let act: ToolActivity => {                      // HostedTool / McpServerTool
          ctx.observe(act);                             // observe-only: log/trace/render
          // NB: no submit. We loop again; the PROVIDER advances the hosted call.
        },
      }
    }
    baml.sys.panic("unreachable")
  }
}


// ----------------------------------------------------------------------------
// NET-NEW · ExecutionContext gains `perform` + `observe`.
//
// The proposal's §7 net-new flag already says ExecutionContext "needs a dispatch
// and, for realtime/harnesses, an event-emit surface." We use exactly that
// emit surface for `observe`, and add `perform` for the computer-use leg. These
// are the host-side callbacks the loop hands control to.
// ----------------------------------------------------------------------------

interface ExecutionContext {
  function dispatch(self, calls: ToolCall[]) -> ToolResult[]   // canonical (FunctionTool)
  function perform(self, call: ComputerCall) -> Screenshot     // computer-use: drive the machine
  function observe(self, activity: ToolActivity) -> null       // hosted/MCP: read-only sink
}


// ----------------------------------------------------------------------------
// NET-NEW · ApprovalGate — human-in-the-loop as a wrapper, not a kind.
//
// HITL is approvals layered on top of any of the above (the background says so).
// It is a combinator over ExecutionContext: it intercepts dispatch/perform,
// surfaces the proposed call to a person, and proceeds only on approval. Because
// it wraps a *context*, it composes with any provider's loop unchanged.
// ----------------------------------------------------------------------------

interface Approver {
  // returns the (possibly edited) args to run, or throws to deny.
  function review(self, name: string, args: map<string, unknown>) -> map<string, unknown> throws baml.errors.LlmClient
}

class ApprovalGate {
  inner: ExecutionContext
  approver: Approver
  gated: string[]                         // tool names that require approval

  implements ExecutionContext {
    function dispatch(self, calls: ToolCall[]) -> ToolResult[] {
      let approved: ToolCall[] = [];
      for (let c in calls) {
        if (self.gated.contains(c.name)) {
          // pause the run for a person; review throws to deny, mutates to edit.
          let args = self.approver.review(c.name, c.args);
          approved = approved + [ToolCall { id: c.id, name: c.name, args: args }];
        } else {
          approved = approved + [c];
        }
      }
      self.inner.dispatch(approved)
    }
    // computer-use clicks are real side effects — gate them too if requested.
    function perform(self, call: ComputerCall) -> Screenshot {
      if (self.gated.contains("computer_use")) {
        self.approver.review("computer_use", call.action);
      }
      self.inner.perform(call)
    }
    // hosted/MCP status is observe-only; nothing to gate, just forward.
    function observe(self, activity: ToolActivity) -> null { self.inner.observe(activity) }
  }
}


// ============================================================================
// PROVIDER · Anthropic (Claude Messages API) — the real divergences.
//
// Claude correlates CLIENT tool results by `tool_use_id`. HOSTED tools
// (web_search_20260209, code_execution_20260120) come back as `server_tool_use`
// + `*_tool_result` blocks the model produced; when the server-side sampling
// loop hits its 10-iteration cap the response is `stop_reason: "pause_turn"` and
// you RE-SEND to continue — you never drive the hosted call. Claude has no
// first-party computer-use-via-Responses analog the way OpenAI does, but it ships
// a client-side computer tool, so ComputerUseTool maps to a client-driven leg.
// MCP is Anthropic's `mcp_servers` connector (Streamable HTTP + OAuth).
// ============================================================================

class Anthropic {
  model: string
  api_key: string?
  max_tokens: int

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      $rust_io_function     // base chat request; tool encoding lives in build_tools below
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text())
    }
  }

  // Encode the heterogeneous ToolKind[] into Anthropic's `tools` + `mcp_servers`
  // wire shape. This is where execution-location divergence becomes concrete:
  // each kind serializes differently, and HostedTool/McpServerTool carry NO
  // client schema.
  function build_tools(self, tools: ToolKind[]) -> baml.http.Request throws baml.errors.LlmClient {
    let entries: map<string, unknown>[] = [];
    let mcp_servers: map<string, unknown>[] = [];
    for (let tk in tools) {
      match (tk) {
        let f: FunctionTool => {
          // client-executed: full JSON-Schema from the first-class `type`.
          entries = entries + [{
            "name": f.name,
            "description": f.description,
            "input_schema": reflect.json_schema_of(f.parameters),
          }];
        },
        let h: HostedTool => {
          // server-hosted: version-suffixed `type`, NO input_schema, config folded in.
          let kind_wire = match (h.kind) {
            "web_search"      => "web_search_20260209",
            "code_execution"  => "code_execution_20260120",
            _                 => h.kind,
          };
          entries = entries + [merge({ "type": kind_wire, "name": h.kind }, h.config)];
        },
        let c: ComputerUseTool => {
          entries = entries + [{
            "type": "computer_20250124",
            "name": "computer",
            "display_width_px": c.display_width,
            "display_height_px": c.display_height,
            "environment": c.environment,
          }];
        },
        let m: McpServerTool => {
          // remote MCP: declared as a connector, not a tool entry. Auth is
          // resolved to a bearer the connector uses (OAuth flow run host-side).
          mcp_servers = mcp_servers + [{
            "type": "url",
            "name": m.server_label,
            "url": m.server_url,
            "authorization_token": self.resolve_mcp_token(m.auth),
            "tool_configuration": match (m.require_approval) {
              let _: ApprovalNever    => { "require_approval": "never" },
              let _: ApprovalAlways   => { "require_approval": "always" },
              let p: ApprovalPerTool  => { "require_approval": { "always": { "tool_names": p.tool_names } } },
            },
          }];
        },
      }
    }
    $rust_io_function   // splice `entries` -> tools[], `mcp_servers` -> mcp_servers[]
  }

  // Resolve McpAuth to a bearer. StaticBearer is a passthrough (first-party only).
  // OAuth21 runs the discovery + audience-bound token grant — the `resource`
  // parameter (RFC 8707) is what binds the token to THIS server.
  function resolve_mcp_token(self, auth: McpAuth) -> string throws baml.errors.LlmClient {
    match (auth) {
      let b: StaticBearer => { b.token },
      let o: OAuth21 => {
        // 1. unauthenticated probe -> 401 WWW-Authenticate -> .well-known metadata
        // 2. (optional) Dynamic Client Registration if o.client_id is null
        // 3. auth-code + PKCE; `resource = o.resource` on BOTH auth & token calls
        //    => the issued token is audience-bound to o.resource and cannot be
        //    replayed against any other MCP server.
        self.mcp_oauth_flow(o)
      },
    }
  }
  function mcp_oauth_flow(self, o: OAuth21) -> string throws baml.errors.LlmClient { $rust_io_function }

  implements Tools {
    type Transcript = AnthropicTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> AnthropicTranscript throws baml.errors.LlmClient {
      $rust_io_function   // function-tool-only path (canonical Tools)
    }
    function step<T>(self, t: AnthropicTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
      $rust_io_function   // reads tool_use blocks -> ToolCalls correlated by tool_use_id
    }
    function submit(self, t: AnthropicTranscript, results: ToolResult[]) -> AnthropicTranscript throws baml.errors.LlmClient {
      // Claude correlates by tool_use_id: each ToolResult.id MUST equal a prior
      // tool_use block's id, or the follow-up 400s. Order is not load-bearing
      // for Claude (id-correlated), unlike Gemini's positional results.
      $rust_io_function
    }
  }

  implements MixedTools {
    function begin_mixed<T>(self, prompt: baml.llm.PromptAst, tools: ToolKind[]) -> AnthropicTranscript throws baml.errors.LlmClient {
      let req = self.build_tools(tools);
      $rust_io_function   // send req; seed transcript with the heterogeneous tool set
    }

    // The divergence the loop must absorb: ONE response can interleave a
    // function tool_use (drive it), a computer_use block (drive it differently),
    // and server_tool_use / mcp_tool_use blocks (observe only). We classify each
    // block by which arm of step_mixed's sum it belongs to. pause_turn is the
    // hosted-tool tell: the provider ran a server tool and wants to continue.
    function step_mixed<T>(self, t: AnthropicTranscript)
        -> T | ToolCalls | ComputerCalls | ToolActivity throws baml.errors.LlmClient {
      $rust_io_function
      // Concretely the native body does:
      //   - block.type == "tool_use"           -> ToolCalls   (id = block.id, your code)
      //   - block.type == "computer_use"        -> ComputerCalls (call_id = block.id)
      //   - block.type == "server_tool_use"     -> ToolActivity (id = block.id, phase from result)
      //     | "web_search_tool_result"
      //     | "code_execution_tool_result"
      //   - block.type == "mcp_tool_use"|"mcp_tool_result" -> ToolActivity (source = "mcp:"+server)
      //   - stop_reason == "pause_turn"         -> re-send to advance the hosted loop,
      //                                             surfaced as a ToolActivity{phase:"in_progress"}
      //   - stop_reason == "end_turn"           -> parse<T> the final message
    }

    function submit_screenshots(self, t: AnthropicTranscript, shots: Screenshot[]) -> AnthropicTranscript throws baml.errors.LlmClient {
      // computer-use answer: each screenshot becomes a tool_result with the
      // matching call_id and an image content block. Correlated by call_id, NOT
      // by ToolResult.id — that's why this isn't `submit`.
      $rust_io_function
    }
  }
}

class AnthropicTranscript { _data: $rust_type }   // provider-owned, opaque


// ============================================================================
// PROVIDER · OpenAIResponses — the Responses API, where the taxonomy is widest.
//
// This is the provider the background's Python examples target. It is where
// hosted tools, computer_use_preview, and the `mcp` bridge are all first-class —
// and where they are *endpoint-gated*: the same tools largely vanish on Chat
// Completions. The divergences from Anthropic: server-side MCP dispatch (the
// provider talks to your MCP server), `previous_response_id` threading for the
// computer-use loop, and computer_call/computer_call_output as the wire shape.
// ============================================================================

class OpenAIResponses {
  model: string
  api_key: string

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      $rust_io_function   // POST /v1/responses
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.json_path("output_text"))
    }
  }

  function build_tools(self, tools: ToolKind[]) -> baml.http.Request throws baml.errors.LlmClient {
    let entries: map<string, unknown>[] = [];
    for (let tk in tools) {
      match (tk) {
        let f: FunctionTool => {
          entries = entries + [{
            "type": "function",
            "name": f.name,
            "description": f.description,
            "parameters": reflect.json_schema_of(f.parameters),
          }];
        },
        let h: HostedTool => {
          // Responses hosted tools: bare {"type": kind} + config. NO version suffix.
          entries = entries + [merge({ "type": h.kind }, h.config)];
        },
        let c: ComputerUseTool => {
          entries = entries + [{
            "type": "computer_use_preview",
            "display_width": c.display_width,
            "display_height": c.display_height,
            "environment": c.environment,
          }];
        },
        let m: McpServerTool => {
          // KEY DIVERGENCE from Anthropic: MCP is a `tools` ENTRY here, and the
          // provider dispatches to the server SERVER-SIDE. Same OAuth/audience
          // model; the connector still wants an audience-bound token.
          entries = entries + [{
            "type": "mcp",
            "server_label": m.server_label,
            "server_url": m.server_url,
            "authorization": self.resolve_mcp_token(m.auth),
            "require_approval": match (m.require_approval) {
              let _: ApprovalNever    => "never",
              let _: ApprovalAlways   => "always",
              let p: ApprovalPerTool  => { "always": { "tool_names": p.tool_names } },
            },
          }];
        },
      }
    }
    $rust_io_function
  }

  function resolve_mcp_token(self, auth: McpAuth) -> string throws baml.errors.LlmClient {
    match (auth) {
      let b: StaticBearer => { b.token },
      let o: OAuth21      => { self.mcp_oauth_flow(o) },   // same RFC-8707 binding
    }
  }
  function mcp_oauth_flow(self, o: OAuth21) -> string throws baml.errors.LlmClient { $rust_io_function }

  implements Tools {
    type Transcript = ResponsesTranscript
    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> ResponsesTranscript throws baml.errors.LlmClient { $rust_io_function }
    function step<T>(self, t: ResponsesTranscript) -> T | ToolCalls throws baml.errors.LlmClient { $rust_io_function }
    function submit(self, t: ResponsesTranscript, results: ToolResult[]) -> ResponsesTranscript throws baml.errors.LlmClient {
      // Responses correlates function results by call_id and threads state via
      // previous_response_id (the transcript holds it). Each function_call_output
      // names its call_id.
      $rust_io_function
    }
  }

  implements MixedTools {
    function begin_mixed<T>(self, prompt: baml.llm.PromptAst, tools: ToolKind[]) -> ResponsesTranscript throws baml.errors.LlmClient {
      let req = self.build_tools(tools);
      $rust_io_function
    }
    function step_mixed<T>(self, t: ResponsesTranscript)
        -> T | ToolCalls | ComputerCalls | ToolActivity throws baml.errors.LlmClient {
      $rust_io_function
      // Classify Responses output[] items:
      //   item.type == "function_call"          -> ToolCalls    (call_id, your code)
      //   item.type == "computer_call"           -> ComputerCalls (call_id; action = item.action)
      //   item.type == "web_search_call"
      //     | "file_search_call"
      //     | "code_interpreter_call"            -> ToolActivity (id = item.id, observe-only)
      //   item.type == "mcp_call" | "mcp_list_tools"
      //     | "mcp_approval_request"             -> ToolActivity (source = "mcp:"+server_label)
      //   final message                          -> parse<T>(output_text)
    }
    function submit_screenshots(self, t: ResponsesTranscript, shots: Screenshot[]) -> ResponsesTranscript throws baml.errors.LlmClient {
      // Responses computer-use answer: a `computer_call_output` input item with
      // {"type":"input_image","image_url":"data:image/png;base64,<b64>"} keyed by
      // call_id, sent with previous_response_id. This is the loop the background's
      // Python `perform(action)` example bounces through.
      $rust_io_function
    }
  }
}

class ResponsesTranscript { _data: $rust_type }


// ----------------------------------------------------------------------------
// Host helpers used above (genuine host boundaries / existing primitives).
// ----------------------------------------------------------------------------
function merge(a: map<string, unknown>, b: map<string, unknown>) -> map<string, unknown> { $rust_io_function }
// reflect.json_schema_of(t: type) -> map<string,unknown> : the existing `type` -> JSON Schema lowering.
