# Add one application tool

> **Status:** Implemented behavior. The `ai.tool(...)` shorthand is the final
> API proposed on top of BEP-062 function values.

`ResolveTicket` can classify a request without tools. Give the model a tool
when it needs application data that is not in the prompt.

## Define a typed handler

```baml
class LookupOrderArgs {
  customer_id: string,
  order_id: string,
}

let lookup_order = ai.tool(
  "lookup_order",
  "Look up an order owned by this customer.",
  (args: LookupOrderArgs) -> Order {
    orders.lookup(args.customer_id, args.order_id)
  },
)
```

The argument type becomes the provider-specific tool schema. The handler
receives validated `LookupOrderArgs`, not arbitrary model text.

## Use it

```baml
let task = ResolveTicket.task(ticket, $provider = ToolModel)

let run = ai.drivers.run_agent(
  task,
  ai.AgentOptions.new(tools = [lookup_order]),
)
```

## What changed

```diff
- let resolution = ResolveTicket(ticket)
+ let task = ResolveTicket.task(ticket, $provider = ToolModel)
+ let run = ai.drivers.run_agent(
+   task,
+   ai.AgentOptions.new(tools = [lookup_order]),
+ )
```

The task still declares `Resolution`. `run_agent` owns the intermediate tool
call, dispatch, tool result, and next provider turn.

## Failure behavior

Invalid arguments become a correlated tool error result so the model may
repair them. Authentication failures and business-policy denials should not be
misreported as schema errors.

## Related design


- [Tools and their owners](../specification/05-tools-and-agents.md#tools-and-their-owners)
