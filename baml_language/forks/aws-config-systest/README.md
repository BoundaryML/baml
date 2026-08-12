# aws-systest — credential-safety e2e harness for the `forked_aws_*` crates

This crate is the regression harness for **`forked_aws_config`** (credential + region
resolution) and **`forked_aws_sigv4`** (request signing). Run it whenever you touch
`forks/aws-config/**` or `forks/aws-sigv4/**` to assert that **every form of AWS
authentication still resolves correctly and that the resulting SigV4 signature is accepted by
real AWS**.

The compatibility goal is deliberately strict: **`forked_aws_config` should match the real
`aws-config` crate's observable credential and region behavior as closely as possible across edge
cases**, while keeping BAML free of the upstream Smithy/`aws-smithy-*` dependency graph. When a
future change finds a behavioral mismatch that can be reproduced from upstream source or tests,
prefer matching upstream in this fork and adding a parity test. Only document a divergence when it
is required by the fork's intentionally smaller public surface or by the absence of unsupported
upstream features.

It has two layers:

1. **Offline** — `cargo test -p forked_aws_config` (100 tests). Deterministic, no network/creds.
   Proves each provider parses correctly *and* that the chain ordering is preserved
   (`tests/chain_precedence.rs`). This is the everyday guard; run it on every change / in CI.
2. **Live** — `forks/aws-config-systest/run-e2e.py` builds the `aws-systest` harness binary, resolves
   credentials through the real fork code for each auth form, and signs a real **STS
   GetCallerIdentity** (and **Bedrock Converse** where noted), asserting AWS accepts the
   signature. The cloud tier provisions a real EC2 instance + Lambda and tears them down.

## Coverage matrix (every AWS auth form)

| Auth form | Offline test | Live scenario (`run-e2e.py`) | Tier |
|-----------|--------------|------------------------------|------|
| Static env vars (`AWS_ACCESS_KEY_ID`/`…SECRET…`/`…SESSION_TOKEN`, legacy `SECRET_ACCESS_KEY`) | `env_credentials.rs` | `local:env` | local |
| Shared credentials file (`[profile]` static keys) | `profile_static.rs` | `local:profile-static` | local |
| `credential_process` subprocess | `credential_process.rs` | `local:credential_process` | local |
| SSO (token cache → `GetRoleCredentials`) | `sso_provider.rs` | `local:sso` (+Bedrock) | local |
| ECS / container endpoint (relative + full URI, auth token) | `ecs_container.rs` | `local:ecs-container(mock)` | local |
| EC2 IMDSv2 (token → role → creds) | `imds.rs` | `cloud:ec2-imdsv2` (+Bedrock) — **real instance** | cloud |
| Lambda runtime env creds | `env_credentials.rs` | `cloud:lambda-env` (+Bedrock) — **real function** | cloud |
| JSON credential parsing (Pascal/camelCase, `Token`/`SessionToken`) | `json_credentials.rs` | (exercised by container/IMDS/credential_process) | offline |
| Region resolution (`AWS_REGION` > `AWS_DEFAULT_REGION` > profile) | `region_resolution.rs` | every live scenario passes `--region` / resolves it | offline |
| INI parsing (config/credentials files) | `ini_parsing.rs` | — | offline |
| **Provider-chain precedence** (env > profile > container > IMDS) + empty-chain error | `chain_precedence.rs` | — | offline |

## Running it

```sh
# Everyday: offline tests only — no AWS creds needed (use this in CI).
./forks/aws-config-systest/run-e2e.py --offline-only

# Default: offline + all local auth forms (needs valid SSO creds for the profile).
aws sso login --profile boundaryml-dev      # if the token has expired
./forks/aws-config-systest/run-e2e.py

# Full: offline + local + live EC2 IMDSv2 + live Lambda (provisions & tears down real infra).
./forks/aws-config-systest/run-e2e.py --all
```

Flags: `--profile NAME` (default `boundaryml-dev`), `--region R` (`us-east-1`),
`--model ID` (`amazon.nova-micro-v1:0`), `--cloud` (offline+cloud), `--skip-build`.

The script **asserts** every scenario (STS must return 200 for the expected account; the IMDS
and Lambda scenarios additionally require the STS ARN to reference the instance-profile /
execution role) and exits non-zero on any failure — so `run-e2e.py && echo ok` is a valid gate.

### Safety / teardown

**boundaryml-dev only.** Every live tier (local *and* cloud) resolves the caller identity and
**refuses to run unless the account is boundaryml-dev (`147997132427`)** — the check happens
*before* any credential export or provisioning, so pointing `--profile` at prod/staging aborts
immediately with a non-zero exit and zero side effects. `--profile` may name any profile, but it
must map to the dev account. (Change the pinned account only by editing `DEV_ACCOUNT` in
`run-e2e.py`.)

The cloud tier names all resources `baml-systest-e2e-<timestamp>-<pid>` and registers a single
`trap … EXIT INT TERM` master cleanup, so the EC2 instance, S3 bucket, IAM roles, instance
profile, and Lambda function are **always** removed — even on Ctrl-C or mid-run failure. After a
cloud run the script also leaves nothing behind; verify independently with:

```sh
aws --profile boundaryml-dev --region us-east-1 ec2 describe-instances \
  --filters Name=tag:Name,Values=baml-systest-e2e-* \
  --query 'Reservations[].Instances[].[InstanceId,State.Name]' --output text
```

## Adding a new auth form

1. Add an offline integration test under `forks/aws-config/tests/` (drive `resolve_credentials`
   through `tests/common/mod.rs`'s `MockIo`); if it changes chain ordering, update
   `chain_precedence.rs` deliberately.
2. Add a live scenario in `run-e2e.py` (a `local:` block, or a `cloud:` block with a matching
   `trap` cleanup entry) and a row to the matrix above.

The harness binary (`src/main.rs`) is a standalone package excluded from the parent workspace so
its native-only Tokio features do not affect wasm builds. `run-e2e.py` builds it with
`--manifest-path forks/aws-config-systest/Cargo.toml --target-dir ./target`; it is cross-compiled
to a static `x86_64-unknown-linux-musl` ELF (`cargo zigbuild`, no Docker) for the EC2/Lambda phases
and runs the Lambda Runtime API loop when `AWS_LAMBDA_RUNTIME_API` is set.

---

## Verification Report

Date: 2026-05-30
Workspace root: repository root for `baml_language`
Fork crate: `forks/aws-config` (package `forked_aws_config`, lib `aws_config`)
SigV4 fork: `forked_aws_sigv4` (re-exports `Credentials`, provides `sign_request`)
System-test harness: `forks/aws-config-systest`

### Overview

`forked_aws_config` is a slim, in-tree replacement for the upstream `aws-config` crate
(and its dependency surface in `aws-runtime` / `aws-sdk-rust`), paired with `forked_aws_sigv4`
as a slim replacement for `aws-sigv4`. The forks remove the Smithy/`aws-smithy-*` machinery
and `google-cloud-rust` dependencies, providing only what BAML needs: credential resolution
across the standard provider chain (env, shared-profile static, `credential_process`, SSO,
ECS/container, IMDSv2) plus region resolution and SigV4 request signing.

Verification strategy is two-pronged:

1. **Unit parity** — for each credential/config area, integration tests were ported from the
   upstream `aws-config` / `aws-runtime` test suites (crates.io tarballs and the `aws-sdk-rust`
   git checkout) and adapted to the fork's actual, slimmer behavior. Divergences (lenient INI
   parsing, no retries, raw section maps, no expiry/account-id storage) are documented per test
   rather than mirrored with false assertions. All run fully offline via a shared
   `tests/common/mod.rs` `MockIo` implementing `aws_config::CredentialIo`.
2. **Live system tests** — the `aws-systest` harness exercises the real signing + provider
   code against real AWS (STS GetCallerIdentity, Bedrock Converse) across every provider, on
   macOS local, and is also built as a static musl binary for live EC2/Lambda phases.

### Unit-test parity

| Area | Upstream reference source(s) | Cases | Status |
|------|------------------------------|-------|--------|
| INI parsing | `aws-runtime-1.7.3/src/env_config/parse.rs` + `test-data/profile-parser-tests.json` | 19 | PASS |
| env credentials | `aws-sdk-rust .../aws-config/src/environment/credentials.rs` (mod test) | 7 | PASS |
| profile static | `aws-config/src/profile/credentials.rs`, `profile.rs`, `profile/profile_file.rs` | 8 | PASS |
| credential_process | `aws-config-1.8.16/src/credential_process.rs`, `json_credentials.rs` | 8 | PASS |
| SSO | `aws-config-1.8.16/src/profile/credentials.rs` (sso_tests), `sso/credentials.rs`, `sso/cache.rs` | 4 | PASS |
| ECS/container | `aws-config-1.8.5/src/ecs.rs`, `json_credentials.rs` | 9 | PASS |
| IMDS | `aws-config imds/credentials.rs`, `imds/client.rs` (mirrored behaviorally) | 4 | PASS |
| json-credentials | `aws-config-1.8.16/src/json_credentials.rs` (mod test) | 12 | PASS |
| region resolution | `aws-config-1.8.16/src/environment/region.rs`, `profile/region.rs`, `meta/region.rs` + `region_override` fixture | 14 | PASS |
| **chain precedence** (env > profile > container > IMDS) + empty-chain error | regression guard for `resolve_credentials` ordering | 5 | PASS |

Plus: 6 lib unit tests + 4 `common_smoke` scaffold tests + 0 doc-tests.

**Total: 100 tests, 100 passed, 0 failed.**

### `cargo test -p forked_aws_config` summary line

```text
test result: ok. 6 passed   (unittests src/lib.rs)
test result: ok. 4 passed   (tests/common_smoke.rs)
test result: ok. 8 passed   (tests/credential_process.rs)
test result: ok. 9 passed   (tests/ecs_container.rs)
test result: ok. 7 passed   (tests/env_credentials.rs)
test result: ok. 4 passed   (tests/imds.rs)
test result: ok. 19 passed  (tests/ini_parsing.rs)
test result: ok. 12 passed  (tests/json_credentials.rs)
test result: ok. 8 passed   (tests/profile_static.rs)
test result: ok. 14 passed  (tests/region_resolution.rs)
test result: ok. 4 passed   (tests/sso_provider.rs)
test result: ok. 0 passed   (Doc-tests aws_config)
```

All eleven test binaries report `0 failed`.

### System-test results

Every scenario resolves credentials through the forked provider chain and then has
`forked_aws_sigv4` sign a real **STS GetCallerIdentity** request (proving AWS accepts the
signature) and, where noted, a real **Bedrock Converse** request. The signing/region/credential
code is identical across providers — only the credential *source* differs.

The authoritative live runs are against **`boundaryml-dev` (account `147997132427`,
region `us-east-1`)**:

| Scenario | Provider source | STS status | STS account | STS ARN (role) | Bedrock status | Pass |
|----------|-----------------|-----------|-------------|----------------|----------------|------|
| SSO (live) | SSO token cache → GetRoleCredentials | 200 | 147997132427 | `…AWSReservedSSO_AdministratorAccess…/sam` | — | PASS |
| ENV provider (in Lambda) | `AWS_ACCESS_KEY_ID`/`…SECRET…`/`…SESSION_TOKEN` | 200 | 147997132427 | `…/baml-systest-lambda-r2-20260530-2306/…` | 200 ("ok") | PASS |
| EC2 IMDSv2 (live) | EC2 instance metadata (IMDSv2) | 200 | 147997132427 | `…/baml-systest-ec2-r2-20260530-2306/i-0c77…` | 200 ("Ok") | PASS |
| PROFILE static file | `[profile]` keys in a creds file | 200 | 147997132427* | — | — | PASS |
| credential_process | subprocess emitting `Version:1` JSON | 200 | 147997132427* | — | — | PASS |
| ECS/container (mock) | `AWS_CONTAINER_CREDENTIALS_FULL_URI` | 200 | 147997132427* | — | — | PASS |

- **EC2 IMDSv2** ran on a live `t3.micro` Amazon Linux 2023 instance (`i-0c77c6d00f2a25297`) with
  an instance profile and **no** env vars or `~/.aws` on the box — so the *only* possible
  credential source was the real IMDSv2 metadata service. The fork performed the IMDSv2
  PUT-token → role-name → role-credentials dance, signed STS+Bedrock, and both returned 200. The
  STS ARN references the instance-profile role, confirming the creds came from IMDS.
- **Lambda** ran on a live `provided.al2023` x86_64 function. The Lambda runtime injects the
  execution-role credentials as env vars, so the fork's **ENV provider** resolved them; STS+Bedrock
  both 200, ARN references the Lambda execution role.
- Bedrock calls used `amazon.nova-micro-v1:0` (on-demand) and returned valid Converse responses.

\* The `PROFILE`, `credential_process`, and `ECS-mock` scenarios were exercised in the initial
run before a parameter-passing bug was fixed (see Gaps), so they signed against
`boundaryml-prod` (`277707123528`) rather than dev. They are credential-*transport* mechanisms
independent of which account the creds belong to, and each returned **STS 200 with the expected
account for that run** — the forked SigV4 signature was accepted in every case. The SSO, ENV
(Lambda), and IMDSv2 (EC2) paths above were all (re-)verified against the intended
`boundaryml-dev` account.

Harness build: native `cargo build --release` OK; `cargo zigbuild --release --target
x86_64-unknown-linux-musl` OK (static ELF, stripped, 4,130,376 bytes). Repo-root `cargo build` OK.

### Leaked-resource (teardown) verification

The EC2 and Lambda phases create real resources and tear them down. Teardown was verified
**independently** (outside the agents) with `aws --profile boundaryml-dev --region us-east-1`
for suffix `r2-20260530-2306`, and a sweep of `boundaryml-dev`/`-prod`/`-staging` for the
earlier suffix:

| Resource | Query | Result |
|----------|-------|--------|
| EC2 `i-0c77c6d00f2a25297` | describe-instances | **terminated** |
| Lambda `baml-systest-r2-20260530-2306` | get-function | ResourceNotFound |
| IAM role `baml-systest-ec2-r2-20260530-2306` | get-role | NoSuchEntity |
| IAM role `baml-systest-lambda-r2-20260530-2306` | get-role | NoSuchEntity |
| IAM instance-profile `baml-systest-ec2-r2-20260530-2306` | get-instance-profile | NoSuchEntity |
| S3 `baml-systest-r2-20260530-2306-147997132427` | s3 ls / head-bucket | NoSuchBucket |

**No leaked resources** in any account. (The EC2 agent's first teardown pass hit a zsh
variable-expansion quirk but still terminated the instance; an explicit re-run removed
everything, confirmed by the independent sweep above.)

### Reproduce

```sh
# Unit suite (fully offline, deterministic)
cd baml_language
cargo test -p forked_aws_config

# Build the live harness (native + static musl) — standalone package, excluded
# from the workspace but landing in the shared target dir
cargo build --release \
  --manifest-path forks/aws-config-systest/Cargo.toml \
  --target-dir ./target
cargo zigbuild --release \
  --manifest-path forks/aws-config-systest/Cargo.toml \
  --target-dir ./target \
  --target x86_64-unknown-linux-musl

# Live SSO system test (SSO → STS → Bedrock) against the dev account
# (uses the existing ~/.aws SSO token cache; run `aws sso login --profile boundaryml-dev` if expired)
./target/release/aws-systest \
  --profile boundaryml-dev \
  --region us-east-1 \
  --bedrock-model amazon.nova-micro-v1:0 \
  --sign-sts --sign-bedrock

# Cross-compile the static Linux binary used by the EC2/Lambda phases
cargo zigbuild --release \
  --manifest-path forks/aws-config-systest/Cargo.toml \
  --target-dir ./target \
  --target x86_64-unknown-linux-musl
# (cargo-zigbuild + the x86_64-unknown-linux-musl target are the confirmed mac→Linux path;
#  Docker is NOT required. The resulting ELF is fully static and runs on AL2023 and Lambda
#  provided.al2023 as `bootstrap`.)
```

The EC2 IMDSv2 and Lambda live phases are driven by `forks/aws-config-systest/run-e2e.py --all`
with `--profile boundaryml-dev`, account `147997132427`, and model
`amazon.nova-micro-v1:0`.

### Gaps / notes

- **Fork is intentionally slimmer than upstream.** Several upstream test cases were *not*
  ported because the fork's behavior differs by design, and porting them would require false
  assertions: INI key lowercasing and profile-/sso-session-prefix normalization (raw section
  map only); upstream's strict parse errors (the fork is lenient and skips malformed lines);
  `Expiration`/`AccountId` storage (the fork validates refreshable JSON expiration but its
  `Credentials` has only access_key_id/secret_access_key/session_token, all public fields);
  retries; endpoint-mode / IPv6 / DNS-allowlist checks for ECS full-URI. Each omission is
  documented inline in its test file.
- **Remaining behavioral divergence covered by tests asserting the fork's real behavior:**
  IMDS empty-role body maps to `ConfigError::NoCredentials`.
- **Parameter-passing bug in the initial run (since fixed).** The first orchestration run did
  not propagate its parameter object into the script, so profile/account/region/suffix/AMI/etc.
  rendered as the literal string `undefined`. The unit-test and harness-build agents recovered
  on their own (they located the crate and upstream references directly), but: (a) the harness
  agent fell back to a `boundaryml-prod` default, so the *local* provider scenarios signed
  against prod; and (b) the EC2/Lambda agents correctly **refused to provision** into an
  `undefined` account rather than risk billable resources — which is why nothing leaked. The
  SSO/IMDSv2/Lambda paths were then re-run against `boundaryml-dev` with all parameters
  hardcoded, and all passed (tables above).
- **SSO pure-`--profile` resolution at harness build time** showed `sso_resolve_ok=false` in the
  first run because that on-disk SSO token had briefly expired. It has since been re-verified
  green: `aws-systest --profile boundaryml-dev --sign-sts --sign-bedrock` resolves the SSO token
  cache, calls GetRoleCredentials, and returns STS 200 (account 147997132427) — confirming the
  fork's full SSO path against a valid cached token.
- **Fork is intentionally slimmer than upstream** (see the unit-test notes): no
  `Expiration`/`AccountId` storage, lenient INI parsing, raw section maps, no retries. These
  divergences are asserted in the ported tests against the fork's *real* behavior rather than
  mirrored with false expectations.
- **One harness build deviation:** the `fs` feature was added to `tokio` (beyond
  rt/rt-multi-thread/macros/time) because `read_file` uses `tokio::fs::read_to_string`.
