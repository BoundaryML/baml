# B-1055: error nodes in graph are formatted incorrectly

Linear: https://linear.app/boundaryml2/issue/B-1055/error-nodes-in-graph-are-formatted-incorrectly

## Status

- Worktree created from `origin/canary` commit `2d8da0cd1b9ec490765bbb29834ddd9b01a6b5ea`.
- Linear report reviewed: an error preview expands vertically and renders a long traceback as unstructured wrapped text, obscuring downstream graph nodes and edges.
- Reproduced locally by serving `tools/sdk-parity-lint/baml_src`, selecting `main`, leaving `repo_root` omitted, and clicking Run.
- Before image: `artifacts/B-1055-before.jpg`.
- Root cause: `CapturedValueCard` renders `value.diagnostic` at unlimited natural height while graph layout reserves only 18px for it, so multiline tracebacks overflow the fixed error node and overlap later nodes and edges.
- The first fix constrained graph diagnostics to one ellipsized row; user review rejected that design because the traceback and structured values need to remain readable in the graph.
- Revised graph cards to preserve each traceback line, render inputs and typed errors as expanded pretty JSON with separate key rows, and allocate graph height for every diagnostic line while leaving the detailed execution panel unchanged.
- After image: `artifacts/B-1055-after.png`.
- Uploaded the before and after images to Linear for embedding in the PR description.
- Verified the real repro after the revision: input and `InvalidArgument` keys render on separate rows, every logical traceback line starts on its own row, long source lines wrap within the error card, and the graph layout reserves the resulting card height without overlap.
- `pnpm format:ci --since=origin/canary --reporter=github` passed.
- `pnpm --filter @b/pkg-playground typecheck` passed.
- `pnpm --filter @b/pkg-playground test` passed after rebasing onto current `origin/canary`: 20 files, 138 tests.
- `pnpm --filter app-vscode-webview build` passed.
- `pnpm --filter app-vscode-webview typecheck` remains blocked by the worktree's absent generated `@b/bridge_wasm` module and its resulting pre-existing implicit-any errors in `src/bridge_wasm.test.ts`.
- Opened ready-for-review PR https://github.com/BoundaryML/baml/pull/4271 against `canary` with before/after playground images in the description.
- Non-Vercel CI passed, including Biome, generated-artifact typecheck, jsdom unit tests, and Playwright browser tests.
- CodeRabbit initially deferred review because of its temporary review limit; an explicit review will be requested when the stated cooldown expires.
- Addressed automated review feedback by keeping graph-only presentation options out of the shared detailed execution panel.
- CodeRabbit approval is pending; Vercel checks are intentionally ignored.
