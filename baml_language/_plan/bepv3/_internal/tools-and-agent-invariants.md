# Tools and Agent invariants

Application tools are ordinary BAML functions. The runtime normalizes each
plain function or configured `ai.Tool` into tool metadata backed by
`baml.AnyFunction`.

## Normalizing a function into a tool

For each function in an LLM declaration's `tools:` list, the compiler records:

- semantic function identity;
- public name and documentation;
- input schema;
- output schema;
- declared throws set;
- default parameter expressions;
- an invocation handle; and
- trace attribution.

The source:

```baml
tools: [
  lookup_order,
  search_policy,
]
```

first performs an ordinary function-to-interface coercion:

```baml
let functions: baml.AnyFunction[] = [
  lookup_order,
  search_policy,
]
```

It then derives a normalized `ai.Tool` for each function:

```baml
let tools: ai.Tool[] = functions.map((handler) -> ai.Tool {
  ai.tool(handler)
})
```

`ai.tool(handler)` uses function reflection to derive the name, documentation,
input schema, output schema, throws set, and default-argument behavior.
Configured forms such as `ai.tool(handler).as_handoff()` change policy without
adding a second dispatcher.

This is function erasure, not result erasure. The task remains `Task<T, P>`.

## Arguments and defaults

When a model omits an optional parameter with a BAML default, BAML evaluates
the default before invoking the function. Provider JSON-schema limitations do
not redefine BAML call semantics.

The Agent validates and normalizes arguments in this order:

1. identify the registered function;
2. parse provider arguments as JSON;
3. validate known fields;
4. apply BAML default arguments;
5. construct typed BAML values; and
6. invoke the function.

Unknown fields, malformed JSON, and missing required fields produce a typed
invalid-arguments event. They do not invoke the function.

## Tool registry

Without an explicit registry, the Agent creates one from the task's effective
application tools. Advanced code may supply an authoritative mutable registry
for dynamic discovery:

```baml
let registry = ai.ToolRegistry.new([]);
registry.add(discovery.add_server)
```

Registry invariants:

- function identity is canonical;
- externally visible tool names are unique at each step;
- adding an already-present function is idempotent;
- removing a function prevents future calls but does not rewrite history;
- the Agent snapshots the offered tool schema per provider turn; and
- a tool call is resolved against the snapshot that produced it.

`tools` and `tool_registry` are two ways to choose one roster:

- with no registry, `tools = null` inherits the task roster;
- with no registry, a `tools` list replaces the task roster;
- with a registry, the registry is authoritative and `tools` must be `null`.

Supplying both a registry and a non-null `tools` list is a configuration error.
Identical function identity is idempotent; a name collision with a different
handler fails.

The final rule avoids races when hooks add or remove tools between a model step
and dispatch.

## Agent loop

Conceptually:

```baml
function Agent.run<T, P>(
  self,
  task: ai.Task<T, P>,
) -> ai.AgentOutcome<T>
  where P: ai.ToolCallingProvider
{
  let state = initialize(task, self)
  state.conversation = self.conversation ?? state.provider.begin(task)

  while (true) {
    let offered_tools = state.registry.snapshot()
    let step = state.provider.step(
      state.conversation,
      offered_tools,
    )

    match (step.outcome) {
      let value: T => {
        return ai.Done {
          value: value,
          conversation: state.conversation,
          meta: agent_meta(state),
        }
      },

      let calls: ai.ToolCalls => {
        match (handoff_requested(calls, offered_tools)) {
          let handoff: ai.Handoff => return handoff,
          null => {},
        }

        let results = dispatch_with_hooks(
          calls,
          offered_tools,
          self.hooks,
        )
        state.conversation = state.provider.submit(
          state.conversation,
          results,
        )
      },
    }

    if (self.limit_reached(state)) {
      return ai.BudgetReached.from(state)
    }
  }
}
```

The real implementation may run independent tool calls concurrently. It must
preserve a deterministic result order keyed by provider call ID.

## Hooks

Hooks may:

- approve, deny, or rewrite pending tool calls;
- inspect results;
- add or remove future tools;
- request a handoff; and
- add trace annotations.

Hooks must not:

- forge a provider conversation owned by a different provider;
- alter a completed historical tool result;
- bypass argument validation;
- cause a denied call to execute; or
- make a side effect replayable after it has started.

Hooks receive typed context. A plain string callback name is not the dispatch
mechanism.

## Tool failures

Tool execution has three important failure phases:

| Phase | Side effect may have started? | Default retry |
| --- | --- | --- |
| Argument validation | No | May ask model to repair |
| Function invocation started | Yes or unknown | Do not replay |
| Function returned a declared error | Function-specific | Follow policy |

The Agent converts safe-to-report tool failures into provider tool results
when policy permits. Fatal runtime failures remain BAML errors.

## MCP discovery

MCP tools are normalized to the same `baml.AnyFunction`-backed registry
entries as BAML functions. Discovery adds functions; it does not add a second
dispatch system.

An MCP server resource owns transport and cleanup. Removing its functions from
a registry does not itself close the server resource.
