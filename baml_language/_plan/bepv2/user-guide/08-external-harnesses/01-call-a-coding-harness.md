# Call an external coding harness

An external harness such as Claude Code or Pi owns its internal model/tool loop,
workspace interaction, and exact continuation state. BAML supplies the typed
task and consumes its events and terminal value.

> **Design status:** scenarios 37–42 prove this reference surface. The compact
> harness signatures still need promotion into the normative-signatures page.

## Declare the typed task

```baml
class Patch {
  files_changed: string[],
  summary: string,
}

function FixRepository(issue: string) -> Patch {
  provider: CodeModel
  prompt: `Inspect the repository and fix: ${issue}. ${ctx.output_format}`
}
```

## Submit it to the harness

```baml
let run = ai.drivers.submit_harness(
  CodeHarness,
  FixRepository.task("repair the flaky auth test"),
  ai.SessionOptions { cwd: "/workspace" },
)

match (run.outcome()) {
  let done: ai.Done<Patch> => done.value,
  let stopped: ai.BudgetReached => preserve_for_review(run.token()),
  let handoff: ai.Handoff => route_handoff(handoff),
}
```

## Why this is not `run_agent`

`run_agent` owns application tool dispatch. A harness already owns its loop and
may have built-in read/edit/bash/search tools, permission prompts, sub-agents,
and provider-native continuation. `submit_harness` delegates that lifecycle
without pretending those tools are local handlers. The driver rebinds and
re-renders the task for `CodeHarness`; the declared `CodeModel` remains the
valid default for an ordinary direct `FixRepository(...)` call.

## Related design and scenarios

- Scenarios 37 harness basics, 42 harness abstraction
