# Remove tools between agent steps

> **Status:** Partial — application-tool removal is implemented. The
> provider-owned example is the proposed typed API; the reference currently
> verifies that ownership rule with a deterministic provider.

Tool rosters change at the boundary between provider steps. A request already
in flight keeps the schemas it was sent; `prepare_step` controls the next
request.

## Remove one application tool

Filter the current snapshot and return every tool that should remain:

```baml
class RemoveRefundToolHooks {
  remove_at_step: int,

  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan throws never {
      if (ctx.step == self.remove_at_step) {
        let next_tools = ctx.tools.filter((tool: ai.Tool) -> bool {
          tool.name != "issue_refund"
        })
        return ai.StepPlan { provider: null, tools: next_tools, stop: null }
      }

      // null means keep the current roster unchanged.
      ai.StepPlan { provider: null, tools: null, stop: null }
    }
  }
}
```

The other hook methods use their interface defaults.

The next model request still receives every other application tool. The driver
applies the replacement to its registry, so it remains active on later steps.

## Remove every application tool

An explicit empty replacement means “no application tools,” not “inherit”:

```baml
class DisableAllTools {
  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan throws never {
      if (ctx.step == 2) {
        return ai.StepPlan { provider: null, tools: [], stop: null };
      }
      ai.StepPlan { provider: null, tools: null, stop: null }
    }
  }
}
```

The driver applies that empty roster to its registry. It remains empty until a
later step returns another complete replacement:

```text
tools: null  -> keep the active application roster
tools: []    -> replace it with zero application tools
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
  final_answer_step: int,

  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan throws never {
      if (ctx.step == self.final_answer_step) {
        return ai.StepPlan {
          provider: FinalAnswerModel, // provider-owned tools are empty
          tools: [],                  // application tools are empty
          stop: null,
        };
      }

      ai.StepPlan { provider: null, tools: null, stop: null }
    }
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

## What to test

Cover these four cases in your application tests:

1. return a complete roster without one application tool;
2. use `tools: []` to remove every application tool;
3. verify a replacement persists and updates an attached registry; and
4. switch between same-name provider configurations while clearing both
   application and provider-owned tools.
