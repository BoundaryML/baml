# Call an external coding harness

> **Status:** Implemented through the reference `ModelHarness` adapter.

An external harness such as Claude Code or Pi owns its internal model/tool loop,
workspace interaction, and exact continuation state. BAML supplies the typed
task and consumes its events and terminal value.

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
  ai.HarnessOptions { cwd: "/workspace" },
)

let patch: Patch = run.value
persist_for_review(run.token)
```

`HarnessRun<T>` keeps the typed terminal value, normalized events, current
conversation projection, and opaque resume token together. Interruption is a
separate session control operation; it is not fabricated as a terminal
`Done<T> | BudgetReached | Handoff` union after the fact.

## Why this is not `run_agent`

`run_agent` owns application tool dispatch. A harness already owns its loop and
may have built-in read/edit/bash/search tools, permission prompts, sub-agents,
and provider-native continuation. `submit_harness` delegates that lifecycle
without pretending those tools are local handlers. The task keeps its declared
model provider; the harness adapter decides how that model intent maps into its
own runtime. `CodeModel` therefore remains valid for an ordinary direct
`FixRepository(...)` call.
