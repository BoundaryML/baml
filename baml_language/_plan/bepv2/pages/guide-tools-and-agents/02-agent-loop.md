# Run the complete agent loop

> **Status:** Implemented in the executable reference.

An agent run may finish normally, stop at a budget, or hand control elsewhere.
Use the explicit outcome union when those states are application control flow.

## Use it

```baml
let outcome = ai.drivers.run_agent(
  ResolveTicket.task(ticket, $provider = ToolModel),
  ai.AgentOptions.new(
    budget = ai.Budget { max_steps: 8, max_cost_usd: 0.25 },
    tools = [lookup_order],
  ),
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

## Native tools are optional

The same loop works whether the provider has a vendor tool API or uses BAML's
prompt/SAP fallback:

```text
native adapter
  active_tools -> request.tools -> native tool-call blocks

prompt-backed adapter
  active_tools -> ${ctx.output_format} for T | ToolCalls
               + each tool's JSON Schema
               -> SAP parses T or ToolCalls
```

Keep `${ctx.output_format}` in the LLM function prompt. A prompt-backed
`ToolCallingProvider` extends it automatically after the driver resolves the
tools for that step; the function author does not hand-write the `ToolCalls`
schema. Native adapters leave it as the final `T` schema and put tools in the
provider request fields instead.

Only application tools with a driver dispatch path can use this fallback.
Provider-owned web-search, code-execution, or similar tools require an adapter
that can actually invoke the vendor feature and retain its result blocks.

## Direct-call convenience

An `Agent` provider may package this loop as its `DriveProvider` behavior:

```baml
let SupportAgent = ai.Agent {
  inner: ToolModel,
  options: ai.AgentOptions.new(tools = [lookup_order]),
}

let resolution = ResolveTicket(ticket, $provider = SupportAgent)
```

Use the direct form only when non-value outcomes have a documented policy.
Use `run_agent` when the application must distinguish them.

## Related design


- [ToolCallingProvider versus Agent](../specification/05-tools-and-agents.md#toolcallingprovider-versus-agent)
