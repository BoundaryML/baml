# Remove tools between agent steps

Tool rosters change at the boundary between provider steps. A request already
in flight keeps the schemas it was sent; `prepare_step` controls the next
request.

## Remove one application tool

Use a `ToolRegistry` when the change should persist for later steps:

```baml
class RemoveRefundToolHooks {
  // ...permission policy...

  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan throws never {
      if (ctx.step == 2 && refund_permission_expired(ctx)) {
        ctx.tool_registry.remove("issue_refund")
      }

      // null means use the registry's current snapshot.
      ai.StepPlan { provider: null, tools: null, stop: null }
    }

    // ...other AgentHooks methods use their defaults...
  }
}
```

The next model request still receives every other registered application tool.

## Remove every application tool

An explicit empty replacement means “no application tools,” not “inherit”:

```baml
class DisableToolsForOneTurn {
  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan throws never {
      if (ctx.step == 2) {
        return ai.StepPlan { provider: null, tools: [], stop: null };
      }
      ai.StepPlan { provider: null, tools: null, stop: null }
    }

    // ...other AgentHooks methods use their defaults...
  }
}
```

That replacement applies to the next turn. If a registry is attached and the
removal should persist, clear the source instead:

```baml
class ClearToolsPermanently {
  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan throws never {
      if (ctx.step == 2) {
        ctx.tool_registry.clear()
      }
      ai.StepPlan { provider: null, tools: null, stop: null }
    }

    // ...other AgentHooks methods use their defaults...
  }
}
```

The distinction is intentional:

```text
tools: null  -> inherit or keep the active application roster
tools: []    -> advertise zero application tools for the next turn
```

## Remove provider-owned tools too

Provider-owned tools are vendor features such as hosted web search, code
execution, or retrieval. The provider executes them, so they are typed fields
on the provider configuration rather than application `Tool` handlers.

Create a provider variant with those tools disabled:

```baml
let ResearchModel = ai.OpenAi {
  model: "gpt-5",
  built_in_tools: [
    ai.openai.WebSearch { search_context_size: "low" },
  ],
}

let FinalAnswerModel = ai.OpenAi {
  ...ResearchModel,
  built_in_tools: [],
}
```

Then clear both authority domains in one step plan:

```baml
class FinalAnswerHooks {
  // ...policy deciding when research must stop...

  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan throws never {
      if (ctx.step == 4) {
        return ai.StepPlan {
          provider: FinalAnswerModel, // provider-owned tools are empty
          tools: [],                  // application tools are empty
          stop: null,
        };
      }

      ai.StepPlan { provider: null, tools: null, stop: null }
    }

    // ...other AgentHooks methods use their defaults...
  }
}
```

`ResearchModel` and `FinalAnswerModel` may have the same provider family,
model, display name, credentials, and endpoint. A non-null `provider` is still
an explicit switch command because the configurations grant different tool
authority. Drivers must not compare display names to decide whether to skip
it.

Provider switching uses transcript export/import as usual. If a provider can
change its built-in-tool configuration on the same native transcript without
losing state, its adapter may preserve exact fidelity; otherwise the reported
conversion fidelity makes the loss visible.

## Why not one global `tools: []`?

A single untyped kill switch would flatten two different execution owners:

```text
application Tool       -> application driver validates and executes it
provider-owned tool    -> provider validates and executes it
```

Keeping them separate means a hook cannot accidentally disable provider
security/retrieval policy while intending only to prune application actions.
The “disable everything” operation remains concise, but it is explicit about
both owners.

## Executable scenarios

The reference package covers four cases in
`ns_ai_scenarios/02_tools_and_agents/10_remove_tools.baml`:

1. remove one application tool from a registry;
2. use `tools: []` to remove every application tool;
3. clear a registry persistently; and
4. switch between same-name provider configurations while clearing both
   application and provider-owned tools.
