# GitHub Actions conventions, implemented in BAML

This is a deterministic BAML program: nightly BAML performs file discovery, YAML parsing, shell-command classification, convention checks, annotation rendering, and exit-status selection. There is no Python/JavaScript lint implementation, generated SDK, model call, API key, or network request during linting. Git is used only to enumerate additional action manifests outside `.github`.

## Run

From the repository root:

```sh
baml toolchain use nightly
baml --project tools/baml-github-actions-rules check
baml --project tools/baml-github-actions-rules test
baml --project tools/baml-github-actions-rules run main
```

The implementation was validated with wrapper `0.2.4` and nightly `0.18.1-nightly.20260908.a`. The project selects the nightly channel in `baml.toml`. CI installs it with [`BoundaryML/setup-baml`](https://github.com/BoundaryML/setup-baml/tree/01c5b25be8c853c3192c3891a0fc62f2a3ea4969), using its documented `toolchain: nightly` input. The action is pinned to current `main` commit `01c5b25be8c853c3192c3891a0fc62f2a3ea4969`: the README suggests `@v1`, but that tag did not exist when checked. The independent `github-actions-rules` job in `ci.yaml` runs on every CI event without change detection, and participates in both the failure gate and release-dispatch gate.

To inspect another checkout, append `-- --repo_root /absolute/path` to `run main`. For structured output without exiting on violations, run `baml --project tools/baml-github-actions-rules run repository_report --output-format json`. `main` emits escaped GitHub error annotations with the file, job, zero-based step index, name, field, rule, and remediation; it exits 1 for violations. YAML parse diagnostics retain the parser's source information. Discovery or I/O failures also fail the command.

## Coverage

Every `.yml` and `.yaml` file beneath `.github/workflows` and every `action.yml` and `action.yaml` beneath `.github/actions` is discovered recursively, including untracked and ignored files. Git also enumerates tracked and untracked action manifests elsewhere in the repository, using standard Git ignores to omit generated dependencies. Local action/reusable-workflow references must resolve to a discovered manifest. There is no workflow filename prefix exclusion or opt-in list. Renamed or newly added legacy files are checked until separately inspected; stale exclusion paths fail the repository audit. Symlinks in the `.github` trees are rejected.

The 27 exact v0 exclusions below were manually audited from their build roots and call sites. Everything else remains in scope, including `ix-*`, developer docs, release verification, on-call, metrics, deployment automation, and the stdlib matrix. The stdlib matrix uses root-workspace dependencies for extraction but produces a v1 stdlib report, so it is covered. An in-scope file cannot call an excluded v0 setup action. `ci.yaml` can still call the explicitly excluded shared Fern publishing workflow.

## Rules

| Rule | Convention |
| --- | --- |
| `TOOL-ACTION` | Install tools through `./.github/actions/setup-mise`. `BoundaryML/setup-baml` is approved for BAML. Standalone Node, pnpm, Python, uv, Ruby, Go, .NET, Rust installation actions and direct mise/cargo-binstall actions are rejected outside their canonical bootstrap. Unknown external actions fail pending an explicit semantic review; the reviewed list covers ordinary checkout, artifacts, credentials, publication and reporting operations, not a file coverage list. |
| `TOOL-INSTALL` | Reject direct global tool installation using cargo/binstall, npm/pnpm/yarn, uv tool/python, pip/pipx, Go, dotnet tool, corepack, mise install/use, managed tools through system package managers, and downloaded installer scripts. Shell quoting, comments, line continuations, command separators, environment assignments, common command wrappers, and command substitutions are recognized. |
| `RUST-CACHE` | Reject Swatinem, alternative sccache actions, and generic caches of Cargo state or Rust targets. Generic caches must use literal non-Rust paths. Existing pre-commit, NuGet, and C# build caches remain valid. |
| `RUST-SETUP` | Direct Rust builds/checks/tests require prior repository cache setup in the same job. A conditional setup only covers builds with that exact condition; unconditional setup covers all later builds. A setup allowed to fail does not establish caching. |
| `CACHE-CONFIG`, `R2-CREDENTIALS` | Keep wrapper/backend configuration in `.envrc`, bind both optional repository R2 secrets, and forward them through reusable workflows. Empty secrets on fork PRs intentionally select the existing local sccache fallback. |
| `BOOTSTRAP` | Preserve the canonical mise cache policy, disabled shims and task auto-install, and the canonical Rust/R2 setup contracts. Bootstrap paths do not exempt unrelated installation commands. |
| `YAML`, `STRUCTURE`, `ACTION`, `LOCAL-ACTION`, `EXCLUSION` | Fail malformed/ambiguous YAML, invalid step structure, dynamic or missing action references, missing local manifests, and stale exclusions instead of silently skipping them. |

Tool use is different from tool setup: `uv run`, `uv sync`, `pnpm install --frozen-lockfile`, `npm ci`, project-local dependency installation, `npm exec`, and invoking installed tools are allowed. Rust toolchains/components/targets continue to use rustup, as in `ci.yaml`; they are intentionally not installed through mise. `cargo fetch` and metadata do not compile. Miri interprets code and does not use compilation caching. Product installer smoke tests exercise the product being released; they are not alternative setup steps for CI build tools.

The shell analysis is static and intentionally does not execute or recursively interpret arbitrary repository scripts, JavaScript/PowerShell programs, or runtime-generated shell. Review changes to those implementations and to approved external actions normally. YAML anchors/aliases and folded scalars are parsed semantically by nightly BAML rather than scanned as raw YAML text. This linter complements actionlint; it is not a replacement for the complete GitHub Actions schema or a security sandbox.

## Canonical bootstrap and migration

`setup-mise/action.yml` alone may use `cargo-bins/cargo-binstall` (required before resolving mise's cargo backend) and `jdx/mise-action`. The composite also exports `mise bin-paths` for the exact requested versions: upstream mise-action exports the project configuration, which can otherwise leave a differently requested Node/Python version off PATH. Its existing `mise-baml3` cache prefix, canary-only cache writes, direct downloads, disabled shims, and disabled task auto-install are checked. `setup-rust/action.yml` alone may use `dtolnay/rust-toolchain` to bootstrap the existing rustup convention; its auxiliary tools now come from mise and its cache setup is unconditional. Native OS/linker dependencies, including the existing `setup-musl-cross` action, remain OS-level bootstrap.

`setup-sccache/action.yml` is a small composite around the existing R2 implementation: it installs `sccache direnv` through `setup-mise`, runs `direnv allow .envrc` and `direnv export gha`, and on Windows builds the repository's native `tools_sccache` wrapper before pointing `RUSTC_WRAPPER` at it. On Linux it also configures cross container mounts for the mise-installed static sccache binary and repository wrapper, forwards the existing cache variables and credential names, and uses a container-local socket/log. `CROSS_CONTAINER_OPTS` applies to both checked-in and generated cross configurations, without copying secrets into configuration files. Credentials are scoped to cache-consuming jobs; the BAML lint job and unrelated publication/verification jobs do not receive them. `.envrc` and `tools/baml-sccache` still own the backend and credentials translation. The only uncached bootstrap compilation exception is the exact native wrapper command in that Windows step; the containing action is otherwise linted normally.

`PyO3/maturin-action` is reviewed as a wheel-build operation, only with `command: build` and its alternative cache disabled. Its isolated manylinux/musllinux packaging containers retain their build internals; using the action merely to install standalone maturin is rejected. Host Python setup now goes through mise and preserves the explicit 3.10/3.11 wheel-build interpreter selection. BAML's external setup action is independently approved by repository identity, for any normal action ref.

The migration removes in-scope Swatinem caches, routes language/CLI setup through mise, preserves Node 24 and the selected pnpm version in `setup-node2`, preserves LLVM 18 for package inspection, and installs CodSpeed through the pinned mise entry. The Node publisher invokes its package-declared napi CLI via `npm exec` instead of installing it globally. Legacy excluded files remain unchanged. The `developer-docs.yml` cache migration and `setup-node2` adjustment overlap the independent developer-docs migration being developed in another worktree; reconcile those changes when choosing/landing this alternative. This PR does not incorporate the separate Python linter implementation.

## Exact v0 exclusions

The executable source of this table is `baml_src/exclusions.baml`; the repository audit verifies that every entry still exists.

| Exact repository path | Audited reason |
| --- | --- |
| `.github/workflows/primary.yml` | Legacy runtime CI builds engine/, typescript/, and integ-tests/ and synchronizes engine/zed. |
| `.github/workflows/release.yml` | Legacy version-tag release orchestrates engine CLI/SDKs, typescript VS Code, JetBrains and engine/zed publication. |
| `.github/workflows/build-cli-release.reusable.yaml` | Builds engine baml_cffi and the legacy baml-cli for release.yml. |
| `.github/workflows/build-python-release.reusable.yaml` | Builds engine/language_client_python wheels and audits those wheels. |
| `.github/workflows/build-ruby-release.reusable.yaml` | Builds engine/language_client_ruby gems with rb-sys-dock. |
| `.github/workflows/build-typescript-release.reusable.yaml` | Builds native addons in engine/language_client_typescript. |
| `.github/workflows/build-vscode-release.reusable.yaml` | Packages typescript/apps/vscode-ext with engine CLI artifacts and the legacy playground. |
| `.github/workflows/build-jetbrains-release.reusable.yaml` | Builds and verifies jetbrains/ for the legacy release.yml pipeline. |
| `.github/workflows/publish-jetbrains-0.225.1.yml` | One-off recovery publication of the legacy JetBrains 0.225.1 plugin. |
| `.github/workflows/publish-crates-0.226.2.yml` | One-off publication of languages/rust crates from verified engine release source. |
| `.github/workflows/publish-zed-release.reusable.yaml` | Publishes and synchronizes the engine/zed extension. |
| `.github/workflows/integ-tests.yml` | Runs legacy tools/bctl integration suites against engine clients. |
| `.github/workflows/test-go-windows-quick.yml` | Builds engine baml_cffi and tests engine/language_client_go on Windows. |
| `.github/workflows/test-go-windows.yml` | Tests engine/language_client_go against engine baml_cffi, including cross compilation. |
| `.github/workflows/test-rust-sdk.yml` | Builds engine baml_cffi and tests languages/rust (not baml_language/sdks/rust). |
| `.github/workflows/rust-coverage.yml` | Manual coverage of the engine workspace with legacy Go/Python/Ruby setup. |
| `.github/workflows/docs.reusable.yaml` | Shared Fern publisher called by ci.yaml; fern/ still publishes v0 documentation, so the whole existing file is explicitly excluded. |
| `.github/actions/engine-setup-rust/action.yml` | Rust/wasm/cross bootstrap and cache for engine release and runtime CI. |
| `.github/actions/engine-setup-node/action.yml` | Node/pnpm and Turbo setup for the legacy root workspace and engine clients. |
| `.github/actions/engine-setup-ruby/action.yml` | Ruby setup used by primary.yml and rust-coverage.yml for engine. |
| `.github/actions/setup-all/action.yml` | Legacy development composite defaults to engine and delegates to engine setup actions. |
| `.github/actions/setup-go/action.yml` | Shared legacy Go setup used by engine CLI, runtime and integration workflows. |
| `.github/actions/setup-java/action.yml` | Java/Gradle setup used by legacy JetBrains release and recovery workflows. |
| `.github/actions/setup-node/action.yml` | Legacy root workspace Node/pnpm/Turbo setup; v1 callers must migrate to mise. |
| `.github/actions/setup-protoc/action.yml` | Shared protoc bootstrap used by engine-setup-rust and legacy Rust SDK tests; v1 callers must use mise. |
| `.github/actions/setup-python/action.yml` | Legacy Python/uv/maturin/ruff setup with integ-tests/python dependency installation. |
| `.github/actions/setup-tools/action.yml` | Legacy disk cleanup and direct mise bootstrap used by engine workflows. |

## Validation

`baml test` covers prohibited actions and installer commands, normal tool use and dependencies, quoting/comments/continuations/substitutions, YAML aliases and folded/flow forms, malformed and duplicate YAML, all exact exclusions and renamed siblings, future workflow/action discovery, stale exclusions, local manifest resolution, cache order/conditions, R2 forwarding, bootstrap contracts, and annotation escaping. Its CLI integration test creates an isolated Git repository and proves that both an ignored new workflow and an untracked action outside `.github` produce annotations and exit status 1. No test runs an LLM.

## Deliberate review decisions

The nightly selector is intentional: this implementation was explicitly requested using `baml toolchain use nightly`, and CI follows that same toolchain through `BoundaryML/setup-baml`. A rolling nightly can expose upstream regressions; selecting a fixed nightly should be a separate policy decision.

R2 access on same-repository pull requests and maintainer-dispatched dry runs preserves the existing `ci.yaml` cache trust model. Fork pull requests receive empty repository secrets and use the local fallback. Removing R2 from all pull requests or dispatched source builds would change that requested convention; this change instead limits credentials to jobs that actually initialize the cache and uses explicit secret forwarding at the new release call sites.
