# BAML GitHub Actions conventions (Python)

Run `mise run github-actions-lint-test` and `mise run github-actions-lint`. CI installs only Python and uv through `.github/actions/setup-mise`, runs the frozen uv project, and executes both commands in the unconditional `github-actions-rules` job in `ci.yaml`. Both the required failure alert and release dispatch depend on that job. No BAML compiler, generated client, API key, or nightly BAML installation is needed.

`rules.py --root /path/to/repo --format text|json|github` supports local diagnostics, machine-readable output, and escaped GitHub error annotations. A violation exits nonzero. Diagnostics identify the file, line, column, rule, job/composite, and step, and suggest the canonical replacement. `--list-exclusions` prints the complete exclusion inventory and reasons.

## Scope and policy

Discovery includes every `.github/workflows/*.yml` and `*.yaml`, plus every repository `action.yml` and `action.yaml`, including untracked files, nested actions outside `.github`, and currently unreferenced actions. Only VCS metadata and installed dependency directories (`.git`, `.jj`, `node_modules`, `.venv`, `__pycache__`) are traversal boundaries. There is no workflow naming pattern, v1 opt-in list, branch filter, or changed-files filter. A newly added workflow or action is checked automatically. New local JavaScript/Docker actions fail pending explicit policy review because their setup implementation cannot be inspected as composite steps. Exclusions must name existing exact files and have nonempty reasons; stale and wildcard exclusions fail.

- Install host tools through `./.github/actions/setup-mise`, including uv, Python, Node, pnpm, Go, Ruby, .NET, and Cargo CLI utilities. Existing v1 convenience actions delegate their tool installation to that action. Runtime-specific versions can be passed in `install_args`, preserving Python wheel build interpreters, Node 24 for typescript2/npm publishing, and pnpm 9 for the separate legacy root dependency workspace used by the v1 stdlib report.
- Remote action identities are reviewed explicitly in `APPROVED_ACTIONS`; unknown actions fail, even if their names do not contain `setup`. This is an action approval policy, not a list of files to inspect. `Swatinem/rust-cache`, setup-node/setup-python/setup-go/setup-dotnet, standalone setup-uv/pnpm, dtolnay/rust-toolchain, taiki-e/install-action, alternative sccache actions, and direct jdx/mise-action usage outside the canonical bootstrap all fail regardless of version or case.
- `BoundaryML/setup-baml` is an explicitly approved BAML toolchain setup exception. `PyO3/maturin-action` is a narrowly approved wheel builder for the existing ABI/manylinux packaging pipeline: only `command: build`, with its GitHub-backed sccache option disabled, and after canonical R2 setup. Its isolated wheel-building bootstrap remains owned by that specialized builder. Ordinary artifact, deployment, authentication, and reporting actions have separately documented approval reasons in the code.
- Rust toolchains, targets, and components continue to use `rustup`, as in the existing CI. Auxiliary binaries such as cross, wasm-pack, cargo-nextest, and cargo-codspeed come from mise. Only the exact `setup-rust/action.yml` may download rustup itself for Windows ARM64 and minimal build containers. Only the exact `setup-mise/action.yml` may call jdx/mise-action and cargo-bins/cargo-binstall; adding another installer to either action still fails.
- Host Rust compilation must follow an unconditional canonical cache setup in the same job. `setup-sccache` installs sccache/direnv through mise and loads the existing `.envrc`, preserving its R2 credentials, endpoint, namespace, and secretless local fallback. Windows builds bootstrap the existing native `tools_sccache` wrapper. Reusable workflows declare and forward only the two optional R2 secrets. The prior explicit `direnv allow .envrc` plus `direnv export gha >> "$GITHUB_ENV"` convention is also accepted when preceded by mise installation of both tools. Composite wrappers are followed recursively; a similarly named or empty action cannot claim to configure caching.
- Rust artifacts, Cargo registries/git state, sccache storage, opaque whole-cache paths, and whole-workspace caches may not use actions/cache, including restore/save subactions. Non-Rust caches, such as prek and NuGet dependencies, remain allowed. Direct environment overrides of the R2 backend, disabling the repository Rust wrapper, missing optional R2 credential mappings, and re-enabling mise task auto-install fail.

## Installing versus using tools

The linter parses YAML and Bash syntax; it does not grep entire YAML documents for tool names. Duplicate keys, recursive aliases, malformed YAML, invalid step structures, and unresolved local references are errors. Ordinary YAML aliases are inspected at each use. Shell checks inspect executable commands, including command substitutions, shell `-c` commands, wrappers such as `sudo`/`env`, and static matrix-provided shell snippets. Comments, quoted log messages, and literal heredoc bodies are data. A whole-step executable GitHub expression that cannot be resolved from a static matrix fails rather than being silently ignored.

`cargo install`, `cargo binstall`, global npm/pnpm installs, `corepack enable`, `go install`, `uv tool install`, `uv python install`, installation of Python CLI tools, direct mise installation, and recognized tool bootstrap downloads are installations. `cargo fetch`, `uv run`, `uv sync`, project dependency installation (`uv pip install -r`, `npm ci`, `pnpm install`), `mise run`, version checks, and Playwright browser provisioning are tool use or project dependencies. Direct cargo/cross/wasm-pack compilation is checked for prior R2 setup, including the existing `$CARGO` convention.

This is a repository convention checker, not a security sandbox or a replacement for actionlint. It does not execute workflows, evaluate arbitrary GitHub expressions, prove shell control flow, or trace arbitrary scripts, JavaScript actions, Dockerfiles, package scripts, or external build containers. Operating-system library provisioning and native cross-compilation SDKs remain separate from language/package-manager installation. Changes to those implementation files still need review. PowerShell's common external tool invocations are recognized by the command scanner; Bash is the repository's canonical shell and the fully parsed syntax. The tests cover real bypass attempts and false positives, not just the current repository passing.

## Manual v0 audit and exact exclusions

The initial audit reviewed all 70 existing workflow/action YAML files at base commit `b9951d7ecb981670d28e594b3482b9a63e22d45c`, including job steps, working directories, release products, local action references, and reusable workflow callers. The 27 exact exclusions below are also stored in `exclusions.json`, the executable source of truth. No excluded file was modified. Existing v1 callers cannot reuse an excluded v0 setup action. A v1 entry point may still invoke the explicitly excluded shared v0 documentation workflow; that does not exempt any other job in the caller.

| Exact excluded path | Reason |
| --- | --- |
| `.github/actions/engine-setup-node/action.yml` | Node/pnpm/Turbo setup for engine and the legacy root workspace, consumed by v0 CI and releases. |
| `.github/actions/engine-setup-ruby/action.yml` | Ruby setup for v0 engine tests and coverage. |
| `.github/actions/engine-setup-rust/action.yml` | Rust/cross/wasm setup whose workspace defaults to engine, consumed by v0 CI and releases. |
| `.github/actions/setup-all/action.yml` | Legacy aggregate action defaults rust-workspace to engine and delegates to engine setup actions; excluded even though currently unreferenced. |
| `.github/actions/setup-go/action.yml` | Legacy Go/goimports setup used by engine builds and v0 release workflows. |
| `.github/actions/setup-java/action.yml` | Java/Gradle setup used to build and publish the v0 JetBrains extension. |
| `.github/actions/setup-node/action.yml` | Legacy root workspace pnpm/Turbo setup; v1 consumers must use setup-mise instead. |
| `.github/actions/setup-protoc/action.yml` | Pinned protoc 23.4 bootstrap consumed by engine-setup-rust and v0 Rust SDK tests; v1 uses mise's protoc pin. |
| `.github/actions/setup-python/action.yml` | Installs maturin/ruff and synchronizes integ-tests/python for v0 engine jobs. |
| `.github/actions/setup-tools/action.yml` | Legacy tool setup used by primary.yml and release.yml; the v1 canonical action is setup-mise. |
| `.github/workflows/build-cli-release.reusable.yaml` | Called by the v0 release; packages engine/target/baml-cli and baml_cffi with README.v0.md. |
| `.github/workflows/build-jetbrains-release.reusable.yaml` | Builds and verifies jetbrains/ for the v0 release.yml pipeline. |
| `.github/workflows/build-python-release.reusable.yaml` | Builds engine/language_client_python wheels and tests their legacy license packaging. |
| `.github/workflows/build-ruby-release.reusable.yaml` | Builds engine/language_client_ruby gems with rb-sys-dock, including the currently unused reusable release entry point. |
| `.github/workflows/build-typescript-release.reusable.yaml` | Builds engine/language_client_typescript native bindings for the v0 release. |
| `.github/workflows/build-vscode-release.reusable.yaml` | Packages typescript/apps/vscode-ext with the v0 CLI and engine WASM. |
| `.github/workflows/docs.reusable.yaml` | Publishes the legacy fern/ documentation, including v0 SDK documentation; called from ci.yaml but intentionally excluded as a whole shared v0 publishing workflow. |
| `.github/workflows/integ-tests.yml` | Runs the legacy integ-tests using engine setup actions and the engine clients. |
| `.github/workflows/primary.yml` | BAML v0 CI: builds engine/, engine/baml-schema-wasm and integ-tests; also syncs the legacy Zed extension. |
| `.github/workflows/publish-crates-0.226.2.yml` | One-off publication of the v0 0.226.2 crates from languages/rust. |
| `.github/workflows/publish-jetbrains-0.225.1.yml` | One-off publication and Marketplace repair of the v0 0.225.1 JetBrains plugin. |
| `.github/workflows/publish-zed-release.reusable.yaml` | Called by release.yml to update the v0 Zed extension release. |
| `.github/workflows/release.yml` | BAML v0 release entry point: publishes engine clients, languages/rust, legacy editor packages and explicitly names the release BAML v0. |
| `.github/workflows/rust-coverage.yml` | Collects engine/ coverage, building the v0 baml_cffi with legacy language clients. |
| `.github/workflows/test-go-windows-quick.yml` | Quick Windows smoke test of engine baml_cffi and engine/language_client_go. |
| `.github/workflows/test-go-windows.yml` | Builds engine/language_client_cffi and tests engine/language_client_go on Windows and Unix. |
| `.github/workflows/test-rust-sdk.yml` | Tests languages/rust against engine baml_cffi, not baml_language/sdks/rust. |

All remaining existing files stay in scope: the main v1 CI, every `build2-*`/`publish2-*` release producer and publisher, release/nightly dispatch, v1 artifact verification, cargo/size/WASM/webview tests and their `ix-*` previews, cache/baseline refresh, developer-docs, TypeScript2 and grammar mirrors, stdlib matrix, Gradle plugin publishing, oncall, product metrics, and atb2 deployment. The shared canonical setup-mise action remains checked because it is the v1 installation boundary, even though legacy helpers also call it. setup-rust, setup-node2, setup-ruby, and setup-musl-cross remain checked. The new setup-sccache action is checked automatically.

## Migration and parallel work

The migration removes in-scope Swatinem caches and alternate host setup actions, routes language tools and CLI dependencies through mise, centralizes the existing R2 setup, and forwards the optional cache credentials through reusable callers. It preserves non-Rust dependency caches and the separate Node/pnpm/Python version requirements. The wheel builder receives the mise-installed sccache binary and R2 credentials in its Linux container, with the repository wrapper on PATH; its competing GitHub cache remains disabled.

`developer-docs.yml` overlaps another independent change: this checkout replaces the snippets job's Swatinem cache with setup-sccache and passes its R2 credentials; its Node setup delegates through the migrated setup-node2 action. Reconcile those equivalent changes when choosing or combining the independent implementations. This directory is the Python alternative and does not depend on the separate BAML implementation.
