// ===========================================================================
// 38 — Permissions & sandboxing
// implement.baml  —  STD-LIB / library-author code
//
// Builds on the canonical primitives (Provider, Realtime, Tools, Channel,
// ExecutionContext, ToolCall/ToolResult). A harness that can run `Bash`/`Edit`
// is dangerous by construction; this file models the two fences the background
// names:
//
//   * permissions  — WHICH tools (allow/deny lists, pattern-scoped denials,
//                     permission modes, a per-call approval callback, and the
//                     declarative `require_approval` on a Tool).
//   * sandbox      — WHERE shell commands run (virtual / local / container).
//
// The teaching points the bodies make real:
//   1. allowed_tools / disallowed_tools / permission_mode are PROVIDER FIELDS
//      — bound config, exactly like `model`. Mode vocabulary is provider-
//      specific (`Sandbox` carries one provider's words; another provider has
//      no modes at all and degrades to the tool list).
//   2. The approval callback rides as an ExecutionContext HOOK, not a provider
//      field, because it needs the live per-call data and a yes/no/ask answer.
//      For a long-lived steerable session the SAME decision surfaces as a
//      Channel EVENT (the host asks; the operator answers over `io`).
//   3. `require_approval` is DECLARATIVE DATA on a Tool — a sieve that the
//      provider reads when wiring the request, distinct from the callback.
//   4. The sandbox is a BOUND FIELD (an interface with local/container/virtual
//      implementations), because "where Bash runs" is config the provider must
//      hold, not something handed in at the call.
//   5. Pattern matching on commands (`Bash(rm*)`) is a sieve, not a fence; the
//      real fence is the sandbox boundary. We show both, and the evaluation is
//      blunt about which one actually contains damage.
// ===========================================================================


// ---------------------------------------------------------------------------
// NET-NEW host primitive: the execution Sandbox (a bound provider field).
//
// "Where does Bash run?" spans a spectrum (background §3): an in-process virtual
// shell (just-bash), the local machine, or a remote container. We model it as a
// capability interface with three implementations. It is the FENCE — the only
// thing that actually contains a destructive command, because pattern policy is
// a sieve. The provider holds a `Sandbox` by value; it is never handed in at the
// call (unlike a Channel), because it is configuration, not a live handle.
// ---------------------------------------------------------------------------

// NET-NEW capability error interface: executing a shell command in a fence is a
// capability beyond call/stream/realtime/tools, so it gets its OWN error channel
// (flagged net-new in evaluation.md). The classifiers a sandbox can meaningfully
// answer are network (the container connector dropped) and a generic "the fence
// itself failed"; rate-limit/parse don't apply, so the interface stays minimal.
interface baml.errors.SandboxError {
  function is_network_error(self) -> bool
  function is_exec_failure(self) -> bool
}

interface Sandbox {
  // The sandbox executes a shell command and returns its captured output. This
  // is the ONLY method the tool loop needs; everything else (isolation, lifetime)
  // is the implementation's business.
  function exec(self, command: string, cwd: string?) -> ShellResult
      throws baml.ExtendUnknownError<baml.errors.SandboxError>

  // A read-interface so policy code can report which fence is in force without
  // knowing the concrete type. `kind` is "virtual" | "local" | "container".
  function kind(self) -> string
}

class ShellResult { stdout: string, stderr: string, exit_code: int }

// In-process virtual shell (Flue default). No real OS process — a just-bash VM.
// Cheap and disposable, but doesn't implement every POSIX corner (see evaluation).
class VirtualSandbox {
  _vm: $rust_type                                   // host-owned just-bash interpreter handle
  implements Sandbox {
    function exec(self, command: string, cwd: string?) -> ShellResult
        throws baml.ExtendUnknownError<baml.errors.SandboxError> {
      just_bash_exec(self._vm, command, cwd)          // runs in-memory; no host fs by default
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.SandboxError>(e, "virtual sandbox exec failed");
    }
    function kind(self) -> string { "virtual" }
  }
  function new() -> VirtualSandbox { VirtualSandbox { _vm: just_bash_new() } }
}

// The host's own working directory. Fast, UNFENCED — trusted CI only.
class LocalSandbox {
  cwd: string
  implements Sandbox {
    function exec(self, command: string, cwd: string?) -> ShellResult
        throws baml.ExtendUnknownError<baml.errors.SandboxError> {
      local_shell_exec(command, cwd ?? self.cwd)      // real process on the host machine
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.SandboxError>(e, "local sandbox exec failed");
    }
    function kind(self) -> string { "local" }
  }
}

// A provisioned, disposable container (Daytona/E2B). Real isolation; the handle
// was created out-of-band and bound in at client-construction time.
class ContainerSandbox {
  _handle: $rust_type                               // remote container session (created by connector)
  implements Sandbox {
    function exec(self, command: string, cwd: string?) -> ShellResult
        throws baml.ExtendUnknownError<baml.errors.SandboxError> {
      container_exec(self._handle, command, cwd)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.SandboxError>(e, "container sandbox exec failed");
    }
    function kind(self) -> string { "container" }
  }
}


// ---------------------------------------------------------------------------
// require_approval — DECLARATIVE DATA on a Tool.
//
// The Responses-API hosted-MCP path expresses approval as data on the tool spec
// rather than a callback: require_approval = "never" | "always" | {always:{tool_names}}.
// We can't add a field to the canonical `Tool` (it is `{name, description,
// parameters}`), so we model the declarative gate as a SIDE TABLE the provider
// consults: a list of `ApprovalRule` paired with the tools. This keeps the
// canonical Tool unchanged and makes the approval posture composable, ordinary
// data — a sieve the provider reads when wiring the request.
// ---------------------------------------------------------------------------

class ApprovalRule {
  tool: string                  // tool name the rule applies to ("*" = all)
  require: string               // "never" | "always"
  // optional pattern scope: "Bash(rm*)" denies the matching invocation outright.
  deny_pattern: string?
}

// A bundle of tools plus their declarative approval rules. The provider reads
// `rules` at begin() to decide which calls need a gate BEFORE any callback runs.
class ToolPolicy {
  tools: Tool[]
  rules: ApprovalRule[]

  function needs_approval(self, call: ToolCall) -> bool {
    for (let r in self.rules) {
      if ((r.tool == call.name || r.tool == "*") && r.require == "always") { return true; }
    }
    false
  }

  // Pattern sieve: "Bash(rm*)" style. Returns true if the call is DENIED outright
  // by a declarative deny_pattern. NOTE (background §3 "what's hard"): this is a
  // sieve — it stops the literal `rm` but not `find . -delete`. The fence is the
  // sandbox, not this function. We keep it because providers expose it, not
  // because it is sufficient.
  function is_denied(self, call: ToolCall) -> bool {
    for (let r in self.rules) {
      match (r.deny_pattern) {
        let pat: string => { if (command_matches(call, pat)) { return true; } },
        _ => {},
      }
    }
    false
  }
}


// ---------------------------------------------------------------------------
// The approval callback — an ExecutionContext HOOK.
//
// Background §3: "the host hands the harness a callback consulted per tool call"
// returning {behavior: allow | deny | ask, updatedInput?}. This is NOT a provider
// field: it needs the LIVE per-call tool_input and returns a three-way answer,
// and the same hook must be swappable per-run without rebuilding the client. The
// natural home is the ExecutionContext, which already owns `dispatch` — the one
// chokepoint every tool call passes through (proposal §7). We extend it with an
// approval hook so the gate lives exactly where dispatch already is.
//
// (net-new: ExecutionContext gains an approval surface. The proposal §7 already
//  flags ExecutionContext needs `dispatch` + an event-`emit` surface; this is the
//  per-call decision half of that.)
// ---------------------------------------------------------------------------

class ApprovalDecision {
  behavior: string              // "allow" | "deny" | "ask"
  updated_input: map<string, unknown>?   // the callback may rewrite the args (Edit case)
  message: string?              // reason, surfaced to the model on deny
}

// The hook signature, as a plain BAML closure: (ToolCall) -> ApprovalDecision.
// "ask" means "fall through to interactive" — see PermissionedContext below,
// which turns "ask" into a Channel event when a control channel is present.


// ---------------------------------------------------------------------------
// Net-new ExecutionContext: PermissionedContext.
//
// The canonical Tools.run_tools calls `ctx.dispatch(calls)`. The base
// ExecutionContext just runs tools. PermissionedContext is the dispatcher that
// enforces permissions at the one chokepoint:
//
//   (1) declarative deny patterns (the sieve) — refuse `Bash(rm*)` style calls;
//   (2) declarative require_approval (data on the tool policy);
//   (3) the can_use_tool callback HOOK — allow / deny / ask, may rewrite args;
//   (4) "ask" escalation — if a control Channel is present, EMIT an approval
//       event and await the operator's answer; otherwise default-deny.
//
// All four ride above the ToolCall seam, so this dispatcher is portable across
// any Tools provider (id-based or name-based) and across any sandbox.
// ---------------------------------------------------------------------------

// A deliberate domain failure raised by the dispatcher when a callback returns a
// behavior that is neither "allow" / "deny" / "ask". It is a tool-loop error (it
// only arises while dispatching tool calls), so it implements ToolError. It is
// neither a network nor a parse failure — the classifiers reflect that.
class BadApprovalBehavior {
  behavior: string
  implements baml.errors.ToolError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

class PermissionedContext {
  policy: ToolPolicy                          // declarative rules (require_approval, deny patterns)
  can_use_tool: ((ToolCall) -> ApprovalDecision)?   // the programmatic callback hook
  control: Channel?                           // present in a steerable session; "ask" -> event

  implements ExecutionContext {
    function dispatch(self, calls: ToolCall[]) -> ToolResult[]
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      let out: ToolResult[] = [];
      for (let call in calls) {
        // (1) declarative deny pattern — the sieve. Refuse before anything runs.
        if (self.policy.is_denied(call)) {
          out.push(ToolResult {
            id: call.id,
            output: deny_payload("denied by policy pattern"),
          });
          continue;
        }

        // (2)+(3) decide the disposition: declarative require_approval OR the
        //         programmatic callback. The callback, if present, wins (it has
        //         the live args and can rewrite them).
        let decision: ApprovalDecision = self.decide(call);

        match (decision.behavior) {
          "deny" => {
            out.push(ToolResult {
              id: call.id,
              output: deny_payload(decision.message ?? "denied"),
            });
          },
          "allow" => {
            // the callback may have rewritten the input (Edit -> updatedInput).
            let effective = apply_updated_input(call, decision.updated_input);
            out.push(ToolResult { id: call.id, output: run_tool(effective) });
          },
          "ask" => {
            // (4) escalate. With a control channel, ask the operator; without one,
            //     "ask" is unsafe-by-default -> deny (a batch run can't block).
            let approved: bool = self.escalate(call);
            if (approved) {
              out.push(ToolResult { id: call.id, output: run_tool(call) });
            } else {
              out.push(ToolResult { id: call.id, output: deny_payload("operator denied") });
            }
          },
          _ => { throw BadApprovalBehavior { behavior: decision.behavior }; },
        }
      }
      out
    } catch (e) {
      // BadApprovalBehavior (a known ToolError) passes through unchanged; the foreign
      // baml.errors.Io a channel `send`/`await_approval` can raise is normalized here.
      _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "permissioned dispatch failed");
    }
  }

  // Resolve the disposition for one call: callback hook beats declarative rules.
  function decide(self, call: ToolCall) -> ApprovalDecision {
    match (self.can_use_tool) {
      let cb: (ToolCall) -> ApprovalDecision => { return cb(call); },
      _ => {},
    }
    // no callback: fall back to the declarative require_approval data.
    if (self.policy.needs_approval(call)) {
      ApprovalDecision { behavior: "ask", updated_input: null, message: null }
    } else {
      ApprovalDecision { behavior: "allow", updated_input: null, message: null }
    }
  }

  // "ask" -> a Channel EVENT when a control channel is present. This is the SAME
  // decision as the callback, surfaced over the live session: emit a request,
  // block on the operator's reply. Without a channel we cannot block, so deny.
  function escalate(self, call: ToolCall) -> bool
      throws baml.ExtendUnknownError<baml.errors.ToolError> {
    match (self.control) {
      let io: Channel => {
        io.send(approval_request_event(call));      // host serializes call -> OutEvent
        return await_approval(io, call.id);          // host blocks on the matching InEvent
      },
      _ => { return false; },                         // unattended run: ask == deny
    }
  } catch (e) {
    // the channel `send` raises a foreign baml.errors.Io; normalize onto the tool channel.
    _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "approval escalation failed");
  }
}

// host glue for the dispatcher
function command_matches(call: ToolCall, pattern: string) -> bool { $rust_io_function }  // "Bash(rm*)" sieve
function deny_payload(reason: string) -> unknown { $rust_io_function }
function apply_updated_input(call: ToolCall, updated: map<string, unknown>?) -> ToolCall { $rust_io_function }
function run_tool(call: ToolCall) -> unknown { $rust_io_function }
function approval_request_event(call: ToolCall) -> OutEvent { $rust_io_function }
function await_approval(io: Channel, call_id: string) -> bool { $rust_io_function }


// ---------------------------------------------------------------------------
// Provider #1: ClaudeCode — a harness with a FULL permission vocabulary.
//
// Divergence shown: this provider has the rich Claude-specific mode words
// (`default` | `acceptEdits` | `plan` | `bypassPermissions`) as a bound field,
// plus allowed/disallowed tool lists as bound fields, plus a bound `sandbox`.
// It is a harness whose transport is a subprocess: NO HttpProvider, and (per
// ../provider-as-marker.md) NO degenerate `call` either — a harness has no honest
// single-turn request/response. It IS a Provider purely via the marker (its
// capabilities `requires Provider`), and exposes only its REAL interactions:
// Tools (the agentic loop) and Realtime (the steerable session). The permission
// fields shape what the subprocess is ALLOWED to wire up; the per-call gate is
// still the ExecutionContext.
// ---------------------------------------------------------------------------

class ClaudeCode {
  model: string?
  sandbox: Sandbox                        // bound field: local | container | virtual (the FENCE)
  allowed_tools: string[]?                // bound allow-list  (e.g. ["Read","Edit","Grep"])
  disallowed_tools: string[]?             // bound deny-list   (e.g. ["Bash(rm*)"])
  permission_mode: string?                // CLAUDE-SPECIFIC vocab; see mode_to_rules below

  // No `implements Provider { call }`: a harness has no honest single-turn call to
  // fake (the OLD degenerate one-shot is retired — ../provider-as-marker.md §4).
  // The class is still a Provider via the marker, because Tools/Realtime below
  // `requires Provider`. Permission modes only mean something inside Tools/Realtime,
  // which is exactly why the fake `call` had nowhere to put an "ask".

  implements Tools {
    // Opaque, provider-owned. Holds the subprocess session + the permission flags
    // it was spawned with. The provider applies allowed/disallowed at the WIRE
    // (the model literally isn't offered a disallowed tool); the ExecutionContext
    // applies the per-call callback on top.
    type Transcript = $rust_type

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // filter `tools` by allowed_tools/disallowed_tools BEFORE offering them, so
      // the model never sees a forbidden tool. permission_mode is translated into
      // the subprocess flags here. The sandbox is bound into the session so every
      // Bash the model emits runs inside `self.sandbox.exec`.
      let offered = filter_tools(tools, self.allowed_tools, self.disallowed_tools);
      claude_code_begin(self, prompt, offered)        // $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "claude code begin failed");
    }
    function step<T>(self, t: Transcript) -> T | ToolCalls
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      claude_code_step(t)                             // $rust_io_function; emits ToolCalls or final T
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "claude code step failed");
    }
    function submit(self, t: Transcript, results: ToolResult[]) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      claude_code_submit(t, results)                  // $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "claude code submit failed");
    }
    // run_tools inherited from Tools default — the per-call gate lives in `ctx`.
  }

  implements Realtime {
    // The long-lived steerable session. The control/event channel is handed in at
    // the call (pass-in) — this is ALSO the channel over which an "ask" approval
    // event flows back to the operator (PermissionedContext.escalate). The mode
    // can be changed mid-session by sending a control event over `io`.
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      claude_code_session(self, prompt, io)           // $rust_io_function; drives steer/interrupt/approve
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "claude code session failed");
    }
  }

  // Translate the Claude-specific mode word into declarative ApprovalRules, so an
  // app can build a PermissionedContext from the provider's posture. This is the
  // ONLY place the mode vocabulary is interpreted — it is provider-specific data.
  function mode_to_rules(self) -> ApprovalRule[] {
    match (self.permission_mode ?? "default") {
      "bypassPermissions" => [ ApprovalRule { tool: "*", require: "never", deny_pattern: null } ],
      "acceptEdits"       => [
        ApprovalRule { tool: "Edit", require: "never", deny_pattern: null },
        ApprovalRule { tool: "Bash", require: "always", deny_pattern: null },
      ],
      "plan"              => [
        // read & propose, never act: every mutating tool requires approval.
        ApprovalRule { tool: "Edit", require: "always", deny_pattern: null },
        ApprovalRule { tool: "Bash", require: "always", deny_pattern: null },
        ApprovalRule { tool: "Write", require: "always", deny_pattern: null },
      ],
      _                   => [ ApprovalRule { tool: "Bash", require: "always", deny_pattern: null } ], // "default"
    }
  }

  function from(model: string, sandbox: Sandbox, mode: string) -> ClaudeCode {
    ClaudeCode {
      model: model,
      sandbox: sandbox,
      allowed_tools: null,
      disallowed_tools: null,
      permission_mode: mode,
    }
  }
}

function filter_tools(tools: Tool[], allow: string[]?, deny: string[]?) -> Tool[] { $rust_io_function }
function claude_code_begin(p: ClaudeCode, prompt: baml.llm.PromptAst, tools: Tool[]) -> $rust_type { $rust_io_function }
function claude_code_step<T>(t: $rust_type) -> T | ToolCalls { $rust_io_function }
function claude_code_submit(t: $rust_type, results: ToolResult[]) -> $rust_type { $rust_io_function }
function claude_code_session(p: ClaudeCode, prompt: baml.llm.PromptAst, io: Channel) -> $rust_type { $rust_io_function }


// ---------------------------------------------------------------------------
// Provider #2: FlueHarness — a harness with NO permission-mode vocabulary.
//
// Divergence (background §3 "what varies"): "other runtimes don't model modes at
// all and rely purely on the tool list." Flue has NO permission_mode field. Its
// only permission control is the allow/deny tool list + whatever the dispatcher
// enforces. Its distinguishing config is the SANDBOX — the virtual just-bash
// shell is the DEFAULT (omit `sandbox` to get it), which we model by defaulting
// the field to a VirtualSandbox.
//
// This proves portability of the PermissionedContext: the SAME dispatcher works,
// but `mode_to_rules` simply doesn't exist here — the app builds rules from the
// tool list alone. The provider divergence (mode vocab) stays inside the provider.
// ---------------------------------------------------------------------------

class FlueHarness {
  model: string
  sandbox: Sandbox                        // defaults to VirtualSandbox via from(); no modes
  allowed_tools: string[]?
  disallowed_tools: string[]?
  // NOTE: deliberately NO permission_mode — Flue has no mode vocabulary.

  // No `implements Provider { call }`: like ClaudeCode, this is a subprocess harness
  // with no honest single-turn call (../provider-as-marker.md §4). It IS a Provider
  // via the marker because Tools/Realtime below `requires Provider`.

  implements Tools {
    type Transcript = $rust_type
    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // same wire move as ClaudeCode: filter by allow/deny before offering. There
      // is no mode to translate — the tool list IS the permission policy here.
      let offered = filter_tools(tools, self.allowed_tools, self.disallowed_tools);
      flue_begin(self, prompt, offered)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "flue begin failed");
    }
    function step<T>(self, t: Transcript) -> T | ToolCalls
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      flue_step(t)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "flue step failed");
    }
    function submit(self, t: Transcript, results: ToolResult[]) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      flue_submit(t, results)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "flue submit failed");
    }
  }

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      flue_session(self, prompt, io)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "flue session failed");
    }
  }

  // Flue's default sandbox is the in-memory just-bash VM (background §3). A custom
  // factory or a Daytona container can be passed instead.
  function from(model: string) -> FlueHarness {
    FlueHarness {
      model: model,
      sandbox: VirtualSandbox.new(),       // the default fence — virtual, in-process
      allowed_tools: null,
      disallowed_tools: null,
    }
  }
  function with_sandbox(model: string, sandbox: Sandbox) -> FlueHarness {
    FlueHarness { model: model, sandbox: sandbox, allowed_tools: null, disallowed_tools: null }
  }
}

function flue_begin(p: FlueHarness, prompt: baml.llm.PromptAst, tools: Tool[]) -> $rust_type { $rust_io_function }
function flue_step<T>(t: $rust_type) -> T | ToolCalls { $rust_io_function }
function flue_submit(t: $rust_type, results: ToolResult[]) -> $rust_type { $rust_io_function }
function flue_session(p: FlueHarness, prompt: baml.llm.PromptAst, io: Channel) -> $rust_type { $rust_io_function }


// host boundaries for the sandbox implementations
function just_bash_new() -> $rust_type { $rust_io_function }
function just_bash_exec(vm: $rust_type, command: string, cwd: string?) -> ShellResult { $rust_io_function }
function local_shell_exec(command: string, cwd: string) -> ShellResult { $rust_io_function }
function container_exec(handle: $rust_type, command: string, cwd: string?) -> ShellResult { $rust_io_function }
