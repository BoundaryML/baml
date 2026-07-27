# Direct calls and loop ownership

There are two legitimate kinds of tool loop. They are distinguished by who
executes the tools.

| | Provider-managed loop | BAML Agent runner |
| --- | --- | --- |
| Runs inside | Vendor or remote service | BAML runtime |
| Executes | Provider-owned tools | Application BAML functions |
| Examples | Hosted search, hosted code execution, remote coding agent | `lookup_order`, `search_policy`, MCP functions |
| BAML sees | Final response, resource, or provider events | Every model step, tool call, result, and stop |
| Configured on | Provider | Agent runner |
| Returns | `T`, `Response<T>`, or resource | `AgentOutcome<T>` |

The two loops must never both claim ownership of the same application-tool
call.

## Standard direct execution

All direct LLM function calls conceptually go through:

```baml
function run_direct<T, P>(
  task: ai.Task<T, P>,
) -> T throws ai.DirectCallError {
  if (task.application_tools.is_empty()) {
    return task.run(
      runner = ai.run.Completion.new(),
    )
  }

  let outcome = task.run(
    runner = ai.run.Agent.new(),
  )

  match (outcome) {
    let done: ai.Done<T> => done.value,

    let stopped: ai.BudgetReached => {
      throw ai.AgentIncomplete {
        outcome: stopped,
      }
    },

    let handoff: ai.Handoff => {
      throw ai.AgentIncomplete {
        outcome: handoff,
      }
    },
  }
}
```

This is semantic pseudocode. The implementation may specialize the branch at
compile time when the default tool set is statically known.

## Dynamic tool overrides

The decision uses the effective application-tool set on the task, after task
overrides:

```baml
let task = ResolveTicket
  .task(ticket)
  .with_tools([])
```

Running this task through an explicit Agent is still valid. If a future API
allows direct execution of an already-created task, its standard lifecycle
would see an empty tool set.

The syntax `ResolveTicket(ticket)` uses the declaration's default task and
therefore the declaration's application tools.

## Direct-call result

A direct call preserves the familiar function contract:

```baml
ResolveTicket(ticket) -> Resolution
```

`Done<Resolution>` is unwrapped. A resumable stop is not silently discarded.
It becomes `AgentIncomplete`, which retains:

- the terminal outcome kind;
- the conversation needed to resume;
- the stop or handoff reason;
- step and usage totals; and
- the originating LLM function identity.

The exception should tell the user how to opt into explicit lifecycle handling:

```text
ResolveTicket stopped after 12 Agent steps.
Use ResolveTicket.task(...).run(runner = ai.run.Agent.new(...))
to handle BudgetReached or Handoff.
```

## Capability selection

The compiler checks the standard lifecycle implied by the LLM declaration:

- no application tools requires `CompletionProvider`;
- one or more application tools requires `ToolCallingProvider`.

Provider-owned tools do not affect this decision. They are not `ai.Tool`
values in the task's application-tool set.

## Provider-managed loops

A provider may execute its own hosted tools or remote agent:

```baml
function InvestigateRepository(issue: Issue) -> Report {
  provider: RemoteCodingAgent

  prompt: `
    Investigate this issue.

    ${issue}

    ${ctx.output_format}
  `
}
```

From BAML's perspective this is one bounded provider operation. The provider
may expose progress events or a resource, but it does not ask the BAML Agent
runner to dispatch its hosted tools.

If the function adds an application tool:

```baml
tools: [lookup_internal_ticket]
```

then the standard direct path uses the BAML Agent. The provider must expose the
tool-calling turn protocol needed for BAML to receive that call and return its
result.

## Ownership invariant

The architecture must not be:

```text
provider.complete(task)
  → starts the BAML Agent
      → calls the same provider for steps
```

That makes a provider method appear to own application execution.

The architecture is:

```text
run_direct(task)
  ├─ no application tools → Completion runner → provider bounded operation
  └─ application tools    → Agent runner → provider turn protocol
```

The Agent owns application control flow. The provider owns vendor protocol and
provider state.
