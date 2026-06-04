# BAML Command Additions

This is the short command-surface summary for the release-pipeline plan.

The important split:

- `baml` is the wrapper users run.
- `baml-cli` is the selected language toolchain binary.
- Normal commands like `baml generate`, `baml run`, `baml describe`, `baml pack`, and `baml lsp` are not new wrapper commands. The wrapper resolves a toolchain and forwards them to `baml-cli`.

## Added To `baml`

| Command | Purpose |
|---|---|
| `baml toolchain install <canary\|nightly\|version>` | Download and verify a toolchain without necessarily making it active. Channel inputs resolve to a concrete version at install time. |
| `baml toolchain use <canary\|nightly\|version>` | Select the active default toolchain. Installs the resolved toolchain if missing. |
| `baml toolchain update` | Refresh the active channel and advance to the latest canary/nightly toolchain. Concrete pinned versions do not move. |
| `baml toolchain status` | Check the active selector against the latest remote channel metadata without installing, updating, or changing selection. |
| `baml toolchain list` | Show installed toolchains and the active selector/version. Local-only; never checks remote versions. |
| `baml toolchain uninstall <version>` | Remove an installed concrete toolchain. |
| `baml self-update` | Update only the wrapper for curl-installed wrappers. Refuse for package-manager-managed wrappers with the right package-manager command. |

## Added To `baml-cli`

| Command | Purpose |
|---|---|
| `baml-cli ide install` | Install or update the IDE extension/assets for the selected toolchain. The wrapper exposes this as `baml ide install` by resolving the toolchain and forwarding the command. Common editors should have non-interactive flags such as `--cursor`, and `--code`. |
| `baml-cli agent install [--dir <path>]` | Install or refresh the latest official BAML agent skills in the current project, or in an explicit directory when `--dir` is supplied. The wrapper exposes this as `baml agent install`; the selected toolchain fetches the latest `BoundaryML/baml-skill` content and writes project-local `.agents/skills/baml-*` and `.claude/skills/baml-*` files. |

## Possible `baml-cli` Work Because Of The Wrapper

These are not approved new commands yet. They are implementation pressure points to keep visible.

| Possible Surface | Why It Might Be Needed |
|---|---|
| `baml-cli ide status` or `baml-cli ide doctor` | Only if `baml ide install` needs a clean way to detect installed editor extensions, explain mismatches, or debug user setup. Prefer avoiding this unless the implementation needs it. |
| `baml-cli lsp` compatibility metadata | Not a new command. The LSP reports BAML-specific protocol metadata through the existing LSP `initialize` result, and the playground reports playground protocol metadata when the WebSocket connects. The VSIX uses integer protocol ranges and capability flags so multiple toolchain versions can work without a new extension install per release. This must not require extra VSIX startup commands or network checks. |

## Not Added

| Command | Reason |
|---|---|
| `baml setup-ide` | Removed from the plan. IDE installation belongs to `baml-cli ide install`; users may invoke it through `baml ide install`. |
| `baml install nightly` / `baml install canary` | Avoid spending the top-level `install` verb on toolchains if BAML later grows project packages. Use `baml toolchain ...`. |
| `baml update` as the primary toolchain command | Avoid ambiguity with future package/dependency updates. Use `baml toolchain update`. |

## Reserved For Future Package Management

These are intentionally not part of the toolchain release plan:

| Future Command | Likely Meaning |
|---|---|
| `baml install <package>` | Install a BAML project package/dependency. |
| `baml add <package>` | Add a BAML project package/dependency. |
| `baml remove <package>` | Remove a BAML project package/dependency. |
| `baml update` | Update project dependencies/packages, if that becomes the chosen package-manager shape. |
