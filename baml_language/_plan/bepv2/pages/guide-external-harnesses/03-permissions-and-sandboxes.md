# Harness permissions and sandboxes

> **Status:** Partial — options and policy ownership are implemented. The
> in-process reference adapter does not provide an OS sandbox.

Permission policy and execution isolation solve different problems.

```text
permission policy: may this requested tool operation run?
sandbox:           where and with what OS/filesystem/network authority does it run?
```

## Configure both

```baml
let harness = ai.HarnessAgent {
  inner: ClaudeCode {
    sandbox: ai.ContainerSandbox { image: "baml-dev" },
    permissions: ReadOnlyUnlessApproved {},
  },
}
```

## Approval policy

```baml
function decide(call: ai.ToolCall) -> ai.ApprovalDecision {
  match (call.name) {
    "read" | "grep" => ai.ApprovalDecision.allow(),
    "edit" | "write" => ai.ApprovalDecision.ask_user(),
    "bash" => ai.ApprovalDecision.deny("shell disabled for this run"),
    _ => ai.ApprovalDecision.deny("unknown harness tool"),
  }
}
```

Approval is a control signal, not a fake tool result or transport error. A
sandbox remains necessary even when policy allows a command: it limits the
consequences of mistakes and compromised tools.

## Test it

Use a virtual sandbox and scripted calls. Assert that denied tools never reach
the sandbox and approved writes cannot escape the configured workspace root.
