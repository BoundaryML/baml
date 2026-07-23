# Handoffs and budgets

> **Status:** Implemented in the executable reference.

Budget stops and handoffs are control-flow outcomes, not fabricated model
values and not ordinary exceptions.

## Mark a handoff tool

```baml
let transfer_to_human = ai.tool(
  "transfer_to_human",
  "Transfer this ticket to a human support agent.",
  (args: TransferArgs) -> null {
    baml.sys.panic("handoff tools are not dispatched locally")
  },
).as_handoff()
```

The driver recognizes the call and returns `Handoff`; it does not dispatch the
handler in this process.

## Handle every terminal outcome

```baml
match (ai.drivers.run_agent(
  ResolveTicket.task(ticket, $provider = ToolModel),
  ai.AgentOptions.new(
    budget = ai.Budget { max_steps: 8, max_cost_usd: 0.25 },
    tools = [lookup_order, transfer_to_human],
  ),
)) {
  let done: ai.Done<Resolution> => persist(done.value),
  let stopped: ai.BudgetReached => queue_for_review(stopped.transcript),
  let handoff: ai.Handoff => send_to_queue(handoff.to, handoff.args),
}
```

## Why direct calls differ

An `Agent` used as `DriveProvider` must document how it resolves budgets and
handoffs while preserving the LLM function's return type. Applications that
need these outcomes should call `run_agent` explicitly.
