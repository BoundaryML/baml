# Test infrastructure for the `sys_llm` → native-BAML migration

Research date: 2026-08-12. Repo: `/Users/aaron/projects/baml/baml_language` (branch `canary`, HEAD `593a51363`).
Every claim below is cited `path:line`. Paths are repo-relative to `baml_language/` unless they start with
`/Users/aaron/projects/baml/` (monorepo root — `.infisical.json`, `.github/`, `integ-tests/`, `engine/` live there).

---

## 0. TL;DR

| Question | Answer |
|---|---|
| Do test **groups** exist? | Yes — `testset "name" { … }` is the only grouping construct. No `group`/`tags` attribute on `test` blocks. |
| Can a group be excluded from plain `baml test` but opted into? | **Yes, today, with zero compiler work.** `[test] default = …` + `[test.profiles.<name>].args` in `baml.toml` (`crates/baml_cli/src/manifest.rs:54-76`), plus glob `--include`/`--exclude` selectors. Verified end-to-end below. |
| Is that mechanism used anywhere in the repo today? | **No.** Zero `baml.toml` in the monorepo contains a `[test]` table (grep, §1.6). The feature is fully built + e2e-tested but has zero adoption. |
| Second, orthogonal gate | A `testset` body is **arbitrary lazily-evaluated BAML**, so `if (baml.env.get("OPENAI_API_KEY") == null) { … } else { test … }` registers zero tests when the key is absent. Verified below. Belt-and-braces with the profile. |
| Infisical | Monorepo-wide `.infisical.json` at `/Users/aaron/projects/baml/.infisical.json`, env `test`, 115 secrets. `infisical run -- <cmd>` works from any subdir. |
| Keys available | `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GOOGLE_API_KEY`, `VERTEX_API_KEY`, `INTEG_TESTS_GOOGLE_APPLICATION_CREDENTIALS_CONTENT`, `AZURE_*`, `GROQ`, `OPENROUTER`, … — but **`AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` are empty strings** (§2.3). |
| Where do provider tests live now | Rust `#[cfg(test)]` inside `crates/sys_llm/src/**` (~319 `#[test]`s, all offline), plus Rust wiremock/request-capture suites in `crates/baml_tests/tests/`, plus native-BAML suites in `crates/baml_tests/baml_src/ns_*/`. |
| Repo preference (confirmed in code) | Native BAML `test` blocks; Rust only when the assertion cannot be expressed in BAML — see the explicit comment at `crates/baml_tests/tests/http.rs:3-4`. |

---

## 1. Does `baml test` support groups / tags / filters?

### 1.1 The grammar: `testset` is the only grouping construct; there are no tags

Test-block parsing lives in one file, `crates/baml_compiler_parser/src/parser.rs`:

- `parse_test_expr` — `crates/baml_compiler_parser/src/parser.rs:7862-7904`
  Grammar: `test <name_expr> [with <runner_expr>] { <block> }`. The name is an **expression** that must
  type-check as a string (`p.parse_expr()` at :7889), so names can be computed.
- `parse_testset` — `crates/baml_compiler_parser/src/parser.rs:7906-7946`
  Grammar: `testset <name_expr> [with <runner_expr>] { <testset body> }`.
- `parse_testset_body` — `crates/baml_compiler_parser/src/parser.rs:7950-7954`
  Bumps `testset_body_depth`, which is what makes nested `test`/`testset` legal *only* at top level or
  inside a testset (`parser.rs:4434-4451`).

There is **no** `group`, `tags`, `@tag`, or attribute syntax on a test block. The only two `group`/`tags`
hits in the parser are unrelated (a `tags string` field in a config-block parse test,
`parser.rs:9101`, `parser.rs:9119`). Old-style `test Name { functions [...] args {...} }` is still
parsed (`parser.rs:7842-7857`), and it also has no tag slot.

**Grouping is therefore purely lexical/naming**: a testset contributes a `::`-joined segment to the
canonical id.

### 1.2 Canonical ids and selectors

Canonical id shape: `root[.namespace]::testset::…::test` — documented in the CLI's after-help,
`crates/baml_cli/src/test_command.rs:34-38`, and produced by
`TestCollector.register_test` / `register_test_set`
(`crates/baml_builtins2/baml_std/testing/registry.baml:33-59` and `:61-94`), which join `prefix + "::" + name`.
`::` is *reserved* inside a declared name and raises a discovery error
(`registry.baml:34-38`, `:67-71`; e2e coverage `crates/baml_cli/tests/test_profiles_e2e.rs:274-320`).

Selector semantics are implemented **twice** and kept in sync:

- Rust: `crates/baml_cli/src/test_filter.rs:18-73` (`glob_match`, `TestFilter::includes_patterns`).
- BAML: `crates/baml_builtins2/baml_std/testing/registry.baml:573-639` (`glob_match`, `leaf_selected`,
  `leaf_selected_layered`).

Rules (`test_filter.rs:1-17`, `test_command.rs:34-38`):
- A selector **without** `*` is a case-sensitive **substring** match on the full id.
- A selector **with** `*` is an **anchored full-id glob**; `*` also matches `::`.
- Repeated `--include` are OR'd; repeated `--exclude` are OR'd; **excludes always win**.
- No includes ⇒ everything not excluded.
- Profile filters and CLI filters are two **independent layers** AND'ed together — deliberately not
  concatenated (`registry.baml:627-639`).

### 1.3 CLI flags on `baml test`

`crates/baml_cli/src/test_command.rs:65-129` (`struct TestArgs`), wired at `crates/baml_cli/src/commands.rs:174`:

| Flag | Line | Meaning |
|---|---|---|
| `--profile <NAME>` | `test_command.rs:76-82` | Apply a named profile from `baml.toml`. Conflicts with `--no-profile`. |
| `--no-profile` | `test_command.rs:85-91` | Ignore the configured default profile. |
| `--list` | `test_command.rs:96-97` | Print selected canonical ids instead of running. |
| `--include` / `-i` | `test_command.rs:99-101` | Repeatable include selector. |
| `--exclude` / `-x` | `test_command.rs:103-105` | Repeatable exclude selector; wins over includes. |
| `--logs <LEVEL>` | `test_command.rs:112-123` | `off` (default) / error / warn / info / debug. |
| `--from <PATH>` | `test_command.rs:69-70` | Hidden; project root override (used by the corpus runner). |

There is **no** `--env`, no `--tag`, no `--live`, no env-gating flag. `BAML_LOG` is **not** read by the new
CLI (grep for `BAML_LOG` across `crates/` returns nothing) — logging is `--logs` only. That matters
because Infisical injects `BAML_LOG=info` (§2.3) and it is harmless here.

### 1.4 Profiles — the built-in opt-in/opt-out mechanism

Manifest schema: `crates/baml_cli/src/manifest.rs:42-76`.

```toml
[test]
default = "regular"          # manifest.rs:56-57 — profile used by a bare `baml test`

[test.profiles.regular]      # manifest.rs:59-61
args = ["-x", "::integration::", "--color", "never"]   # manifest.rs:67-72 — argv array, never a shell string
```

- A profile is **preset `baml test` argv**, parsed by the real clap grammar
  (`test_command.rs:851-896`), so anything valid on the command line is valid in a profile.
- Banned inside a profile: `--profile`, `--no-profile`, `--project`, `--directory`, `--from`,
  `--features`, `--help` (`test_command.rs:851-872`).
- Resolution order: `--no-profile` → explicit `--profile` → `[test].default`
  (`test_command.rs:771-778`). A missing profile is an actionable error listing the available names
  (`test_command.rs:786-802`).
- Direct CLI scalars override profile scalars (`test_command.rs:814-840`); CLI includes **narrow**
  the profile's set rather than OR-ing with it (`test_command.rs:842-846` + `registry.baml:630-639`;
  e2e `crates/baml_cli/tests/test_profiles_e2e.rs:119-135`).
- Unknown keys in `[test.profiles.<name>]` warn rather than fail (`manifest.rs:205-217`).

Filters reach BAML as four string arrays through
`testing.TestRegistry.run_filtered` / `list_filtered`
(`crates/baml_cli/src/test_command.rs:1167-1214` → `registry.baml:249-282`).

### 1.5 Lazy testsets: excluded groups are never even executed

`testset` bodies are **collector closures** (`TestSetBody = (TestCollector) -> void`,
`crates/baml_builtins2/baml_std/testing/types.baml:52`), expanded on demand by
`expand_testset_registry` (`registry.baml:304-325`).

Before expanding, the selector layer proves the subtree can't contribute:
`subtree_excluded` (`registry.baml:644-659`) returns true when a plain-substring exclude already occurs
in `"<testset id>::"`, or when a trailing-`*` glob matches that prefix; `subtree_may_be_selected`
(`registry.baml:676-689`) gates the collector call at `registry.baml:714-733`.

This is load-bearing for live tests: **an excluded `live` testset's body never runs**, so it never
constructs clients, never reads env, never makes a call. Regression coverage:
`crates/baml_cli/tests/test_profiles_e2e.rs:167-224` (a `throw` inside an excluded testset must not fire)
and `:226-272`.

### 1.6 Adoption today: zero

```
grep -rn "\[test\]" --include=baml.toml  /Users/aaron/projects/baml  → no hits
grep -rln "test.profiles" --include=baml.toml /Users/aaron/projects/baml → no hits
```
The profile machinery exists, is documented in the CLI after-help, and has a dedicated e2e suite
(`crates/baml_cli/tests/test_profiles_e2e.rs`, 401 lines), but **no project in the monorepo uses it**.
This migration would be its first consumer.

### 1.7 Verified experiment (this is not theory)

Scratch project `<scratchpad>/tp` with:

```toml
[package]
name = "tp"
[test]
default = "unit"
[test.profiles.unit]
args = ["-x", "::live::"]
[test.profiles.live]
args = ["-i", "::live::"]
```

```baml
test "always" { assert.is_true(true) }

function PingO(q: string) -> string { client "openai/gpt-4o-mini"        prompt `One word. ${q}` }
function PingA(q: string) -> string { client "anthropic/claude-haiku-4-5" prompt `One word. ${q}` }
function PingG(q: string) -> string { client "google/gemini-2.5-flash"    prompt `One word. ${q}` }

testset "live" {
  if (baml.env.get("OPENAI_API_KEY")    != null) { test "openai"    { assert.is_true(PingO("hi").length() > 0) } } else { null }
  if (baml.env.get("ANTHROPIC_API_KEY") != null) { test "anthropic" { assert.is_true(PingA("hi").length() > 0) } } else { null }
  if (baml.env.get("GOOGLE_API_KEY")    != null) { test "google"    { assert.is_true(PingG("hi").length() > 0) } } else { null }
}
```

Observed results with `target/debug/baml-cli`:

| Command | Result |
|---|---|
| `baml-cli test --list` (default profile) | `Selected 1 test(s)` → `root.demo::always` only. Live subtree pruned pre-expansion. |
| `baml-cli test --list --no-profile` | same 1 test (collector registered nothing without keys). |
| `baml-cli test --profile live` (no keys) | `Finished no tests selected`, **exit code 5**. |
| `OPENAI_API_KEY=sk-x baml-cli test --profile live` | `PASS root.demo::live::openai_live`, exit 0. |
| `infisical run -- … baml-cli test --profile live` | `PASS …::openai`, `PASS …::anthropic`, `PASS …::google` — `3 passed, 0 failed, 3 total in 2s`. |

Two model-id notes discovered while doing this (both live-verified against the real APIs):
- `anthropic/claude-3-5-haiku-latest` → **404 not_found_error**. `anthropic/claude-haiku-4-5` works.
- `google/gemini-2.0-flash` → **404**, "no longer available". `google/gemini-2.5-flash` works.
- `google-ai/…` is **not** a valid shorthand prefix. The compile-time table is
  `crates/baml_compiler2_ast/src/lower_cst.rs:744-753`: only `openai`, `anthropic`, `google`,
  `claude-code`. Anything else is error `E0010` (`lower_cst.rs:794-803`).

### 1.8 Verdict + smallest change

**Grouping + opt-in already exists. Nothing needs building in the compiler or CLI.**

Smallest change for this migration:

1. Add to `crates/baml_tests/baml_src/baml.toml` (currently only `[package] name = "baml_tests"`,
   `crates/baml_tests/baml_src/baml.toml:1-2`):

   ```toml
   [test]
   default = "offline"

   [test.profiles.offline]
   args = ["-x", "::live::"]

   [test.profiles.live]
   args = ["-i", "::live::"]
   ```

2. Put every live provider test under a `testset "live"` (nested testsets fine: `live::openai`,
   `live::anthropic`, …) inside a namespace under `crates/baml_tests/baml_src/`.
3. Guard each leaf's *registration* on `baml.env.get("<KEY>") != null` so a `--profile live` run
   without that provider's key silently registers nothing rather than failing.

Two sharp edges worth designing around:

- **Exit code 5 on an empty selection** (`crates/baml_cli/src/lib.rs:69`, `:93-94`;
  emitted at `test_command.rs:429`, `:728-729`, `:951-952`). If *all* keys are missing,
  `--profile live` exits 5 (non-zero). Either always register one always-passing
  "preconditions" leaf inside `testset "live"`, or have the runner script tolerate 5.
- **Tests run unbounded-parallel by default**: `run_testset` with no runner calls
  `run_children_parallel`, which `spawn`s every child at once
  (`registry.baml:483-492`, `:537-542`). Live provider tests will all fire simultaneously and can trip
  rate limits. Fix inside BAML: `testset "live" with testing.Sequential() { … }`
  (`crates/baml_builtins2/baml_std/testing/runners.baml:97-101`). For flaky live calls,
  `test "x" with testing.Retry(3) { … }` (`runners.baml:35-67`); `testing.Quorum(n, m)`
  (`runners.baml:1-33`) and `testing.PassRate(threshold)` (`runners.baml:69-95`) are also available.
  Both `with` forms verified working (`testset "runners" with testing.Sequential()` containing
  `test "a" with testing.Retry(3)` → both PASS).

There is **no "skip" outcome**: `Outcome = "pass" | "fail" | "error"`
(`crates/baml_builtins2/baml_std/testing/types.baml:4`). A test is skipped only by not being
registered or not being selected. If the migration wants a visible "skipped because no key" signal,
that would be new work (new `Outcome` variant threaded through `RunReport`/`TestReport`/
`FlatTestReport` in `types.baml:7-95`, the aggregation in `registry.baml:426-482`/`:837-941`, and the
Rust reporter). **Recommend not doing this** — a `log.warn` in the testset body plus `--list`
is enough (note: `log.warn` output requires `--logs warn` or higher; default is `off`,
`test_command.rs:112-123`).

### 1.9 Hard constraint: tests in `baml_std` do not run

`baml test` collects **only the `user` package**:
`crates/baml_cli/src/test_command.rs:584` calls `engine.collect_tests("user", …)`, and
`collect_tests` looks up `$init_test` for `"user"` versus `"{package}.$init_test"` otherwise
(`crates/bex_engine/src/lib.rs:3897-3906`). No caller ever passes a stdlib package name.

⇒ **Do not put `test` blocks inside `crates/baml_builtins2/baml_std/`** — they would compile into
`openai.$init_test` / `anthropic.$init_test` and never execute. Consistent with the current state:
`grep -rln '^test \|^testset ' crates/baml_builtins2/baml_std/` returns nothing.

All native-BAML provider tests must live in a **user** project. The right home is
`crates/baml_tests/baml_src/ns_<something>/`, which is compiled as `user` and executed by
`crates/baml_tests/tests/baml_src.rs:155-183` (`cargo run -p baml_cli -- test --from …/baml_src`).

---

## 2. Infisical

### 2.1 Configuration

- `/Users/aaron/projects/baml/.infisical.json` — `workspaceId: 63f942680b94e4248e89eb42`,
  `defaultEnvironment: "test"`, no branch mapping. It sits at the **monorepo root**, so
  `infisical run --` works from `baml_language/` and any deeper directory (verified: running from
  `baml_language/` injected all 115 secrets).
- `/Users/aaron/projects/baml/typescript2/app-website/.infisical.json` — same workspace, same `test` env.
- CLI on this machine: `/Users/aaron/.local/share/mise/installs/npm-infisical-cli/latest/bin/infisical`.

### 2.2 How the repo already uses it

- CI: `/Users/aaron/projects/baml/.github/workflows/integ-tests.yml:98-102` —
  `infisical/secrets-action@v1.0.9`, `method: oidc`, `identity-id: 5b66a909-…`, `env-slug: test`,
  `project-slug: gloo-infra-9-fkp`. Then `tools/bctl integ-tests --suite {python,typescript,ruby}`
  at `:103-113`.
- Local dev driver: `/Users/aaron/projects/baml/tools/build:122`, `:279-284`, `:308-317`, `:341`, `:363-364` —
  e.g. `BAML_LOG=info infisical run --env=dev -- uv run pytest -s tests/providers/…`.
  Note both `--env=dev` and `--env=test` appear; `test` is the default.
- Version bump script: `/Users/aaron/projects/baml/tools/bump-version.py:314` —
  `infisical run --env=test -- uv run pytest`.
- **The one `baml_language`-side consumer today**: the SSE recorder,
  `sdk_tests/harness/llm_recordings/tests/recordings.rs:17`, `:35-44` — its missing-key hint literally
  says *"maybe we should be running with Infisical? `infisical run -- cargo nextest run -p sdk_test_llm_recordings`"*.
  Documented in `sdk_tests/fixtures/llm_functions/recordings/README.md:39-50`.
- **No CI job runs `baml_language`'s Rust tests under Infisical.** The `cargo-tests.reusable.yaml`
  jobs and the `snapshot-tests` job (`.github/workflows/cargo-tests.reusable.yaml:1320-1373`) have no
  Infisical step. In particular `snapshot-tests` is the job that runs `-p baml_tests` — i.e. the job
  that runs `baml test` over `crates/baml_tests/baml_src` — **with no provider keys at all**. This is
  precisely why the live tests must default to *off*.

### 2.3 What keys are actually available (env `test`, 115 secrets)

Enumerated with `infisical secrets --env=test` (names only). Provider-relevant subset:

| Secret | Present? | Notes |
|---|---|---|
| `OPENAI_API_KEY` | ✅ (len 164) | live-verified against `gpt-4o-mini` |
| `ANTHROPIC_API_KEY` | ✅ (len 108) | live-verified against `claude-haiku-4-5` |
| `GOOGLE_API_KEY` | ✅ (len 39) | live-verified against `gemini-2.5-flash` |
| `VERTEX_API_KEY` | ✅ (len 53) | native `google` package has **no** Vertex path (§3.5) |
| `INTEG_TESTS_GOOGLE_APPLICATION_CREDENTIALS_CONTENT` | ✅ (2396 bytes) | service-account JSON, used by engine integ-tests |
| `AWS_ACCESS_KEY_ID` | ⚠️ **empty string** | |
| `AWS_SECRET_ACCESS_KEY` | ⚠️ **empty string** | |
| `AWS_PROFILE` | ✅ = `boundaryml-dev` | Bedrock relies on local SSO/profile creds, not Infisical |
| `AWS_REGION` | ✅ = `us-east-1` | |
| `AZURE_OPENAI_API_KEY`, `AZURE_OPENAI_DEPLOYMENT_ID`, `AZURE_OPENAI_RESOURCE_NAME` | ✅ | |
| `AZURE_AI_FOUNDRY_API_KEY`, `AZURE_AI_FOUNDRY_PROJECT_ENDPOINT` | ✅ | |
| `AZURE_CLIENT_ID` / `AZURE_CLIENT_SECRET` / `AZURE_TENANT_ID` | ✅ | |
| `GROQ_API_KEY`, `OPENROUTER_API_KEY`, `DEEPSEEK_API_KEY`, `DEEPSEEK_AZURE_API_KEY`, `MOONSHOT_API_KEY`, `HUGGING_FACE_KEY`, `AI_GATEWAY_API_KEY` | ✅ | OpenAI-compatible endpoints |
| `BAML_LOG` | ✅ = `info` | **not read by the new CLI** (no `BAML_LOG` hit anywhere in `crates/`) |

**Bedrock caveat**: a live AWS/Bedrock test cannot be driven from Infisical alone — the static keys are
blank and it falls back to `AWS_PROFILE=boundaryml-dev`, i.e. the developer's local
`~/.aws` SSO session. Plan Bedrock live coverage as developer-local-only, or add real keys to
Infisical first. (Engine's own integ-tests reference `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` /
`AWS_SESSION_TOKEN` in `/Users/aaron/projects/baml/integ-tests/baml_src/clients.baml`, so they hit the
same wall unless the runner has a session.)

### 2.4 The exact commands an agent should use

Offline (the default; no keys, what CI runs):
```bash
cd /Users/aaron/projects/baml/baml_language
target/debug/baml-cli test --from crates/baml_tests/baml_src
```

Live, opted in (run from the monorepo root or anywhere below it):
```bash
cd /Users/aaron/projects/baml
infisical run -- baml_language/target/debug/baml-cli test \
    --from baml_language/crates/baml_tests/baml_src \
    --profile live --logs info
```

Live, one provider only:
```bash
infisical run -- baml_language/target/debug/baml-cli test \
    --from baml_language/crates/baml_tests/baml_src \
    --profile live -i "::live::openai::"
```

Discover ids without running anything:
```bash
baml_language/target/debug/baml-cli test --from … --profile live --list
```

Re-record the SSE fixtures (needs `OPENAI_API_KEY`):
```bash
infisical run -- cargo nextest run -p sdk_test_llm_recordings      # sdk_tests/…/recordings/README.md:41-50
INSTA_UPDATE=always infisical run -- cargo nextest run -p sdk_test_llm_recordings
```

Notes:
- Always `target/debug/baml-cli`, **never** `target/debug/baml` (stale wrapper). The
  "using the internal BAML toolchain binary directly is not recommended" warning is cosmetic;
  set `BAML_CLI_ALLOW_DIRECT=1` to silence it (as the e2e suites do —
  `crates/baml_tests/tests/baml_src.rs:178`, `crates/baml_cli/tests/test_profiles_e2e.rs:67`).
- When shelling out in tests, also isolate `BAML_HOME` and `BAML_CACHE_DIR`
  (`crates/baml_tests/tests/baml_src.rs:161-181`, `crates/baml_cli/tests/test_profiles_e2e.rs:59-77`),
  otherwise the CLI writes `<project>/.baml/cache` into the source tree that concurrent snapshot
  tests scan.

---

## 3. Existing provider tests: where they live and what they look like

### 3.1 `sys_llm`'s own tests — all in-crate, all offline

`crates/sys_llm/` has **no `tests/` directory**; every test is a `#[cfg(test)] mod tests` inside `src/`.
24 such modules, ~**319** `#[test]` functions:

| File | `#[cfg(test)]` at | `#[test]` count |
|---|---|---|
| `crates/sys_llm/src/types/output_format.rs` | :873 | 71 |
| `crates/sys_llm/src/stream_accumulator.rs` | :324 | 28 |
| `crates/sys_llm/src/build_request/google.rs` | :310 | 28 |
| `crates/sys_llm/src/build_request/openai/chat_completions.rs` | :323 | 27 |
| `crates/sys_llm/src/build_request/anthropic.rs` | :292 | 21 |
| `crates/sys_llm/src/specialize_prompt/transformations.rs` | :336 | 21 |
| `crates/sys_llm/src/lib.rs` | :983 | 19 |
| `crates/sys_llm/src/build_request/openai/responses.rs` | :260 | 15 |
| `crates/sys_llm/src/parse_response/google.rs` | :181 | 13 |
| `crates/sys_llm/src/resolve_media.rs` | :363 | 12 |
| `crates/sys_llm/src/parse_response/openai/chat_completions.rs` | :301 | 12 |
| `crates/sys_llm/src/parse_response/anthropic.rs` | :185 | 11 |
| `crates/sys_llm/src/parse_response/bedrock.rs` | :138 | 10 |
| `crates/sys_llm/src/parse_response/mod.rs` | :166 | 10 |
| `crates/sys_llm/src/parse_response/openai/responses.rs` | :153 | 6 |
| `crates/sys_llm/src/specialize_prompt/mod.rs` | :62 | 5 |
| `crates/sys_llm/src/baml_std.rs`, `auth_request/bedrock.rs` | :367, :214 | 3 each |
| `crates/sys_llm/src/build_request/openai/images.rs` | :120 | 2 |
| `crates/sys_llm/src/build_request/mod.rs`, `parse_response/openai/images.rs` | :286, :59 | 1 each |
| (`auth_request/mod.rs:135`, `auth_request/vertex.rs:378`, `build_request/bedrock.rs:415` also have modules) | | |

Dev-dependencies are only `rsa` and `tokio` (`crates/sys_llm/Cargo.toml:52-55`) — **no wiremock, no
reqwest**. These are pure unit tests over request-building and response-parsing, with hand-rolled
in-module fakes for env/IO (e.g. `crates/sys_llm/src/auth_request/mod.rs:255-260` fakes
`OPENAI_API_KEY`/`ANTHROPIC_API_KEY` lookups; `crates/sys_llm/src/build_request/mod.rs:1581`,
`:1726-1760`, `:1829`, `:1952` fake the Vertex token exchange and
`GOOGLE_GENAI_USE_VERTEXAI` env flips).

⇒ **Every one of these is expressible as a native BAML `test` block** (they need no network and no
mock server). That's the bulk of the migration's test work.

### 3.2 Rust request-shape tests against the *native* providers (the parity template)

`crates/baml_tests/tests/structured_prompt_requests.rs` (99 lines) is the closest existing analogue of
what the migration needs. Header: *"Provider request builders consume the structural `ai.Prompt`
produced by an LLM spec. These tests stay offline and inspect only the serialized body."*
(`:1-2`).

Shape (`:7-33`): build a BAML source string with an LLM function, call `<Fn>$spec()` to get the
prompt template, construct an `ai.ModelTurnInput`, call the provider's internal render function, and
`serde_json::from_str` the `.body`.

Provider entry points it exercises:
- `openai.internal.openai_render(client, input)` — `:38-41`; defined at
  `crates/baml_builtins2/baml_std/openai/ns_internal/responses.baml:117`.
- `anthropic.internal._anthropic_request(client, input, false)` — `:60-63`; defined at
  `crates/baml_builtins2/baml_std/anthropic/ns_internal/messages.baml:177`.
- `google.internal.google_render(client, input)` — `:82-85`; defined at
  `crates/baml_builtins2/baml_std/google/ns_internal/gemini.baml:215`.

Assertions: role splitting (`input[0].role == "system"`), Anthropic's `system` block extraction,
Gemini's `systemInstruction` / `contents` mapping (`:44-56`, `:66-77`, `:87-98`).

**This suite could be rewritten in native BAML** — `baml.json.path<T>` / `path_or<T>` already exist
and are exercised in BAML at `crates/baml_tests/baml_src/ns_provider_stdlib/provider_stdlib.baml:11-43`.
Recommend porting it as part of the migration rather than growing it in Rust.

Related: `crates/baml_tests/tests/env.rs:100-135` asserts that a runtime-constructed
`openai.OpenAiClient` / `anthropic.AnthropicClient` defaults its `api_key` from
`OPENAI_API_KEY` / `ANTHROPIC_API_KEY` — via `std::env::set_var` + `openai.internal.openai_render`
(`env.rs:117`). Not a network test.

### 3.3 Wiremock request-capture tests (the legitimate Rust-only category)

Only three suites in `crates/baml_tests/tests/` use wiremock (`grep -rln wiremock crates/`):

- `crates/baml_tests/tests/prompt_tag_e2e.rs` (185 lines) — starts a `MockServer`, mounts
  `POST /responses` returning a hand-built OpenAI Responses **SSE** body
  (`:68-80`), points a `openai.OpenAiClient.new(base_url = …)` at it (`:22-32`), drives
  `Greet$stream("World").final()` (`:98-121`), then reads `server.received_requests()` and asserts the
  rendered prompt reached the wire (`:123-132`). Second case asserts `ctx.output_format` for a class
  return reaches the streaming request (`:137+`).
- `crates/baml_tests/tests/streaming_sse_primitives.rs` (508 lines) — raw `baml.http.fetch_sse` /
  `SseStream.next()` / `.close()` against a mock server (`:1-40`).
- `crates/baml_tests/tests/http.rs` (346 lines) — header explicitly states the reason it stays in Rust:
  *"Tests here use insta snapshots (bytecode and/or traceback text), which cannot be expressed in BAML."*
  (`http.rs:3-4`).

Also `crates/bex_engine/tests/cancellation.rs` uses wiremock.

### 3.4 Native-BAML provider/HTTP tests — the preferred style, already proven

Two suites in `crates/baml_tests/baml_src/` were **converted from the Rust wiremock versions** and use
an in-process HTTP server written entirely in BAML:

- `crates/baml_tests/baml_src/ns_http_server/http_server.baml` — header at `:1-8`:
  *"End-to-end `baml.http.Server` tests, converted from crates/baml_tests/tests/http.rs."*
  12 `test` blocks (`:32`, `:49`, `:67`, `:92`, `:118`, `:141`, `:152`, `:184`, `:211`, `:230`, `:257`, `:278`).
  Pattern: `baml.http.Server.bind("127.0.0.1:0")` → `spawn { server.serve(handler) }` →
  `baml.http.fetch/send` at `server.addr` → `task.cancel()` (`:13-33`).
- `crates/baml_tests/baml_src/ns_streaming_sse_primitives/streaming_sse_primitives.baml` — header at
  `:1-11`; 8+ tests driving `baml.http.fetch_sse` against the BAML server
  (`:25-48`, `:52-78`, `:82-102`, `:108-134`, `:142-167`, `:173-200`, `:205-225`, `:230-251`).

**Critical authoring gotcha, stated in both headers**: *"Each case is a top-level `function` … (needed to
avoid the test-block local-boxing VM bug)"* (`http_server.baml:3-5`,
`streaming_sse_primitives.baml:6`). Write the logic as a top-level `function`, then a thin `test`
block that asserts on its return value. Plan on this for every migrated test.

⇒ **A wiremock server is not required to test request shape or SSE parsing.** A BAML `baml.http.Server`
handler can capture the request (method/url/headers/body) and return a canned provider response, all
inside a native `test`. This is the strongest recommendation in this document.

### 3.5 The keyless replay harness (SSE recordings)

- `sdk_tests/fixtures/llm_functions/baml_src/ns_replay/replay_server.baml` — a fake LLM endpoint in
  BAML. `ReplayHandler.handle` (`:11-51`) serves a recorded SSE body for **any POST**
  (`:17-47`, deliberately path-agnostic so "other providers' paths also work", `:18-19`), streaming
  one event at a time with a 10 ms gap (`:38-43`) — the comment at `:21-28` explains why buffered
  single-write responses mask host-driven streaming bugs. Entry points
  `replay_serve_until_shutdown` (`:58-72`) and `replay_serve_detached` (`:76-93`).
- Rust/host side: `sdk_tests/crates/rust/llm_functions/customizable/replay_harness.rs:1-15`
  ("Keyless replay harness … with **no `OPENAI_API_KEY`**"), exposing `BAML_REPLAY_BASE_URL`.
- Recorder: `sdk_tests/harness/llm_recordings/tests/recordings.rs:1-51`. Whether it hits the network is
  decided **only by insta state** (`:12-17`): a capture runs when `<name>.snap.sse` is missing or
  `INSTA_UPDATE` forces it; otherwise it validates the checked-in payload offline. So a normal run —
  even under `infisical run --` — makes **no** network calls (`README.md:28-33`).
- Redaction: the recorder strips `authorization`-family headers and filters OpenAI key prefixes; CI
  greps the directory and fails on a hit (`README.md:59-64`); insta filter `sk-[A-Za-z0-9_-]+ →
  [REDACTED-KEY]` at `recordings.rs:79`.

This is the model to copy if the migration wants *recorded real provider responses* (Anthropic
`message_start`/`content_block_delta`, Gemini `generateContent` streams, Bedrock event-stream) without
keys at test time. Note `sdk_test_*` crates are **excluded from every cargo CI job**
(`.github/workflows/cargo-tests.reusable.yaml:125`, `:227`, `:322`, `:438`) and run in a dedicated
matrix (`:507-626`).

### 3.6 The corpus runner and the CI jobs that will police this

- `crates/baml_tests/tests/baml_src.rs` — compiles the whole `baml_src/` corpus
  (`:57-67`), snapshots bytecode **per namespace** (`:110-153`), and runs the CLI test suite
  (`:155-183`). Stdlib packages are excluded from the bytecode snapshots by
  `baml_builtins2::stdlib_package_names()` (`:118-127`), so adding an `openai`/`anthropic`/`google`
  builtin package "never floods these snapshots" (`:120-123`).
- CI: `snapshot-tests` (`.github/workflows/cargo-tests.reusable.yaml:1320-1373`) runs
  `cargo insta test --test-runner nextest -p baml_tests -p baml_cli -p baml_lsp2_actions
  --all-features --unreferenced=reject` and then **fails on any uncommitted `.snap`/`.snap.new`**
  (`:1374-1385`). No Infisical step ⇒ the whole `baml_tests` corpus must pass offline.
- The three cargo-test jobs `--exclude baml_tests` precisely because `snapshot-tests` owns it
  (`cargo-tests.reusable.yaml:117-129`, `:316-326`, `:432-447`).

### 3.7 Compiler-phase / snapshot tests (auto-generated)

`crates/baml_tests/projects/<name>/*.baml` folders are auto-discovered by `build.rs` and turned into
per-phase snapshot tests with no test code written (`crates/baml_tests/README.md:1-49`). Existing
provider-adjacent fixtures: `projects/compiles/o1_allowed_roles/o1_clients.baml`,
`projects/diagnostic_errors/client_option_types/client_option_types.baml`,
`projects/compiles/config_model_string/model_string.baml`. Use these for **diagnostic** coverage of new
client options (e.g. "unknown option `foo` on `BedrockClient`"), not for behavior.

---

## 4. Parity reference: where the engine's live provider tests live

The old compiler's provider behavior is asserted by the language-client integration suites at the
monorepo root, not in Rust:

- `/Users/aaron/projects/baml/integ-tests/baml_src/clients.baml` — the client declarations, referencing
  `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GOOGLE_API_KEY`, `VERTEX_API_KEY`,
  `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`/`AWS_SESSION_TOKEN`, `AZURE_OPENAI_*`,
  `GROQ_API_KEY`, `OPENROUTER_API_KEY`, `TOGETHER_API_KEY`, `DEEPSEEK_AZURE_API_KEY`,
  `INTEG_TESTS_GOOGLE_APPLICATION_CREDENTIALS_CONTENT`.
- `/Users/aaron/projects/baml/integ-tests/python/tests/` — `test_functions.py`, `test_request.py`,
  `test_client_option.py`, `test_client_response.py`, `test_media_inputs.py`, `test_errors.py`,
  and `tests/providers/{test_openai_responses.py, test_aws_video_request.py}`.
- TypeScript sibling: `/Users/aaron/projects/baml/integ-tests/typescript/tests/providers/*.test.ts`
  (invoked from `/Users/aaron/projects/baml/tools/build:308-317`).
- Driver: `/Users/aaron/projects/baml/integ-tests/run-tests.sh`, and in CI
  `tools/bctl integ-tests --suite {python,typescript,ruby}` after the Infisical OIDC step
  (`.github/workflows/integ-tests.yml:98-113`).
- `test_request.py` in particular is the engine's request-shape oracle — the natural parity checklist
  when porting `sys_llm/src/build_request/**` to BAML.

Note the engine's `integ-tests` job only runs on `workflow_dispatch`/`workflow_call`/pushes to
`sam/integ-tests-ci` (`.github/workflows/integ-tests.yml:5-8`) and its actual test steps for the
matrix are commented out (`:35-52`) — the live suite is effectively manual today. Same posture is
appropriate for the new live group.

---

## 5. Gaps and recommendations

1. **Nothing to build for grouping.** Adopt `[test] default` + `[test.profiles.{offline,live}]` in
   `crates/baml_tests/baml_src/baml.toml`. First user of the feature in the monorepo (§1.6).
2. **Double-gate**: profile exclusion (`-x "::live::"`) prunes the subtree before its collector runs
   (`registry.baml:644-659`), and env-conditional registration inside the testset means a
   `--profile live` run without a key registers nothing. Use both.
3. **Handle exit 5** for an all-keys-missing `--profile live` run (`crates/baml_cli/src/lib.rs:93-94`)
   — simplest fix is one always-registered "live preconditions" leaf.
4. **Serialize live tests** with `testset "live" with testing.Sequential()`
   (`runners.baml:97-101`); default execution is unbounded-parallel `spawn`
   (`registry.baml:483-492`). Consider `testing.Retry(2)` on individual live leaves
   (`runners.baml:35-67`).
5. **Never put `test` blocks in `baml_std`** — `collect_tests("user")` means they'd never run
   (`test_command.rs:584`, `bex_engine/src/lib.rs:3897-3906`).
6. **Prefer the BAML `baml.http.Server` mock over wiremock** for request-shape and SSE tests; the
   pattern is proven at `ns_http_server/http_server.baml` and
   `ns_streaming_sse_primitives/streaming_sse_primitives.baml`, and the replay server at
   `ns_replay/replay_server.baml:11-51` shows it works as a fake *provider*. Keep Rust only for
   insta-snapshot assertions (bytecode / traceback text), per `http.rs:3-4`.
7. **Remember the local-boxing VM bug**: put test bodies in top-level `function`s and keep `test`
   blocks to assertions (`http_server.baml:3-5`).
8. **Port `structured_prompt_requests.rs` to BAML** as the first migrated test, establishing the
   `<Fn>$spec()` + `ai.ModelTurnInput` + `baml.json.path<T>` idiom in-language.
9. **Bedrock live coverage is blocked on credentials**: `AWS_ACCESS_KEY_ID` /
   `AWS_SECRET_ACCESS_KEY` are empty in Infisical `test`; only `AWS_PROFILE=boundaryml-dev` +
   `AWS_REGION=us-east-1` are set (§2.3). Either add keys to Infisical or scope Bedrock live tests
   to developer machines with an SSO session. (The native stdlib has no `aws`/`bedrock` package at
   all today — `ls crates/baml_builtins2/baml_std/` → `ai anthropic assert baml boundary claude_code
   google log openai reflect testing`.)
10. **Vertex live coverage has keys but no code path**: `VERTEX_API_KEY` and
    `INTEG_TESTS_GOOGLE_APPLICATION_CREDENTIALS_CONTENT` exist, but the native `google` package has
    no Vertex/aiplatform/service-account handling
    (`grep -n 'vertex\|aiplatform\|GOOGLE_APPLICATION' crates/baml_builtins2/baml_std/google/**` →
    no hits), while `crates/sys_llm/src/auth_request/vertex.rs` and
    `crates/sys_llm/src/build_request/google.rs:138-170` implement it. Vertex tests must wait on the
    port.
11. **Model ids in live tests must be current**: `claude-3-5-haiku-latest` and `gemini-2.0-flash`
    both 404 as of 2026-08-12. Use `anthropic/claude-haiku-4-5` and `google/gemini-2.5-flash`.
    Provider shorthands are limited to `openai`, `anthropic`, `google`, `claude-code`
    (`crates/baml_compiler2_ast/src/lower_cst.rs:744-753`) — anything else needs an explicit
    `X.Client.new(base_url = …)` expression.
12. **Optional future work (only if the plan demands it)**: a real `"skip"` `Outcome`
    (`types.baml:4`) threaded through `RunReport`/`TestReport`/`FlatTestReport` and the Rust
    reporter. Recommend deferring — the profile + registration gate already produces the desired
    behavior, and `--list` shows exactly what got selected.
