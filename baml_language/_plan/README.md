# `_plan/` — how this directory works

Working docs for the LLM-provider redesign (`baml.ai`). This README is the entry point: it says
what each file is, which are **living** vs **frozen**, and the workflow for implementing and
checking things off. If you are an AI agent picking up work here, read this first, then follow
the reading order below.

## Reading order (fresh agent)

1. **[`implementation-checklist.md`](./implementation-checklist.md)** — the live state. What's done, what's next, in order. *Start here to pick work.*
2. **[`llm-desugar-capabilities-plan.md`](./llm-desugar-capabilities-plan.md)** — the **current macro-phase** design (client param → `baml.ai.Provider`, `//baml:llm_capability` registry, companion desugaring, `ns_ai_scenarios` reorg, integ testsets). Detailed, user-approved; read the § the checklist item links to.
3. **[`llm-provider-plan.md`](./llm-provider-plan.md)** — the **master design record**: the target model, the blocking design decisions D1–D9 (effect axis, sum outcomes, Failure base, …), prerequisites P1–P8, and the original 6-phase roadmap. Frozen as a decision record — consult it for *why*, don't re-litigate it.
4. **[`deviations.md`](./deviations.md)** — every place the implementation diverged from the plans and why. Append-only log.
5. **[`baml_gotchas.md`](./baml_gotchas.md)** — hard-won language/tooling field notes. **Read before writing stdlib BAML**; append new gotchas as found.
6. The design corpus in [`../llm-provider/`](../llm-provider/):
   - [`ideas/scenarios/_conventions.md`](../llm-provider/ideas/scenarios/_conventions.md) — the model in one screen (real vs invented spellings; companion spine).
   - [`ideas/scenarios/NN-*/`](../llm-provider/ideas/scenarios/) — 47 numbered scenarios (`README` / `implement.baml` / `usage.baml` / `evaluation.md`). Frozen design exploration; the `ns_ai_scenarios/` test tree mirrors this numbering.
   - [`REALIZED.md`](../llm-provider/REALIZED.md) / [`E2E_TESTS.md`](../llm-provider/E2E_TESTS.md) — living indexes of what's built and the verified test surface.

## Living vs frozen

| Living (update as you work) | Frozen (decision records — do not rewrite) |
|---|---|
| `implementation-checklist.md` (check items off, reorder Next-up) | `llm-provider-plan.md` (master design; has a status banner only) |
| `deviations.md` (append divergences) | `../llm-provider/ideas/**` (scenario corpus) |
| `baml_gotchas.md` (append findings) | `llm-desugar-capabilities-plan.md` (current-phase design; edit only on explicit user redirection) |
| `../llm-provider/REALIZED.md`, `E2E_TESTS.md` (extend as coverage lands) | |

## Workflow (per checklist item)

1. **Pick** the topmost unchecked item in the checklist's "Next up"; read its linked design §.
2. **TDD** — failing test first. **Native BAML tests are preferred** over Rust (`test` blocks in
   `crates/baml_tests/baml_src/`; live/API tests in `testset "integ-test"`). Rust only for what
   BAML can't host: wiremock/request-capture, compiler-phase assertions, runner plumbing.
3. **Strict-check stdlib edits** via `baml-cli run --file <trivial>.baml` (catches what
   `baml_test!` downgrades to warnings). Remember: stdlib `.baml` edits need `touch` before
   `cargo build -p baml_cli`.
   **Live/integ tests** need API keys — inject them via
   `infisical run --env=test -- <command>`, e.g.
   `infisical run --env=test -- cargo test -p baml_tests --test ai_anthropic` or
   `infisical run --env=test -- baml test --from crates/baml_tests/baml_src -i "integ-test::"`.
   Without keys the live tests skip silently (that's by design — don't "fix" it).
4. **Run** the affected suites (`cargo test -p baml_tests --test ai_provider …`, `baml_src`)
   and **regen snapshots** where they legitimately changed (`compiles/__baml_std__`, baml_src
   bytecode, LSP listings) — in a dedicated commit if large.
5. **Commit per ✅** and check the item off in `implementation-checklist.md`.
6. **Log** any divergence in `deviations.md`, any new language finding in `baml_gotchas.md`,
   and extend `E2E_TESTS.md`/`REALIZED.md` when the verified surface grows.

## How the plans relate

- `llm-provider-plan.md` (2026-06) designed the whole model and phased it 0–6. Implementation
  then built Phases 0–3 nearly fully and large parts of 4–5 opportunistically (see REALIZED.md),
  but its Phase-1 "client-as-sugar + companion desugar" bullet was deliberately deferred
  (orchestrator delegation shipped instead — see deviations.md).
- `llm-desugar-capabilities-plan.md` (2026-07) is the plan for exactly that deferred core plus
  the open capability registry, the scenario-test reorg, and the integ-test tiers. It
  **supersedes** the master plan's Phase-1 wiring bullets and most of its Part IV (migration),
  and executes its P3.
- `implementation-checklist.md` is the single execution tracker across both.
