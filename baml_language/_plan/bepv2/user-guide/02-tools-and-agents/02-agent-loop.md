# Run the complete agent loop

An agent run may finish normally, stop at a budget, or hand control elsewhere.
Use the explicit outcome union when those states are application control flow.

## Use it

```baml
let outcome = ai.drivers.run_agent(
  ResolveTicket.task(ticket, $provider = ToolModel),
  ai.AgentOptions {
    budget: ai.Budget { max_steps: 8, max_cost_usd: 0.25 },
    tools: [lookup_order],
  },
)

match (outcome) {
  let done: ai.Done<Resolution> => done.value,
  let stopped: ai.BudgetReached => queue_for_review(stopped.transcript),
  let handoff: ai.Handoff => route_handoff(handoff),
}
```

## The loop in one block

```text
provider.begin(task)
while true:
  provider.step(transcript, active_tools)
    T         -> return Done<T>
    ToolCalls -> validate, dispatch, observe, provider.submit(results)
  enforce budget and handoff policy
```

`ToolCallingProvider` owns provider turns and its exact transcript.
`run_agent` owns dispatch order, hooks, budgets, switching, and termination.

## Direct-call convenience

An `Agent` provider may package this loop as its `DriveProvider` behavior:

```baml
let SupportAgent = ai.Agent {
  inner: ToolModel,
  options: ai.AgentOptions { tools: [lookup_order] },
}

let resolution = ResolveTicket(ticket, $provider = SupportAgent)
```

Use the direct form only when non-value outcomes have a documented policy.
Use `run_agent` when the application must distinguish them.

## Related design and scenarios

- [ToolCallingProvider versus Agent](../../pages/05-tools-and-agents.md#toolcallingprovider-versus-agent)
- Scenario 10 agentic loop

