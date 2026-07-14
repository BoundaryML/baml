# Multiple and parallel tools

Three ideas are easy to conflate:

1. several tools are available;
2. the model calls tools sequentially across turns; or
3. the model requests several independent calls in one turn.

## Add another tool

```baml
let search_policy = ai.tool(
  "search_policy",
  "Search customer-support policy.",
  (args: SearchPolicyArgs) -> PolicyExcerpt {
    policies.search(args.query)
  },
)

let outcome = ai.drivers.run_agent(
  ResolveTicket.task(ticket, $provider = ToolModel),
  ai.AgentOptions { tools: [lookup_order, search_policy] },
)
```

The provider may choose either tool, both tools in later turns, or multiple
calls in the same turn.

## Parallel dispatch

The driver preserves the provider's call IDs and dispatches one batch. A
parallel dispatcher may use structured concurrency:

```baml
function dispatch_parallel(calls: ai.ToolCall[]) -> ai.ToolResult[] {
  await baml.future.all(calls.map((call) -> {
    spawn { tool_registry.dispatch(call) }
  }))
}
```

Results are correlated by `ToolCall.id`; array position is not the protocol.
Do not treat `Resolution[]` or another list output as an instruction to enable
parallel tool calls. Output cardinality and tool-call concurrency are separate.

## Side effects

Read-only lookups may run concurrently. Two refund operations may require
ordering, idempotency keys, or a transaction. Parallelism is a dispatcher
policy, not something inferred solely from the provider request.

## Related design and scenarios

- Scenario 11 parallel tools
- Scenario 12 tool taxonomy

