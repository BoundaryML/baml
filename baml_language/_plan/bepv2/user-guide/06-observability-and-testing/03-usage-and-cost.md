# Track usage and cost

Usage belongs to each provider attempt. Agent-level totals aggregate attempts
and turns without replacing the underlying records.

## One response

```baml
let response = ai.drivers.drive_with_meta(ResolveTicket.task(ticket))

match (response.meta.usage) {
  let usage: ai.Usage => {
    meter.add_tokens(usage.input_tokens, usage.output_tokens)
  },
  null => log.debug("provider did not report token usage"),
}
```

## Agent budget

```baml
let run = ai.drivers.run_agent(
  ResolveTicket.task(ticket, $provider = ToolModel),
  ai.AgentOptions {
    budget: ai.Budget { max_steps: 10, max_cost_usd: 0.30 },
    tools: [lookup_order, search_policy],
  },
)
```

Usage updates should be emitted as events as soon as they are known. A final
aggregate is useful, but cannot reconstruct failed retry attempts unless those
attempts were traced.

## Provider differences

Token categories and cached/reasoning tokens may differ by provider. Keep the
portable `Usage` minimum stable and expose provider-specific categories in
typed metadata or attributes.

## Related design and scenarios

- Scenarios 32 observability, 34 cost and tokens

