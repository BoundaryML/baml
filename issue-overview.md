# B-1061: Hide argument placeholders when default args are enabled

Linear: https://linear.app/boundaryml2/issue/B-1061/default-arg-ui-when-default-args-are-checked-the-placeholder-in-the

## Status

- 2026-07-29: Reviewed the bug report: when the playground's default-argument option is checked, argument inputs continue to show placeholder text even though the defaults are supplying the values.
- 2026-07-29: Refreshed the source repository from `origin` and created the isolated Git worktree `/Users/sam/baml-worktrees/B-1061` from the latest `origin/canary` commit `52910c8fbb7b166967f199cb2bb2bc2f06e82f2b` on branch `agent/b-1061-default-arg-placeholder`.
- 2026-07-29: Installed the TypeScript workspace dependencies, built the playground WASM bridge from this worktree, and launched the unmodified Prompt Fiddle at `http://localhost:3000/`.
- 2026-07-29: Reproduced the bug with `function Defaults(name: string = "Ada Lovelace", count: int = 3, ratio: float = 0.5)`: after checking the explicit-value control for `name`, the enabled input had an empty value but retained the `"Ada Lovelace"` placeholder, making the default expression look like the value that would be sent.
- 2026-07-29: Captured the checked-state before image at `typescript2/pkg-playground/docs/B-1061-before.jpg`.
- 2026-07-29: Identified the root cause in `ArgsForm`: `ParamRow` forwards `param.defaultExpression` as the input placeholder in both modes instead of limiting it to the unchecked, use-default state.
- 2026-07-29: Implemented mode-aware placeholders for default-capable parameters: unchecked use-default inputs retain the declared default expression, while checked explicit-value inputs receive an empty placeholder; required parameters continue to use their existing schema-specific placeholders.
- 2026-07-29: Verified both modes in the running Prompt Fiddle: unchecked `name` is disabled by its fieldset and exposes the `"Ada Lovelace"` placeholder, while checked `name` is enabled with an empty value and empty placeholder.
- 2026-07-29: Captured the identically framed after image at `typescript2/pkg-playground/docs/B-1061-after.jpg`; both before and after artifacts are genuine 696x256 JPEG files.
- 2026-07-29: Automated validation passed with `pnpm exec biome check pkg-playground/src/ArgsForm.tsx`, `pnpm --filter @b/pkg-playground typecheck`, `pnpm --filter @b/pkg-playground test` (145 tests), `pnpm --filter app-vscode-webview typecheck`, `pnpm --filter app-promptfiddle typecheck`, `pnpm --filter app-vscode-webview test:unit:run -- ExecutionPanel.strict-mode.test.tsx` (22 tests), and `git diff --check`.
- 2026-07-29: The first webview test run caught that required parameters had also lost their schema-specific placeholders; the condition was narrowed to default-capable parameters and the rerun passed.
- 2026-07-29: PR publication, CI monitoring, and CodeRabbit approval are pending.
