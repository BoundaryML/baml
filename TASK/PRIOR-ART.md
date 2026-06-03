# Prior Art For BAML Release And Toolchain UX

Research date: 2026-05-29.

## Executive Summary

The current architecture is directionally sound: a small `baml` wrapper, versioned toolchains under `~/.baml`, project pins in `baml.toml`, normal commands forwarded to the concrete CLI, and Python runtime packages controlled by Python package managers.

The main correction is command surface. Prior art strongly suggests that top-level `install` and `update` should not mean "select a language toolchain" if BAML expects to grow a package/dependency manager. Rust avoids this by splitting `rustup toolchain install` from `cargo install` and `cargo update`. Python/uv separates `uv self update`, `uv python install`, `uv tool install`, `uv add`, and `uv sync`. Go makes toolchain updates a module/toolchain concern, while `go install pkg@version` installs executables. Bun and Deno show why single-binary tools must reserve words carefully: `bun install` and `bun update` are package manager verbs, while `bun upgrade` updates Bun itself; `deno install` installs a script/app while `deno upgrade` updates the runtime.

Recommended BAML shape:

```bash
baml toolchain install canary
baml toolchain install nightly
baml toolchain install 0.11.0
baml toolchain use canary
baml toolchain use nightly
baml toolchain use 0.11.0
baml toolchain update
baml toolchain list
baml toolchain uninstall 0.11.0
baml self-update
baml ide install --cursor
```

Top-level `baml install` is reserved for future BAML project packages/dependencies. Top-level `baml update` is also reserved until package-management semantics exist. The documented toolchain update command is `baml toolchain update`.

Normal commands such as `baml generate`, `baml run`, `baml describe`, `baml pack`, and `baml lsp` should remain pure pass-throughs and should not block on network after a suitable toolchain is installed.

## Proposed BAML Command Surface

| User intent | Recommended command | Semantics |
|---|---|---|
| Install/select canary toolchain globally | `baml toolchain use canary` | Resolves current canary, installs if missing, records active channel as `canary`. |
| Subscribe to nightly globally | `baml toolchain use nightly` | Resolves current nightly, installs if missing, records active channel as `nightly`. |
| Install but do not activate a version/channel | `baml toolchain install 0.11.0` / `canary` / `nightly` | Downloads and verifies a concrete toolchain. For a channel, resolves to the current concrete version. |
| Pin a project | `baml toolchain pin 0.11.0` or edit `baml.toml` | Writes `[toolchain] version = "0.11.0"` to the nearest project. Optional `pin nightly` writes channel if the team wants project-level channel following. |
| Update active channel | `baml toolchain update` | If active selector is `canary` or `nightly`, refresh channel manifest and advance to current head. If active selector is a concrete version, say it is pinned and suggest `baml toolchain use canary/nightly`. |
| Update wrapper itself | `baml self-update` | Replaces only the wrapper for curl/script installs. Refuses for Homebrew/AUR/uv/pipx-managed installs with manager-specific instructions. |
| Install IDE assets | `baml ide install` | User-facing command for IDE setup/update. It is forwarded by the wrapper to the selected toolchain payload. Common editors should support flags such as `--cursor`, `--code`, `--windsurf`, and `--all` to avoid an interactive prompt. |
| Future project package install | `baml install <package>` | Reserved for BAML packages/dependencies; do not spend this verb on toolchains. |
| Future project package update | `baml update` or `baml package update` | Reserve until package manager design is concrete. If ambiguous, prefer `baml package update`. |

Why `toolchain` over `version`: `version` reads like information display or project-version bumping in several ecosystems (`uv version` reads/updates the project version). `toolchain` is explicit, matches Rust and Go vocabulary, and makes future package-manager verbs unambiguous.

Compatibility option:

```bash
baml version use nightly
baml version update
```

This is acceptable only as an alias if the team strongly prefers "version" wording in docs. The primary help text should use `toolchain` to avoid confusing toolchain selection with package or project version changes.

## Prior-Art Matrix

| Ecosystem | Tool self-update | Runtime/toolchain update | Project dependency update | Project pin | Channel model | Offline/cache behavior | IDE/LSP pattern | BAML implication |
|---|---|---|---|---|---|---|---|---|
| Rust | `rustup self update`; `rustup update` also checks self-update in rustup-managed installs | `rustup toolchain install`, `rustup update`, `rustup default` | `cargo update` updates `Cargo.lock`; `cargo install` installs executables | `rust-toolchain.toml` plus command/env/directory overrides | `stable`, `beta`, `nightly`, dated nightlies | Cargo has `--offline`; rustup installs/updates explicitly sync manifests | `rust-analyzer` is separate from compiler/cargo but usually finds project toolchain | Strongest model: namespace toolchains away from package manager verbs. |
| Bun | `bun upgrade`; Homebrew users are told to use `brew upgrade bun` | Single binary, so upgrade replaces runtime/PM/test runner together | `bun install`, `bun add`, `bun update` | `package.json` + `bun.lock` | `bun upgrade --canary`, `--stable` | Global cache, frozen lockfile, dependency lifecycle restrictions | No separate ecosystem LSP model central to Bun | Do not use `install`/`update` for BAML toolchains if future packages exist. |
| mise/asdf | `mise self-update` unavailable when package-manager-installed; asdf says update with original install method | `mise install`, `mise use`; `asdf install`, `asdf set` | Delegated to backend tools | `mise.toml`, `.tool-versions`, global config | Uses exact, fuzzy, `latest`, backend-specific channels | Installs into local data dir; normal shim execution resolves local installs | Shims are common for IDEs/CI | `install` is safe only because mise is explicitly a tool manager, not BAML's language/package CLI. |
| Node/npm/Corepack/Volta | npm can update npm; Volta has its own installer; Corepack manages package-manager shims | nvm/Volta install Node versions; Corepack prepares package managers | `npm install`, `npm update`, `pnpm update`, `yarn add` | `package.json` `packageManager`, Volta section, lockfiles | Node has LTS/current; package managers have dist-tags | npm/npx cache; prompts before ephemeral installs | Language service delegated to editor/extensions | Node shows user confusion when runtime managers and package managers share vocabulary. |
| Python/uv/pipx | `uv self update`; pipx itself updated by pipx/pip/brew/etc. | `uv python install/upgrade`; pyenv for Python versions | `uv add`, `uv sync`, `uv lock`, `pip install`, `pip install -U` | `pyproject.toml`, lockfiles, `.python-version` | Pre/dev releases opt-in; exact versions common | uv aggressively caches; `uvx` env is disposable cache, `uv tool install` env is persistent | Python extension bundles/delegates servers; tools exposed through console scripts | PyPI `baml` command can be a launcher, but `import baml_core` must remain the installed wheel. No wheel self-mutation. |
| Go | Install Go by package/installer; no in-command self-update norm | Go 1.21+ `go` can auto-switch/download toolchain based on `go.mod`/`go.work`; `go get go@latest` updates toolchain dependency | `go get` changes module requirements; `go mod tidy`; `go install pkg@version` installs executables | `go` and `toolchain` lines in `go.mod`/`go.work` | Stable/release-candidate toolchain names | Module cache + `go.sum` verification; vendor mode avoids network | `gopls` is official language server installed as a Go tool | Toolchain mismatch should be explicit and actionable; automatic downloads are acceptable only in commands that opted into mutation. |
| Deno | `deno upgrade` updates executable; supports version/channel/nightly and checksum | Single runtime binary; `upgrade` is runtime-level | `deno add`, `deno update`; `deno install` installs script/app binaries | `deno.json`, lockfile | `deno upgrade nightly`, alpha/beta/rc | Cached downloads, lockfile integrity, checksum flag | `deno lsp` is built into the runtime; VS Code extension delegates to Deno | Use `upgrade`/`self-update` for wrapper, not `update`; keep checksums first-class. |
| Homebrew/AUR | Package manager owns wrapper updates | Formula/pkg upgrades should not silently mutate user state beyond package files | Package manager is external | Formula/pkgbuild version and checksum | Stable formula versions; AUR can offer `-bin` or VCS variants | Bottles/source archives have checksums; post-install caveats are normal | Caveats can tell users how to run IDE setup | Managed wrappers must refuse `self-update` and point at manager commands. |
| VS Code extensions | Marketplace owns extension updates | Extension may bundle, download, or launch local server | N/A | User/workspace settings | Extension version channel via marketplace/prerelease | Extension host should avoid unexpected heavy work on activation | LSP runs as a separate process; VSIX can be installed from file | Prefer VSIX as a thin client launching local `baml`/toolchain, with clear mismatch diagnostics. |
| GitHub Releases | N/A | Hosts versioned assets and release notes | N/A | Tags/releases | Canary and nightly/prerelease tags possible | Assets are durable URLs; not a directory/index by themselves | N/A | Use GitHub Releases as immutable storage/changelog, while `pkg.boundaryml.com` is the machine-facing directory. |

## Detailed Findings

### Rust

Rust has the cleanest split for BAML's desired architecture:

- `rustup` manages Rust toolchains and channels.
- `cargo` manages project dependencies, lockfiles, package publishing, executable installs, and builds.
- `rust-toolchain.toml` pins a project toolchain in source control.
- `cargo +nightly test` is an explicit one-command override.

`rustup` resolves toolchains by a documented precedence: command shorthand, environment variable, directory override, `rust-toolchain.toml`, then default. That maps directly onto BAML's proposed precedence of `BAML_VERSION`, nearest `baml.toml`, user config, then canary.

Rust's important naming lesson is that `install` is not globally overloaded. The full commands are `rustup toolchain install nightly` and `cargo install ripgrep`. Users learn which product owns which surface.

BAML decision:

- Use a namespaced toolchain command, ideally `baml toolchain install/use/update`.
- Keep `baml install` free for packages.
- Make `baml.toml [toolchain]` source-control-friendly and explicit, like `rust-toolchain.toml`.
- Add a command override if needed later, e.g. `BAML_VERSION=nightly baml generate` before inventing `baml +nightly generate`.

### Bun

Bun is a counterexample because it intentionally combines runtime, package manager, test runner, bundler, and installer in one binary. Its command surface is correspondingly careful:

- `bun install` installs project dependencies and writes `bun.lock`.
- `bun update` updates dependencies.
- `bun add` changes dependencies.
- `bun upgrade` updates the Bun binary itself.
- `bunx` runs package executables, checking local packages before falling back to package install/cache.
- For Homebrew/Scoop installs, Bun tells users to use the package manager instead of self-upgrading.

Bun also has nightly via `bun upgrade --nightly`, but that changes the single Bun binary. BAML has two products, so copying Bun literally would be wrong.

BAML decision:

- Do not use `baml install nightly` if `baml install <package>` is likely later.
- Do not use `baml update` for toolchains without a namespace unless the team intentionally accepts future ambiguity.
- Use `baml self-update` only for the wrapper, and refuse under managed installs.

### mise / asdf

mise and asdf are useful because they are pure tool managers:

- `mise install node@20` installs a tool version but does not necessarily activate it.
- `mise use node@20` installs and records activation in `mise.toml` or global config.
- `mise self-update` updates mise itself and is unavailable when installed by a package manager.
- asdf separates `asdf install <tool> <version>` from `asdf set <tool> <version>`, with `.tool-versions` as the project pin.

This supports a BAML distinction between "download a concrete toolchain" and "select a default channel/version."

BAML decision:

- `baml toolchain install <selector>` should install but not necessarily select.
- `baml toolchain use <selector>` should install if missing and record the selected selector.
- For channels, store both the selector (`nightly`) and the resolved version (`0.11.1-nightly.20260529.a`) so offline commands know what to run and status commands can explain drift.

### Node

Node's ecosystem splits responsibility in practice, but users often experience it as one large surface:

- nvm/Volta install and select Node versions.
- npm/pnpm/yarn install and update packages.
- npx/npm exec and pnpm dlx run package binaries from local dependencies or cache.
- Corepack provides package-manager shims keyed by project metadata.
- Volta pins runtime and package-manager versions in `package.json`.

The warning sign is user confusion around "install node", "npm install", "npm update", "npx", package manager versions, and project lockfiles. BAML can avoid that by not making the same verb mean toolchain today and packages tomorrow.

BAML decision:

- Avoid `baml version` as the primary namespace because `version` can also mean "project/package version" in tools like uv and npm.
- Prefer `baml toolchain ...` for the language payload and reserve `baml package ...` or top-level package verbs for dependency management.

### Python / uv / pipx

Python is directly relevant to `uvx baml generate`, `uv run baml generate`, and `import baml_core`.

Important norms:

- Installed distributions expose commands through `console_scripts`; installers create wrappers in the environment's script directory.
- `uvx` is `uv tool run`: it runs a Python package command in an isolated cached environment. `uv tool install` creates a persistent isolated environment. `uv run` runs inside a project environment and may sync/update that environment.
- `uvx` may use cached tool environments, but those environments are disposable and should not be manually mutated.
- `pipx` installs Python app packages into isolated virtual environments and exposes their apps on PATH; `pipx run` uses temporary environments.
- PEP 440 developmental releases are `.devN`, are treated as pre-releases, and SemVer hyphen prereleases cannot be used as public PyPI versions without translation.

BAML decision:

- Publish separate conceptual packages even if distribution names need compatibility:
  - `baml_core`: importable runtime library. It must never self-update or install toolchains as an import side effect.
  - `baml`: console-script launcher package. It may provide `baml = baml_launcher:main`, resolve BAML toolchains, and run CLI commands.
- If packaging constraints require one PyPI project for both, still treat imports and CLI as separate surfaces: importing `baml_core` only imports the installed wheel; invoking `baml` can launch/download CLI toolchains only for explicit CLI commands.
- `uvx baml generate` should run the latest PyPI `baml` launcher chosen by uv unless pinned (`uvx baml==0.10.0 generate`). The launcher may download a BAML toolchain into `~/.baml`, but it must not mutate the Python environment or upgrade `baml_core`.
- `uv run baml generate` should use the project environment's installed `baml` console script. If the project does not include it, uv's normal rules apply; BAML should not bypass uv's environment semantics.
- If `pip install baml==0.10.0` but `baml.toml` expects `0.9.x`, the launcher should use the project/toolchain selector and warn only if the launcher protocol is too old to understand the manifest. It should not rewrite the Python package.
- Generated code should embed the generator/toolchain version and the expected `baml_core` compatibility range. On import or first runtime call, if installed `baml_core` is incompatible, raise a loud diagnostic with exact installed, generated, and expected versions plus package-manager commands such as `uv add baml_core==...`, `pip install baml_core==...`, or `uv sync`.

This is the main fix for the "current version conflict class": do not silently synchronize Python packages and CLI toolchains. Detect and explain.

### Go

Go 1.21+ treats the Go toolchain like a dependency of the module/workspace:

- `go` and `toolchain` lines in `go.mod`/`go.work` guide toolchain selection.
- The `go` command may find a suitable toolchain on PATH or download/cache one.
- If automatic switching is disabled, Go refuses to run when the project requires a newer version.
- `go get go@latest` updates the module's toolchain requirement.
- `go install pkg@version` installs executable packages without modifying the current module.
- Module downloads are verified through `go.sum` and the checksum database for public modules.

BAML decision:

- Project pinning belongs in `baml.toml`, but commands that mutate that pin should be explicit (`baml toolchain pin`, not `baml generate`).
- A normal `baml generate` may auto-install a missing exact pinned toolchain only if the team accepts Go-like auto-download. If implemented, it must print that it is downloading because the project requires that version. If the team wants stricter behavior, fail and suggest `baml toolchain install`.
- If a concrete pinned version is unavailable offline, fail with a local-cache message; do not silently fall forward to canary/nightly.

### Deno

Deno's single-binary model uses:

- `deno upgrade` for updating the runtime executable, including specific versions and channels such as nightly.
- `deno install` for installing scripts/app commands.
- `deno update` for dependencies.
- cached runtime downloads and checksum verification.
- a built-in `deno lsp` rather than a separate package-managed server.

BAML decision:

- `upgrade`/`self-update` should mean the installed wrapper, not the active language toolchain.
- `install` should be left available for future installable BAML artifacts/packages, matching Deno's use of `deno install` for user apps, not runtime channels.
- Per-version toolchain archives should have SHA-256 in manifests from day one.

### Homebrew / AUR

Package-manager norms:

- Homebrew formulae install into a managed Cellar and symlink into the prefix. Formulae are versioned package definitions with checksums.
- Homebrew caveats are a standard way to show post-install instructions.
- Homebrew package upgrades should be controlled by `brew upgrade`; tools installed by Homebrew should avoid replacing their own binaries.
- Homebrew generally avoids repackaging language-specific dependencies when those ecosystems already manage them directly.
- Arch/AUR packages are built from PKGBUILD metadata and should install files owned by the package manager, not surprise-mutate user state during every upgrade.
- PKGBUILD sources and checksums are part of the package recipe; VCS-style packages have separate naming/versioning conventions. That supports separate `baml` and `baml-bin` package templates if BAML wants source-built and binary-wrapper variants.

BAML decision:

- Homebrew/AUR should package only `baml-wrapper`.
- First install may bootstrap canary toolchain if implemented as an explicit, idempotent wrapper-owned step, but package upgrades must not reset channel/config or reinstall user-scoped toolchains.
- Prefer caveats that say:

```text
BAML installed the wrapper. To install or update the language toolchain:
  baml toolchain use canary
  baml toolchain update

To install editor support:
  baml ide install --cursor
```

- If managed wrapper is too old:

```text
This baml wrapper is too old to read manifest schema v2.
It was installed by Homebrew. Update it with:
  brew upgrade baml
Then rerun:
  baml toolchain update
```

Use equivalent messages for AUR, uv tool, pipx, and curl installs.

### VS Code Extensions And LSP

VS Code's LSP model is a client extension launching a language server in a separate process. The language server can be implemented in any language. Extensions commonly choose among:

- bundle a server,
- download a server,
- locate a server on PATH,
- use a user/workspace setting for the server path.

Rust Analyzer's current extension model is relevant even without copying its exact implementation: the extension is a client, the server is versioned independently, and project Rust version is still governed by rustup/toolchain selection. Go's `gopls` is an official language server installed as a tool. Deno exposes `deno lsp` from the runtime.

BAML decision:

- Public command should be `baml ide install`, forwarded by the wrapper to the selected toolchain payload. Do not add `baml setup-ide`.
- Toolchain install/update may recommend IDE setup/update, but must not silently install editor extensions.
- The VSIX should be platform-neutral and launch the configured `baml` wrapper from PATH or a user setting. It should not bundle platform-specific `baml-cli`.
- The extension should show mismatches in the editor when the project-pinned toolchain, active wrapper-selected toolchain, and VSIX protocol expectations disagree. The compatibility gate is explicit LSP/playground protocol ranges and capability flags, not exact BAML semver equality.
- LSP compatibility metadata should ride on the existing LSP `initialize` result. Playground compatibility should be checked only when the playground WebSocket connects. Do not add activation-time network checks, extra `baml` process spawns, or VSIX-owned toolchain downloads.

### GitHub Releases

GitHub Releases are a good asset store and maintainer-facing changelog:

- Releases are associated with tags.
- Release assets are easy for humans and automation to inspect.
- Release notes can be generated and edited.

They are not a complete channel directory. Mutable "latest canary" and "latest nightly" pointers are better represented by small manifest JSON files on `pkg.boundaryml.com`.

BAML decision:

- Create GitHub Releases for every canary toolchain.
- Create GitHub prereleases for nightly only if maintainers need the audit trail and GitHub asset UI. This is reasonable, but the user-facing channel should remain `pkg.boundaryml.com/manifest/v1/nightly.json`.
- `pkg.boundaryml.com` should be the directory/API: wrapper manifest, channel pointers, per-version metadata, install scripts.
- GitHub Releases should be storage/changelog infrastructure, not the normal end-user install surface.

## Direct Implications For BAML

### 1. What `baml install` Means

Reserve it for future project packages/dependencies. Toolchain/channel selection should move under `baml toolchain`.

Using `baml install nightly` for toolchains would force the future package manager into awkward alternatives such as `baml package install`, `baml add`, or `baml deps add`. The plan avoids that now.

### 2. How `baml update` Is Reserved

Do not define top-level `baml update` as the primary toolchain command. Use:

```bash
baml toolchain update   # active channel/toolchain
baml self-update        # wrapper binary
baml package update     # future package/dependency update
```

If a temporary top-level alias is ever added for v1 compatibility, document it as:

```text
Alias for: baml toolchain update
Updates the active BAML language toolchain. Does not update the wrapper or project packages.
```

This alias should print exactly what it updated:

```text
Updated nightly: 0.11.1-nightly.20260528.b -> 0.11.1-nightly.20260529.a
Wrapper unchanged. To update the wrapper, run baml self-update.
```

### 3. How should nightly channel resolution work?

Installing/using a channel should do both:

- record the selector (`nightly`) as the user's active channel;
- resolve and install the current concrete nightly version at that moment.

Normal commands should not do blocking network lookups. They should use the recorded/resolved local toolchain. Channel pointers refresh only on explicit commands:

```bash
baml toolchain update
baml toolchain use nightly
baml toolchain install nightly
```

Offline behavior:

- If the required local toolchain exists, run it.
- If the active selector is `nightly` but the network is unavailable during `toolchain update`, keep the current installed nightly and report that update could not check the remote.
- If a project pins an exact missing version and the network is unavailable, fail with the exact missing version and suggest retrying `baml toolchain install <version>` when online.

Status wording:

```text
Active channel: nightly
Installed version: 0.11.1-nightly.20260528.b
Latest known remote: 0.11.1-nightly.20260529.a
Run `baml toolchain update` to advance.
```

### 4. What should happen for `uvx baml generate` and `uv run baml generate`?

`uvx baml generate` runs a Python-distributed console script in an isolated uv tool environment. That console script should behave as a BAML launcher for CLI commands. It may install/download BAML toolchains in `~/.baml` only as part of explicit CLI execution. It must not mutate the uv tool environment or upgrade `baml_core`.

`uv run baml generate` runs inside a project Python environment. It should respect whatever `baml` package version the project resolved. If the project environment contains `baml_core`, it remains governed by the project lockfile/package manager.

`import baml_core` is never a launcher and never a self-updater.

Mismatch policy:

- CLI/toolchain versus project pin mismatch: wrapper selects project pin; if unsupported by wrapper, fail with wrapper update instructions.
- Generated code versus `baml_core` mismatch: runtime error with installed/generated/expected versions and package-manager command suggestions.
- PyPI launcher version versus toolchain version mismatch: allowed if launcher protocol supports the manifest; otherwise fail with actionable update command.

### 5. How should IDE install/update work?

Use `baml ide install` as the user-facing command. The command is implemented by the selected toolchain payload and exposed through wrapper pass-through. Support common editor flags such as `--cursor`, `--code`, `--windsurf`, and `--all`.

Toolchain install/update should run the same setup flow only when interactive, or print the follow-up command when not interactive. The flow must be idempotent.

The VSIX should launch local `baml lsp` through the wrapper. This makes IDE behavior match the CLI's project-pin resolution. The VSIX should not bundle platform-specific CLIs.

VSIX/toolchain compatibility is by protocol range, not by exact BAML version. The VSIX passes supported LSP/playground protocol ranges during LSP initialize, the LSP returns its current protocol metadata, and the playground validates its own protocol lazily over the WebSocket startup path. This avoids the old extension-owned "download exact CLI and restart" loop.

### 6. How should package-manager installs behave?

Package managers should install the wrapper only. First install may offer or run canary bootstrap if it is:

- idempotent;
- skipped when `~/.baml/config.toml` or installed toolchains already exist;
- non-resetting on wrapper upgrades;
- easy to disable in non-interactive installs.

Wrapper `self-update` must be refused for managed installs. The refusal should name the detected manager and command.

### 7. How should GitHub Releases be used?

Use GitHub Releases for immutable assets, checksums, provenance attachments when available, and maintainer-facing notes. Use `pkg.boundaryml.com` manifests for user-facing channel resolution and install/update directory lookups.

Canary and nightly can both have GitHub release pages, but nightly releases should be marked prerelease and should not be the primary user documentation path. Release notes should live in both generated GitHub releases and a human-readable changelog page for canary releases.

### 8. What security and reproducibility expectations are standard?

Minimum v1 expectations:

- immutable per-version archives;
- SHA-256 for every archive in the per-version manifest;
- manifest schema version;
- wrapper min-version requirement in manifests;
- atomic install into temp dir followed by rename;
- executable bit and expected binary names verified before activation;
- archive path traversal checks before extraction;
- idempotent reruns;
- mutable channel manifests that point to immutable per-version manifests;
- local cache that never runs a partially extracted toolchain.

Strongly recommended v1.1 or v1 if feasible:

- Sigstore/GitHub artifact attestation or SLSA provenance for release artifacts;
- manifest signatures or signed checksums;
- transparent provenance documentation for package-manager formulas.

`pkg.boundaryml.com` as directory plus GitHub Releases as storage is a reasonable split. Do not execute any downloaded toolchain until checksum verification, archive extraction validation, binary presence validation, and basic `baml-cli --version` sanity check pass.

## Specific Risks In The Current Plan

1. Top-level `baml install nightly` spends the most obvious future package-manager verb on toolchain selection.

2. Top-level `baml update` will be ambiguous once BAML has packages: users will not know if it updates wrapper, toolchain, dependencies, generated code, or indexes.

3. `baml version ...` is better than `install`, but still ambiguous because many CLIs use `version` for project/package versions. `toolchain` is more explicit.

4. Python launcher and Python runtime are easy to conflate. If the PyPI `baml` package both exposes `baml` and contains `baml_core`, users may reasonably expect `pip install baml==X` to control imports and CLI behavior together. The plan must document the boundary.

5. IDE setup must stay discoverable even though it lives in the selected toolchain payload. Use `baml ide install` in user docs and add common editor flags so users can avoid prompts.

6. Package-manager post-install bootstrap can become surprising if it runs on upgrades or changes existing channel state. The plan needs stricter idempotency language.

7. Automatic nightly following can accidentally become "network on every command." The plan should explicitly say normal pass-through commands are local-only and channel pointers refresh on explicit update/install/use.

8. GitHub Releases can become the accidental public API if manifest URLs point users there directly. Keep `pkg.boundaryml.com` as the documented API.

## Recommended Changes To `TASK.md`

These edits have been applied to the current plan and user-path docs.

1. Wrapper-owned commands are:

```text
baml toolchain install <canary|nightly|<version>>
baml toolchain use <canary|nightly|<version>>
baml toolchain uninstall <version>
baml toolchain list
baml toolchain update
baml self-update
```

2. `baml install`, `baml add`, and top-level `baml update` are reserved for future package/dependency management until that design is settled.

3. Nightly examples use:

```bash
baml toolchain use nightly
baml toolchain update
```

4. `baml update` is not the primary toolchain command. If a temporary alias is added later, it must clearly print that it is aliasing `baml toolchain update` and have a deprecation plan before package management ships.

5. Keep `baml ide install` as the primary user command. Describe it as wrapper pass-through to the selected toolchain payload, and include non-interactive editor flags.

6. Python packaging boundary:

```text
The Python runtime import (`baml_core`) is governed only by the Python environment. The Python `baml` console script is a launcher for CLI commands. It may install BAML toolchains under `~/.baml`, but it must never modify the installed Python wheel or silently upgrade `baml_core`.
```

7. Generated-code compatibility requirement: generated code embeds generator version and expected `baml_core` range; runtime mismatch produces a loud diagnostic with package-manager commands and `baml generate` when regeneration is the fix.

8. Nightly status semantics: active selector, installed concrete version, latest known remote, and explicit update command.

9. Package-manager bootstrap: first install only, idempotent, non-resetting, skipped for existing config/toolchains, non-interactive-safe, and no automatic editor extension install.

10. V1 security checks: SHA-256 verification, path traversal protection, temp-dir extraction, atomic rename, manifest schema/min wrapper version, and post-extract version sanity check.

## Recommended Changes To `USER-PATHS.md`

1. New Canary User:

`brew install baml` installs the wrapper and either bootstraps canary idempotently or prints `baml toolchain use canary`; editor setup is explicit through `baml ide install`.

2. Existing Canary User:

Use `baml toolchain update`.

3. Nightly User:

Use:

```bash
baml toolchain use nightly
baml toolchain update
```

4. IDE User:

Keep `baml ide install` as the primary story, show editor flags such as `baml ide install --cursor`, and note that the VSIX launches local `baml lsp`.

5. Python User:

Add explicit stories for:

```bash
uvx baml generate
uv run baml generate
python -c "import baml_core"
```

The first two are launcher/CLI paths; the last is runtime import and must report version mismatches rather than mutating the environment.

## Resolved Decisions From Review

1. `baml toolchain use <selector>` writes the active default selector in wrapper-owned user config. Project-local selection is done by editing `baml.toml [toolchain]`; do not make `use` prompt for global versus project-local in v1.

2. Normal pass-through commands do not make network requests. A missing exact pinned toolchain fails with installed-toolchain context and an explicit `baml toolchain install <version>` command. `baml toolchain use` and `baml toolchain install` may install missing toolchains because those commands are allowlisted for network access.

3. Top-level `baml update` is not the primary toolchain command. The documented command is `baml toolchain update`; top-level `baml update` remains reserved for future package/dependency semantics unless a temporary alias is intentionally added with a deprecation plan.

4. `baml ide install` installs the release-built VSIX/assets from the selected toolchain. It should support editor flags such as `--cursor`, `--code`, `--windsurf`, and `--all`; Marketplace publishing is deferred.

5. Keep the conceptual split between the Python `baml` launcher surface and `baml_core` runtime surface. The implementation may use one or two PyPI distributions depending on migration constraints, but imports must never self-update or mutate the Python environment.

6. Use `pkg.boundaryml.com` manifests as the user-facing channel directory. GitHub Releases may exist for nightly auditability and immutable asset storage, but they are not the normal user install/update surface.

7. Signing/provenance is not a v1 blocker. The minimum v1 bar is SHA-256, safe extraction, schema validation, and atomic install. Sigstore/SLSA/provenance should be tracked as a follow-up if it does not fit v1 release engineering scope.

## Sources

- Rust rustup overrides: https://rust-lang.github.io/rustup/overrides.html
- Rust rustup channels: https://rust-lang.github.io/rustup/concepts/channels.html
- Cargo update: https://doc.rust-lang.org/cargo/commands/cargo-update.html
- Cargo install: https://doc.rust-lang.org/cargo/commands/cargo-install.html
- Bun install: https://bun.sh/docs/pm/cli/install
- Bun update: https://bun.sh/docs/pm/cli/update
- Bun upgrade/nightly/Homebrew guidance: https://bun.com/docs/installation
- Bunx: https://bun.sh/docs/pm/bunx
- mise getting started: https://mise.jdx.dev/getting-started.html
- mise install: https://mise.jdx.dev/cli/install.html
- mise use: https://mise.jdx.dev/cli/use.html
- mise self-update: https://mise.jdx.dev/cli/self-update.html
- asdf versions and shims: https://asdf-vm.com/manage/versions.html
- asdf core update guidance: https://asdf-vm.com/manage/core.html
- npm npx: https://docs.npmjs.com/cli/commands/npx/
- Volta tool installation and pinning: https://www.voltajs.com/guide/installing.html
- uv tools: https://docs.astral.sh/uv/concepts/tools/
- uv CLI reference: https://docs.astral.sh/uv/reference/cli/
- uv cache: https://docs.astral.sh/uv/concepts/cache/
- pipx docs: https://pipx.pypa.io/canary/docs/
- Python entry points specification: https://packaging.python.org/en/latest/specifications/entry-points/
- Python version specifiers / PEP 440: https://packaging.python.org/en/latest/specifications/version-specifiers/
- Go toolchains: https://go.dev/doc/toolchain
- Go modules reference (`go install`, module cache, checksums): https://go.dev/ref/mod
- gopls package docs: https://pkg.go.dev/golang.org/x/tools/gopls
- Deno upgrade: https://docs.deno.com/runtime/reference/cli/upgrade/
- Deno install: https://docs.deno.com/runtime/reference/cli/install/
- Deno modules/cache/lockfiles: https://docs.deno.com/runtime/fundamentals/modules/
- Homebrew formula cookbook: https://docs.brew.sh/Formula-Cookbook
- Homebrew manpage: https://docs.brew.sh/Manpage
- Arch PKGBUILD: https://wiki.archlinux.org/title/PKGBUILD
- Arch VCS package guidelines: https://wiki.archlinux.org/title/VCS_package_guidelines
- Arch User Repository: https://wiki.archlinux.org/title/Arch_User_Repository
- VS Code language server extension guide: https://code.visualstudio.com/api/language-extensions/language-server-extension-guide
- VS Code install from VSIX: https://code.visualstudio.com/docs/configure/extensions/extension-marketplace
- GitHub Releases: https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases
