# BAML sccache wrapper

This crate builds the native `baml-sccache` `RUSTC_WRAPPER`. It maps BAML's explicit R2 credentials to the AWS environment names consumed by sccache and, for local macOS development, can retrieve those credentials from Infisical without writing them to disk or adding them to the parent process environment.

The canonical explicit override names are `BAML_SCCACHE_R2_ACCESS_KEY_ID` and `BAML_SCCACHE_R2_SECRET_ACCESS_KEY`. `BAML_SCCACHE_R2_ACCESS_KEY` remains a deprecated compatibility alias for the first name. `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` are downstream sccache names, not the canonical BAML configuration interface. A complete explicit BAML pair always wins and prevents an Infisical request. A partial pair disables R2 rather than mixing credential sources. GitHub CI continues to pass the canonical BAML pair and only maps it to the sccache child; CI never performs Infisical lookup.

The wrapper reads the project ID from the repository's checked-in `.infisical.json`, uses environment slug `gh-ci`, secret path `/`, and cloud base URL `https://app.infisical.com`, and requests only `BAML_SCCACHE_R2_ACCESS_KEY_ID` and `BAML_SCCACHE_R2_SECRET_ACCESS_KEY`, concurrently, through the official `infisical` Rust SDK 0.0.3. `.infisical.json` remains the project source of truth; its `defaultEnvironment` does not override the cache-specific `gh-ci` selection.

`.envrc` is the source of truth for `SCCACHE_BUCKET`, region, endpoint, and the local-versus-CI key prefix. It marks loaded environments with `BAML_SCCACHE_DIRENV_LOADED=1`. When the native binary starts without that marker, it locates the repository and re-runs itself through `direnv exec`, which loads `.envrc` without serializing or printing the environment. The wrapper copies those non-secret settings to sccache only when it has a complete credential pair; local fallback removes them so a secretless build cannot accidentally attempt R2.

## Human-session setup

The official Rust SDK 0.0.3 supports Universal Auth but does not provide a documented API for consuming the human session stored by `infisical login`, and a CLI login is not automatically visible to the SDK. The wrapper deliberately does not read the CLI credential database or macOS Keychain and does not invoke `infisical export` or any token-printing command.

Prefer a short-lived human token over a machine identity credential on a laptop. Allow the repository `.envrc` once, then after `infisical login`, use the CLI's documented token handoff in a one-command environment so the token is never exported into the long-lived development shell. The binary loads `.envrc` through direnv even when the calling shell has not loaded it:

```bash
direnv allow .envrc
INFISICAL_TOKEN="$(infisical user get token --plain)" baml_language/target/debug/baml-sccache --start-server
```

The token exists only in the wrapper process. The wrapper removes `INFISICAL_TOKEN` before spawning sccache, fetches the R2 pair into redacting/zeroizing memory, maps it to `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` only on that child, and starts the long-lived sccache server. Later rustc wrapper calls reuse the server without another Infisical request. If the human session expires, log in again and rerun the command after `sccache --stop-server`; logging out or revoking the session makes the next lookup fall back to local cache.

Set `BAML_SCCACHE_INFISICAL=0` to opt out. Automatic lookup is limited to non-CI macOS processes. `BAML_SCCACHE_INFISICAL=1` opts another local platform into the same flow for testing, but never overrides CI detection. Missing CLI setup, a missing or expired token, network failure, missing project access, or an absent secret produces a concise cache-disabled reason and uses host-local sccache; no secret values are printed. A running sccache server is reused whether it was configured for R2 or local storage.

The current SDK gap means a developer must explicitly bridge the already authenticated CLI session into the wrapper. The alternative documented SDK flow is Universal Auth, but that would place a machine identity client secret on each laptop, so this integration intentionally does not enable it.

## Manual smoke test

This smoke test is opt-in and must only be run when a valid human CLI session already exists:

```bash
sccache --stop-server >/dev/null 2>&1 || true
INFISICAL_TOKEN="$(infisical user get token --plain)" baml_language/target/debug/baml-sccache --start-server
baml_language/target/debug/baml-sccache --show-stats
```

Confirm that the command succeeds and that subsequent compilation increases cache activity. Neither command prints the R2 credentials. Do not add shell tracing, print the environment, or paste the token into a command argument.
