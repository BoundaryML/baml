# tools_xtask (`cargo xtask`)

Workspace task automation. The one subcommand today is `workflows`, which
generates the core `.github/workflows/*.yaml` files from typed Rust builders
so CI configuration is code-reviewed, refactorable, and drift-checked.

```bash
cd baml_language
cargo xtask workflows           # regenerate .github/workflows/*.yaml
cargo xtask workflows --check   # verify committed YAML matches the generators
mise run gen-workflows          # same as the first command
```

`cargo xtask` is a cargo alias (see `baml_language/.cargo/config.toml`) for
`cargo run -p cargo-xtask --`.

Generated files (one module per file, in `src/workflows/`):

| Module               | Output                                            |
| -------------------- | ------------------------------------------------- |
| `ci`                 | `ci.yaml` (orchestrator)                          |
| `cargo_tests`        | `cargo-tests.reusable.yaml`                       |
| `size_gate`          | `size-gate.reusable.yaml`                         |
| `wasm_pack_tests`    | `wasm-pack-tests.reusable.yaml`                   |
| `webview_tests`      | `webview-tests.reusable.yaml`                     |

Every generated file starts with an `# @generated` banner. **Do not edit the
YAML by hand** — edit the generator module and regenerate; CI runs
`cargo xtask workflows --check` so drift fails the build. The generator
deletes stale files carrying the banner but never touches hand-written
workflows (releases, docs, etc.).

## Running CI locally

The generated workflows are deliberately thin: almost every step is a plain
shell command run from `baml_language/` with tools provisioned by mise and
environment provided by the repo-root `.envrc`. To reproduce a job locally:

1. `mise install` (tool versions come from `mise.toml` / `.mise.toml`).
2. `direnv allow` at the repo root — CI runs the exact same `.envrc`
   (`direnv export gha >> $GITHUB_ENV`), so a direnv-enabled local shell has
   the same environment CI has.
3. Run the job's commands. The main ones:

| CI job                | Local equivalent (from `baml_language/`)                                                          |
| --------------------- | ------------------------------------------------------------------------------------------------- |
| prek (pre-commit)     | `prek run --all-files --hook-stage manual` (or `mise run clippy`, `mise run fmt`, …)               |
| cargo-test-{linux,macos,windows} | `cargo nextest run --all-features -E 'not package(baml_tests) and not package(/^sdk_test_/)'` |
| sdk-tests             | `cargo nextest run -p sdk_test_python_pydantic2 --all-features` (ditto `sdk_test_typescript_node`) |
| snapshot-tests        | `cargo insta test --test-runner nextest -p baml_tests -p baml_cli -p baml_lsp2_actions --all-features --unreferenced=reject` |
| cargo-test-wasm       | `cargo build <wasm pkgs> --target wasm32-unknown-unknown --no-default-features --release` (see generator for the package query) |
| cargo-build-msrv      | `cargo +<rust-version from Cargo.toml> test --no-run --all-features`                               |
| cargo-doc             | `RUSTDOCFLAGS='-D warnings' cargo doc --all --no-deps`                                             |
| size-gate             | `mise run size-gate` (per-platform: `cargo run -p cargo-size-gate -- size-gate check …`)           |
| prof-concurrency      | `RUSTFLAGS='--cfg baml_loom' CARGO_TARGET_DIR=target/loom cargo test -p bex_events --release --lib prof::` then `cargo +nightly miri test -p bex_events --lib prof::` (MIRIFLAGS=`-Zmiri-ignore-leaks -Zmiri-disable-isolation`) |
| proto-sync            | regenerate per `crates/bridge_ctypes/README.md`, then check `git status` is clean                  |

The authoritative command for any job is its generator module — read the
`run:` strings there rather than the YAML.

## Knobs GitHub Actions depends on

Things the workflows assume exist, in roughly the order they bite:

- **`.envrc` (repo root) is the single source of truth for the build
  environment.** Every job runs `direnv allow .envrc && direnv export gha`.
  It sets `RUSTC_WRAPPER`/`SCCACHE_*` unconditionally; the only inputs it
  reads are the two R2 credentials below. Locally it additionally sources
  `~/.envrc.baml` — that is where you put your own `BAML_SCCACHE_R2_*`.
- **Secrets:** `BAML_SCCACHE_R2_ACCESS_KEY_ID` /
  `BAML_SCCACHE_R2_SECRET_ACCESS_KEY`, threaded through
  `on.workflow_call.secrets` (optional) into the workflow `env:` block.
  `.envrc` maps them to the `AWS_*` names sccache expects. When absent (fork
  PRs, local without `~/.envrc.baml`), sccache falls back to the runner-local
  disk cache — builds still work, just colder.
- **mise + the `setup-mise` composite action**
  (`.github/actions/setup-mise`, `install_args:` per job): provisions
  sccache, direnv, python, uv, node, pnpm, protoc, and cargo tools via the
  `cargo:` backend (`cargo:cargo-nextest`, `cargo:cargo-insta`; cargo-binstall
  prebuilts). Windows also installs `clang` for llvm-strip. Versions are
  pinned by the repo's mise config, so CI and local share them.
- **Workflow-level env:** `CARGO_INCREMENTAL=0`, `CARGO_NET_RETRY=10`,
  `CARGO_TERM_COLOR=always`, `RUSTUP_MAX_RETRIES=10` (see
  `vars::standard_env`).
- **Windows specials:**
  - `CARGO_BUILD_JOBS=16` on the 8-vCPU Windows runners (2x oversubscription
    was the measured sweet spot).
  - the native `tools_sccache` wrapper is built first and exported as
    `RUSTC_WRAPPER` (cmd.exe's ~8191-char limit breaks script wrappers;
    see the step comment in `cargo_tests::build_native_sccache_wrapper`).
  - sdk python tests export `UV_FIND_LINKS=<target>/wheels` so fixtures
    install the prebuilt `baml_core` wheel instead of rebuilding it.
  - linker overrides live in `baml_language/.cargo/config.toml`
    (`rust-lld` for `x86_64-pc-windows-msvc`).
- **Caching:** Swatinem/rust-cache holds registry/git state (not targets);
  sccache holds compiled artifacts. Shared keys: `linux-cargo`,
  `macos-cargo`, `windows-cargo`, `linux-wasm`, `linux-msrv`,
  `prof-concurrency`, `linux-arm-bench`. Only pushes to `canary` save
  (`save-if`); PR jobs are read-only consumers.
- **Runner labels** live in `workflows/runners.rs` (Blacksmith labels like
  `blacksmith-8vcpu-ubuntu-2404`, `blacksmith-6vcpu-macos-latest`,
  `blacksmith-8vcpu-windows-2025`). Blacksmith jobs use
  `useblacksmith/checkout@v1`; GitHub-hosted/macos/windows use
  `actions/checkout@v6`.
- **Change gating:** `ci.yaml`'s `determine_changes` job diffs against the
  merge base and exposes outputs (`code`, `docs`, `webview`, `proto`,
  `prof`, `run_perf`, …) that gate every downstream job. If you add a path
  that should trigger a job, update the corresponding `change_check`
  pathspec list in `workflows/ci.rs`.
- **Merge queue:** `merge_group` events skip macos/windows legs and non-linux
  sdk matrix entries (they already ran on the PR).

## Generator architecture notes

`tasks/workflows.rs` is the generation driver. The gh-workflow fork cannot
express a few YAML constructs, so the driver applies post-serialization text
transforms — keep them in mind when editing:

- `secrets: inherit` (sentinel `with:` key, see `vars::call_reusable_inherit`)
- `permissions: write-all` (collapsed from the 10-scope map)
- `timeout-minutes: ${{ matrix.timeout }}` (sentinel integer `424242`)
- arbitrary-precision `serde_json` numbers collapsed back to plain scalars

The emitted YAML is re-parsed after the transforms as a sanity check, but
comments in `run:` shell strings are the only comments that survive
generation — YAML-level comments from the old hand-written files are gone by
design.
