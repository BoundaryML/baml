# B-1055: error nodes in graph are formatted incorrectly

Linear: https://linear.app/boundaryml2/issue/B-1055/error-nodes-in-graph-are-formatted-incorrectly

## Status

- Worktree created from `origin/canary` commit `2d8da0cd1b9ec490765bbb29834ddd9b01a6b5ea`.
- Linear report reviewed: an error preview expands vertically and renders a long traceback as unstructured wrapped text, obscuring downstream graph nodes and edges.
- Reproduced locally by serving `tools/sdk-parity-lint/baml_src`, selecting `main`, leaving `repo_root` omitted, and clicking Run.
- Before image: `artifacts/B-1055-before.jpg`.
- Root cause: `CapturedValueCard` renders `value.diagnostic` at unlimited natural height while graph layout reserves only 18px for it, so multiline tracebacks overflow the fixed error node and overlap later nodes and edges.
- Implemented the fix in `typescript2/pkg-playground/src/CapturedValueCard.tsx`: diagnostics now stay on one ellipsized row, matching the graph layout allocation while retaining the full diagnostic in the card tooltip and detailed run panel.
- After image: `artifacts/B-1055-after.jpg`.
- Uploaded the before and after images to Linear for embedding in the PR description.
- Verified the real repro after the change: the traceback is truncated in the graph card and the downstream node and edge no longer overlap it.
- `pnpm format:ci --since=origin/canary --reporter=github` passed.
- `pnpm --filter @b/pkg-playground typecheck` passed.
- `pnpm --filter @b/pkg-playground test` passed: 16 files, 126 tests.
- `pnpm --filter app-vscode-webview build` passed.
- `pnpm --filter app-vscode-webview typecheck` remains blocked by the worktree's absent generated `@b/bridge_wasm` module and its resulting pre-existing implicit-any errors in `src/bridge_wasm.test.ts`.
- PR, CI monitoring, and CodeRabbit approval are pending.
