# B-1056: monaco dropdown has bad spacing

Linear: https://linear.app/boundaryml2/issue/B-1056/monaco-dropdown-has-bad-spacing

## Status

- 2026-07-28: Created isolated Git worktree from the latest `origin/canary` commit `3f82a26f8d5361f531e2e8e8fc56838fe3ba8d6b`.
- 2026-07-28: Reviewed the bug report: Monaco Explorer tree twisties for `sdk-parity-lint` and `baml_src` overlap adjacent text instead of maintaining normal spacing.
- 2026-07-28: Reproduced the bug in the unmodified Prompt Fiddle at `http://localhost:3000/`: every Explorer tree twistie overlaps its adjacent label by 3px. The root `workspace` twistie rendered with a 16px outer width despite its 16px declared width plus 6px right padding, and its transformed right edge landed at x=67 while the label began at x=64.
- 2026-07-28: Captured the before image at `typescript2/pkg-editor/docs/B-1056-before.png`.
- 2026-07-28: Identified the root cause: Prompt Fiddle globally applies `box-sizing: border-box`, while the upstream VS Code tree stylesheet relies on the default `content-box` sizing for `.monaco-tl-twistie`. This causes the 6px spacing padding to consume the declared width instead of extending it.
- 2026-07-28: Implemented a scoped `content-box` override for Monaco tree twisties inside `#workbench-container`, restoring the upstream width and padding contract without changing the Prompt Fiddle shell.
- 2026-07-28: Verified the fix in the running Prompt Fiddle: the twistie computed style is now `content-box`, the Explorer labels shift right to preserve the intended padding, and `workspace`/`baml_src` no longer visually collide with their expanded-state arrows.
- 2026-07-28: Captured the identically framed after image at `typescript2/pkg-editor/docs/B-1056-after.png`; both before and after artifacts are genuine 300x210 PNG files.
- 2026-07-28: Automated validation passed with `pnpm --dir typescript2 --filter app-promptfiddle typecheck`, `pnpm --dir typescript2 --filter app-promptfiddle build`, and `pnpm --dir typescript2 exec biome check pkg-editor/src/views-workbench.css` from the repository root.
- 2026-07-28: The narrower `pnpm --filter @b/pkg-editor typecheck` entry point cannot run in the clean workspace because that package's TypeScript configuration requests the undeclared `@types/node`; the Prompt Fiddle typecheck and production build both compile the shared editor successfully.
- 2026-07-28: Committed the fix as `ed08692198cae2c729d024477fab2d12cdfbae5e` and pushed branch `sam/b-1056`.
- 2026-07-28: Opened ready-for-review PR https://github.com/BoundaryML/baml/pull/4272 against `canary`; its description embeds the durable before and after PNGs from the branch.
- 2026-07-28: CodeRabbit requested consistent Explorer terminology and an explicit working directory for the Biome validation command; both documentation quick wins are addressed.
- 2026-07-28: CI monitoring and CodeRabbit re-approval are pending.
