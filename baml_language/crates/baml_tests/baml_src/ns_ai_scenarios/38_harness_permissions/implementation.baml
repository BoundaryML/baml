// Scenario 38 — permissions decide which tool calls may run; a sandbox is the
// separate fence that decides where commands execute.

enum s38_ApprovalBehavior {
  Allow
  Deny
  Ask
}

class s38_ApprovalDecision {
  behavior: s38_ApprovalBehavior,
  updated_args: string?,
  message: string?,
}

class s38_PermissionDenied {
  tool: string,
  mode: string,
  reason: string,

  implements root.ai.Failure {
    function kind(self) -> root.ai.FailureKind throws never { root.ai.FailureKind.Refusal }
    function commit_state(self) -> root.ai.CommitState throws never { root.ai.CommitState.NotCommitted }
    function retry_after(self) -> baml.time.Duration? throws never { null }
    function is_resumable(self) -> bool throws never { false }
  }
}

interface s38_Sandbox {
  function label(self) -> string throws never
  function exec(self, command: string) -> string throws never
}

class s38_VirtualSandbox {
  implements s38_Sandbox {
    function label(self) -> string throws never { "virtual" }
    function exec(self, command: string) -> string throws never { "virtual:" + command }
  }
}

class s38_ContainerSandbox {
  image: string,

  implements s38_Sandbox {
    function label(self) -> string throws never { "container:" + self.image }
    function exec(self, command: string) -> string throws never { "container:" + command }
  }
}

function s38_is_write_tool(name: string) -> bool throws never {
  name == "write_file" || name == "edit" || name == "bash"
}

function s38_allow() -> (root.ai.ToolCall) -> s38_ApprovalDecision throws never {
  (call: root.ai.ToolCall) -> s38_ApprovalDecision {
    s38_ApprovalDecision { behavior: s38_ApprovalBehavior.Allow, updated_args: null, message: null }
  }
}

function s38_deny() -> (root.ai.ToolCall) -> s38_ApprovalDecision throws never {
  (call: root.ai.ToolCall) -> s38_ApprovalDecision {
    s38_ApprovalDecision {
      behavior: s38_ApprovalBehavior.Deny,
      updated_args: null,
      message: "operator denied",
    }
  }
}

function s38_ask() -> (root.ai.ToolCall) -> s38_ApprovalDecision throws never {
  (call: root.ai.ToolCall) -> s38_ApprovalDecision {
    s38_ApprovalDecision {
      behavior: s38_ApprovalBehavior.Ask,
      updated_args: null,
      message: "operator approval required",
    }
  }
}

function s38_execute(call: root.ai.ToolCall, sandbox: s38_Sandbox) -> root.ai.ToolResult throws never {
  if (call.name == "bash") {
    let args = baml.json.from_string<s38_BashArgs>(call.args) catch (e) {
      _ => return root.ai.ToolResult { id: call.id, output: "invalid bash arguments" },
    };
    return root.ai.ToolResult { id: call.id, output: sandbox.exec(args.command) };
  }
  if (call.name == "read_file") {
    return root.ai.ToolResult { id: call.id, output: "read ok" };
  }
  root.ai.ToolResult { id: call.id, output: "wrote ok" }
}

// This is the single permission chokepoint. Declarative allow/deny lists run
// first, then the per-call callback decides Ask-mode writes. With no live
// operator, Ask defaults to deny.
function s38_gate(
  calls: root.ai.ToolCall[],
  mode: string,
  allowed_tools: string[],
  denied_tools: string[],
  sandbox: s38_Sandbox,
  approve: ((root.ai.ToolCall) -> s38_ApprovalDecision throws never)?,
) -> root.ai.ToolResult[] throws s38_PermissionDenied {
  let results: root.ai.ToolResult[] = [];
  for (let call in calls) {
    if (!allowed_tools.includes(call.name) || denied_tools.includes(call.name)) {
      throw s38_PermissionDenied { tool: call.name, mode: mode, reason: "tool is outside the declared roster" };
    }
    if (!s38_is_write_tool(call.name) || mode == "full-auto") {
      results.push(s38_execute(call, sandbox));
      continue;
    }
    if (mode == "read-only" || mode == "plan") {
      throw s38_PermissionDenied { tool: call.name, mode: mode, reason: "mode forbids writes" };
    }
    if (mode == "ask") {
      let decision = match (approve) {
        let callback: (root.ai.ToolCall) -> s38_ApprovalDecision => callback(call),
        null => s38_ApprovalDecision {
          behavior: s38_ApprovalBehavior.Deny,
          updated_args: null,
          message: "unattended ask defaults to deny",
        },
      };
      match (decision.behavior) {
        s38_ApprovalBehavior.Allow => {
          let effective = root.ai.ToolCall {
            id: call.id,
            name: call.name,
            args: decision.updated_args ?? call.args,
          };
          results.push(s38_execute(effective, sandbox));
        },
        s38_ApprovalBehavior.Deny => throw s38_PermissionDenied {
          tool: call.name,
          mode: mode,
          reason: decision.message ?? "operator denied",
        },
        s38_ApprovalBehavior.Ask => throw s38_PermissionDenied {
          tool: call.name,
          mode: mode,
          reason: "no interactive approval channel was supplied",
        },
      }
      continue;
    }
    throw s38_PermissionDenied { tool: call.name, mode: mode, reason: "unknown permission mode" };
  }
  results
}

// Scenario-specific policy expressed through the shared behavioral middleware
// contract. Sandbox execution remains a separate dispatcher concern.
class s38_PermissionMiddleware {
  mode: string,
  allowed_tools: string[],
  denied_tools: string[],
  approve: ((root.ai.ToolCall) -> s38_ApprovalDecision throws never)?,

  implements root.ai.ToolMiddleware {
    function before_tool(
      self,
      context: root.ai.ToolContext,
      call: root.ai.ToolCall,
    ) -> root.ai.ToolDecision throws never {
      if (!self.allowed_tools.includes(call.name) || self.denied_tools.includes(call.name)) {
        return root.ai.ToolDecision.deny("(" + self.mode + "): tool is outside the declared roster");
      }
      if (!s38_is_write_tool(call.name) || self.mode == "full-auto") {
        return root.ai.ToolDecision.allow(call);
      }
      if (self.mode == "read-only" || self.mode == "plan") {
        return root.ai.ToolDecision.deny("(" + self.mode + "): mode forbids writes");
      }
      if (self.mode == "ask") {
        let decision = match (self.approve) {
          let callback: (root.ai.ToolCall) -> s38_ApprovalDecision => callback(call),
          null => s38_ApprovalDecision {
            behavior: s38_ApprovalBehavior.Deny,
            updated_args: null,
            message: "unattended ask defaults to deny",
          },
        };
        match (decision.behavior) {
          s38_ApprovalBehavior.Allow => root.ai.ToolDecision.allow(root.ai.ToolCall {
            id: call.id,
            name: call.name,
            args: decision.updated_args ?? call.args,
          }),
          s38_ApprovalBehavior.Deny => root.ai.ToolDecision.deny(
            "(" + self.mode + "): " + (decision.message ?? "operator denied"),
          ),
          s38_ApprovalBehavior.Ask => root.ai.ToolDecision.deny(
            "(" + self.mode + "): no interactive approval channel was supplied",
          ),
        }
      } else {
        root.ai.ToolDecision.deny("(" + self.mode + "): unknown permission mode")
      }
    }
  }
}

// The Tools loop requires a non-throwing dispatcher. A permission refusal is
// recoverable loop data, so it becomes ToolResult output for the model.
function s38_permissioned_dispatch(
  mode: string,
  allowed_tools: string[],
  denied_tools: string[],
  sandbox: s38_Sandbox,
  approve: ((root.ai.ToolCall) -> s38_ApprovalDecision throws never)?,
) -> (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never {
  (calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] {
    let execute = (approved: root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never {
      approved.map((call: root.ai.ToolCall) -> root.ai.ToolResult { s38_execute(call, sandbox) })
    };
    let middleware: root.ai.ToolMiddleware[] = [s38_PermissionMiddleware {
      mode: mode,
      allowed_tools: allowed_tools,
      denied_tools: denied_tools,
      approve: approve,
    }];
    root.ai.dispatch_with_middleware(
      calls,
      execute,
      middleware,
      [],
      [],
      root.ai.ToolContext { run_id: null, turn: 0, provider: "s38-permission-harness" },
    )
  }
}

class s38_HarnessState {
  remaining: root.ai.ToolCall[],
  final_json: string,
}

class s38_HarnessAgent {
  name: string,
  model: string?,
  sandbox: s38_Sandbox,
  permission_mode: string,
  allowed_tools: string[],
  denied_tools: string[],
  scripted_calls: root.ai.ToolCall[],
  final_json: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { self.name }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }

  implements root.ai.Tools {
    type Transcript = s38_HarnessState

    function begin<T>(self, request: root.ai.Request<T>) -> s38_HarnessState
        throws baml.errors.ToolError | baml.errors.UnknownError {
      s38_HarnessState { remaining: self.scripted_calls, final_json: self.final_json }
    }
    function step<T>(self, transcript: s38_HarnessState) -> T | root.ai.ToolCalls
        throws baml.errors.ToolError | baml.errors.UnknownError {
      match (transcript.remaining.at(0)) {
        let call: root.ai.ToolCall => root.ai.ToolCalls { calls: [call] },
        null => baml.json.from_string<T>(transcript.final_json) catch (e) {
          _ => throw baml.errors.UnknownError { data: e, message: ["permission harness final parse failed"] },
        },
      }
    }
    function submit(
      self,
      transcript: s38_HarnessState,
      results: root.ai.ToolResult[],
    ) -> s38_HarnessState throws baml.errors.ToolError | baml.errors.UnknownError {
      s38_HarnessState {
        remaining: transcript.remaining.slice(1, transcript.remaining.length()),
        final_json: transcript.final_json,
      }
    }
  }
}

function s38_read_only_harness(calls: root.ai.ToolCall[], final_json: string) -> s38_HarnessAgent throws never {
  s38_HarnessAgent {
    name: "read-only-harness",
    model: "claude-sonnet-4-6",
    sandbox: s38_VirtualSandbox {},
    permission_mode: "read-only",
    allowed_tools: ["read_file", "write_file"],
    denied_tools: [],
    scripted_calls: calls,
    final_json: final_json,
  }
}

function s38_full_auto_harness(calls: root.ai.ToolCall[], final_json: string) -> s38_HarnessAgent throws never {
  s38_HarnessAgent {
    name: "full-auto-harness",
    model: "claude-sonnet-4-6",
    sandbox: s38_ContainerSandbox { image: "baml-test:latest" },
    permission_mode: "full-auto",
    allowed_tools: ["read_file", "write_file", "bash"],
    denied_tools: [],
    scripted_calls: calls,
    final_json: final_json,
  }
}
