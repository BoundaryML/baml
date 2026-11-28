# BCS API

The BCS client as BAML hosts use it: the CLI, bridges, packed programs, and telemetry delivery. The client is `bcs-sdk`, which BCS publishes; this crate re-exports its modules and adds what is BAML's own: the environment BAML reads and how it words a failure.

- `endpoint`: the Boundary API endpoint, `BOUNDARY_API_URL` before a configured URL before BCS's own.
- `store`: the saved logins under `$BAML_HOME` (default `~/.baml`).
- `diagnostics`: credential-safe wording of each failure for BAML's users; a host chooses the operation's outcome.

From `bcs-sdk`:

- `auth`: device login and endpoint selection.
- `api_keys`: read-only API-key identity and permission introspection.
- `Store`: an AWS-style JSON login cache under `<home>/login/cache/`, with one owner-only file per canonical Boundary API endpoint. CLI, bridges and packed programs share the same file format. Files contain the persistent session credential and caller profile; writes replace the file atomically. These files are plaintext and readable by processes running as the same OS user.
- `credentials`: redacted credentials, optional request targets, and shared process-local access-token state.
- `query`: blocking NDJSON queries and cancellation.
- `telemetry`: asynchronous upload preparation and heartbeat, plus protobuf uploads authorized by the upload plan.
- `upload`: the upload contract and its envelope, `btel-core`'s (`btel_core::upload`).

Windows cache operations use an owner-only empty `<endpoint-hash>.lock` file to coordinate readers, atomic replacement and logout across hosts. The lock file stays after logout and contains no credentials. Locks cover only local file operations, never HTTP requests. A blocked cache-directory path is a storage error, not an absent login.

Create `Authentication::shared` with the canonical endpoint and original credential, then put it in a `RequestAuthorization` with the operation's optional `Target`. Authentication is shared by endpoint and original credential; targets remain specific to each operation.

For a saved `bdry_session_` credential, the first successful operation can return `Boundary-Access-Token` and `Boundary-Access-Expires-At`. Query and telemetry retain that `bdry_access_` token only in process memory and reuse it across authorized operations. Expiry selects the original credential again; a cached token refused with HTTP 401 is invalidated and the operation retries once before admission. HTTP 403 does not trigger renewal. Secret API keys authenticate directly.

Concurrent bootstrap and renewal requests share one in-flight operation across blocking query callers and async telemetry callers. Waiting requests reuse its token, or receive its renewal failure. Transport failures and cancellation release the waiters; later operations can try again. HTTP requests and waiter notifications run outside the cache lock.

Credentials are sent only to their configured API endpoint. Upload bodies retain their protobuf format and use the upload plan's authorization.

## Embedded telemetry in artifacts

`baml pack main --embed-telemetry` and `baml generate --embed-telemetry` read exactly the manifest-selected environment variable. It must contain a nonempty Unicode API key other than `local`. The builder makes one read-only `POST /v1/auth/verify` request with `{"fullResponse":true}` and requires the complete `200 OK` introspection response. It checks the key's owning organization slug and project name against `boundary.project`, then requires exactly one environment ingestion target, no query or other operation permissions, and resolved environment metadata matching the expected name and owning parent IDs. Ambient `BOUNDARY_PROJECT` does not change these assertions. The artifact retains the exact credential and selected endpoint; verification or compilation failure prevents output. Embedding creates no credential or authorization grant and sends no source, bytecode, or digest. Configure the endpoint and publisher policy in `baml.toml`:

```toml
[boundary]
project = "publisher/app"
api_url = "https://api.prod.bcs.boundaryml.com"

[pack]
on_initial_telemetry_failure = "warn"
initial_telemetry_warning_message = "Telemetry unavailable; continuing."
http_bodies = false

[pack.build]
embed_telemetry_api_key_from_env_var = "BAML_WRAPPER_BOUNDARY_API_KEY"
embed_telemetry_env = "staging"

[pack.env_var_names]
BOUNDARY_PROJECT = "ACME_BOUNDARY_PROJECT"
BOUNDARY_API_KEY = "ACME_BOUNDARY_API_KEY"
BAML_TELEMETRY = "ACME_TELEMETRY"
BAML_GC_GROWTH = "ACME_GC_GROWTH"         # optional, see [pack.gc]

[pack.gc]
growth = 2
min_growth = "4MiB"

[bridge]
on_initial_telemetry_failure = "warn"

[bridge.build]
embed_telemetry_api_key_from_env_var = "BAML_BRIDGE_BOUNDARY_API_KEY"
embed_telemetry_env = "staging"
```

`[pack.gc]` is not telemetry: it bakes the heap's collection policy into the packed binary (`growth`, a multiple of the data that survived a collection, `0.25` to `1000`, default `4`; `min_growth`, a size of at least `"1MiB"`, default `"8MiB"`). `BAML_GC_GROWTH` and `BAML_GC_MIN_GROWTH`, or the names `[pack.env_var_names]` gives them, override it at startup. There is no `[bridge.gc]`: generated SDKs read the variables. See [Memory in ENV_VARS.md](../../ENV_VARS.md#memory).

Pack and bridge settings are independent. An unset, empty, or non-Unicode source variable makes the pack or generate command fail before writing the artifact. Without `--embed-telemetry`, no API key is embedded.

An embedded credential fixes the destination by default. `env_var_names` accepts only name strings; publishers can rename both destination variables together. With named bindings, a project and API key select caller-owned ingestion; a project alone uses the caller's saved login and personal environment; a key alone lets the API infer its bound ingestion environment. The publisher's embedded key is never reused for a caller-selected destination. With no binding values set, the embedded credential remains selected. Without an embedded credential, omitted bindings accept the standard Boundary variables and saved login.

New packed programs and generated bridges bind `BAML_TELEMETRY` by default; a name string remaps it, and only that variable controls recording. It accepts `off`, `low`, `medium` (default), or `high`, ignoring case and whitespace. `false`, `0`, and `no` mean off; `true`, `1`, `yes`, and `on` keep the default. Empty means unset; other values disable recording. Off disables local recording and cloud requests, including with embedded credentials. The control is read at engine startup. If a destination owns `BAML_TELEMETRY` (including Windows case aliases), use a distinct recording binding such as `ACME_TELEMETRY` for opt-out.

A permitted API-key variable set to `local` selects local recording; empty destination variables are unset. `BOUNDARY_API_URL` overrides the baked endpoint, including embedded artifacts; an invalid URL records locally instead of stopping an enabled artifact.

Newly packed binaries and generated bridges default to `warn` for initial telemetry failures; publishers can explicitly choose `abort`, `warn`, or `ignore`. Authorization starts immediately in the background. Initial failure cancels BAML execution for `abort`; `warn` prints the configured message and continues; `ignore` continues silently. Bridge `abort` returns an operation error while the host process stays running. Ordinary caller-configured CLI cloud recording also defaults to `warn` and continues without cloud telemetry after an initial failure. After initial success, later telemetry failures remain nonfatal under every policy.

At `medium` and `high`, network spans record request and response bodies, streamed events included. `http_bodies = false` keeps every body out of them at every level, for an API whose responses carry secrets such as session tokens. They still record the method, the sanitized URL and headers, the status, timing and whether the request failed, but not its error, which can quote the body. Model calls are network spans too, so this also keeps rendered prompts and raw model responses out; token usage is still recorded. Function spans record no error value either, whatever their trace options or function policy ask, because an error can carry a body, such as a model error's `raw_body`; a failed span still records that it failed, and a failed future does not carry the error that escaped it. Their arguments and results are recorded as their trace policy says. Omission or `true` records bodies.

`--embed-telemetry` is a boolean opt-in and accepts no value. `[pack.build]` and `[bridge.build]` have identical settings: `embed_telemetry_api_key_from_env_var` selects the credential variable, and `embed_telemetry_env` names the expected environment, in any ASCII case: `Production` and `production` are one environment. Pack reads `[pack.build]`; generate reads `[bridge.build]`. Both require `boundary.project = "org_handle/project_name"`. Without the flag the builder does not read the embedding credential or verify it.

The old `[pack]` and `[bridge]` `telemetry_environment` settings must move to their respective build tables as `embed_telemetry_env`. The old credential-variable setting `build.embed_telemetry_env` becomes `build.embed_telemetry_api_key_from_env_var` in both tables. Runtime bindings remain in `[pack.env_var_names]` and `[bridge.env_var_names]`.

After credential selection, embedded API keys and caller-supplied API keys use the same authentication cache, target construction, heartbeat, upload preparation, and delivery path. An omitted runtime project is inferred from the API key’s ingestion permission. The build-time expected environment is an assertion, not a separate runtime routing mechanism. Revocation, later permission changes, unresolved destinations, and conflicting caller constraints follow ordinary API-key errors and the configured runtime failure policy. No runtime request contains `buildId`.

Deploy BCS support for opt-in introspection and resolved environment metadata before shipping clients that embed credentials. `baml cloud auth status` continues using default verification with no request body and requires `204 No Content`. An older server returning 204 for `fullResponse: true` fails embedding. Artifacts with older minted build tokens must be rebuilt using ordinary API keys; there is no build registry compatibility path. Local recording source snapshots and their CAS references remain independent; cloud source-version attribution requires a separate design.
