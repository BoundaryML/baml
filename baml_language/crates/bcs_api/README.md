# BCS API

Cloud wire contracts and native clients shared by the CLI, bridges, packed programs, and telemetry delivery.

- `auth`: device login and endpoint selection.
- `api_keys`: read-only API-key identity and permission introspection.
- `store` (exposed through `auth::Store`): an AWS-style JSON login cache under `$BAML_HOME/login/cache/` (default `~/.baml/login/cache/`), with one owner-only file per canonical Boundary API endpoint. CLI, bridges and packed programs share the same file format. Files contain the persistent session credential and caller profile; writes replace the file atomically. These files are plaintext and readable by processes running as the same OS user.
- `credentials`: redacted credentials, optional request targets, and shared process-local access-token state.
- `query`: blocking NDJSON queries and cancellation.
- `telemetry`: asynchronous upload preparation and heartbeat, plus protobuf uploads authorized by the upload plan.
- `wire` and `proto`: telemetry control contracts and the existing `cloud.proto` envelope.

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

[bridge]
on_initial_telemetry_failure = "warn"

[bridge.build]
embed_telemetry_api_key_from_env_var = "BAML_BRIDGE_BOUNDARY_API_KEY"
embed_telemetry_env = "staging"
```

Pack and bridge settings are independent. An unset, empty, or non-Unicode source variable makes the pack or generate command fail before writing the artifact. Without `--embed-telemetry`, no API key is embedded.

An embedded credential fixes the destination by default. A publisher can explicitly name both destination variables, or set both to `false`. With named bindings, a project and API key select caller-owned ingestion; a project alone uses the caller's saved login and personal environment; a key alone lets the API infer its bound ingestion environment. The publisher's embedded key is never reused for a caller-selected destination. With no binding values set, the embedded credential remains selected. Without an embedded credential, omitted bindings accept the standard Boundary variables and saved login.

The recording-level binding is independent of destination selection and embedded credentials. New packed programs and generated bridges bind `BAML_TELEMETRY` by default. A string in `[pack.env_var_names]` or `[bridge.env_var_names]` selects the accepted recording variable; that variable alone controls recording, so `ACME_TELEMETRY=off` disables even with `BAML_TELEMETRY=high`, and `ACME_TELEMETRY=medium` remains enabled even with `BAML_TELEMETRY=off`. Unset custom variables keep the baked detail level. Explicit `false` freezes detail, but retains an off-only veto through `BAML_TELEMETRY`; enabling/detail-changing/invalid values cannot unfreeze it. A destination binding that owns the name `BAML_TELEMETRY` excludes that implicit control: use a distinct explicit recording binding to offer opt-out. Destination variables are never parsed as levels.

An effective recording variable takes `off`, `low`, `medium`, or `high`, case-insensitive and trimmed; `false`, `0`, and `no` also mean `off`, and `true`, `1`, `yes`, and `on` keep the baked default. Empty means unset, and any other value disables recording for a bound variable. Frozen policies accept only the off aliases; all other values keep their baked level. Disabled recording starts no processor, file store, authorization, upload preparation, upload, heartbeat or retry worker, including when the caller has a saved login, API key or embedded credential, explicit spans, repeated calls or later shutdown. The control is sampled during engine startup; cached SDK engines retain that decision until replaced. Native application HTTP and execution remain available. Browser/Workers WASM bridges have no native BCS recording publisher or OS environment control.

A permitted API-key variable set to `local` selects local recording; empty destination variables are unset. `BOUNDARY_API_URL` always overrides the baked endpoint, including embedded artifacts; an invalid URL records locally instead of stopping an enabled artifact. An effective off choice bypasses runtime destination validation. Invalid publisher bindings fail generation/packing; malformed/nonstandard colliding recording bindings in deserialized policies fail closed without reading destination credentials as levels. A destination that owns the standard recording name keeps baked detail without an environment recording control, including case aliases on Windows for bridge metadata generated on a case-sensitive host. See [Runtime traces](../../ENV_VARS.md#runtime-traces) for the complete precedence table and storage rules.

The existing v1 generated metadata and packed Borsh policy layout are unchanged. New producers serialize the default recording binding explicitly. Legacy `recording_level_envvar = None` cannot distinguish omission from explicit `false`; both retain baked detail with the standard off-only veto, unless a destination owns that name. The bridge does not infer missing provenance from the copied original manifest. Normal bridge/toolchain matching and artifact ABI validation still apply; rebuild older independently shipped engines to receive the new runtime behavior.

Newly packed binaries and generated bridges default to `warn` for initial telemetry failures; publishers can explicitly choose `abort`, `warn`, or `ignore`. Authorization starts immediately in the background. Initial failure cancels BAML execution for `abort`; `warn` prints the configured message and continues; `ignore` continues silently. Bridge `abort` returns an operation error while the host process stays running. Ordinary caller-configured CLI cloud recording also defaults to `warn` and continues without cloud telemetry after an initial failure. After initial success, later telemetry failures remain nonfatal under every policy.

At `medium` and `high`, network spans record request and response bodies, streamed events included. `http_bodies = false` keeps every body out of them at every level, for an API whose responses carry secrets such as session tokens. They still record the method, the sanitized URL and headers, the status, timing and whether the request failed, but not its error, which can quote the body. Model calls are network spans too, so this also keeps rendered prompts and raw model responses out; token usage is still recorded. Function spans are unchanged: they record arguments, results and errors as their trace policy says, and an error can carry a body, such as a model error's `raw_body`. Omission or `true` records bodies.

`--embed-telemetry` is a boolean opt-in and accepts no value. `[pack.build]` and `[bridge.build]` have identical settings: `embed_telemetry_api_key_from_env_var` selects the credential variable, and `embed_telemetry_env` names the expected environment. Pack reads `[pack.build]`; generate reads `[bridge.build]`. Both require `boundary.project = "org_handle/project_name"`. Without the flag the builder does not read the embedding credential or verify it.

The old `[pack]` and `[bridge]` `telemetry_environment` settings must move to their respective build tables as `embed_telemetry_env`. The old credential-variable setting `build.embed_telemetry_env` becomes `build.embed_telemetry_api_key_from_env_var` in both tables. Runtime bindings remain in `[pack.env_var_names]` and `[bridge.env_var_names]`.

After credential selection, embedded API keys and caller-supplied API keys use the same authentication cache, target construction, heartbeat, upload preparation, and delivery path. An omitted runtime project is inferred from the API key’s ingestion permission. The build-time expected environment is an assertion, not a separate runtime routing mechanism. Revocation, later permission changes, unresolved destinations, and conflicting caller constraints follow ordinary API-key errors and the configured runtime failure policy. No runtime request contains `buildId`.

Deploy BCS support for opt-in introspection and resolved environment metadata before shipping clients that embed credentials. `baml auth status` continues using default verification with no request body and requires `204 No Content`. An older server returning 204 for `fullResponse: true` fails embedding. Artifacts with older minted build tokens must be rebuilt using ordinary API keys; there is no build registry compatibility path. Local recording source snapshots and their CAS references remain independent; cloud source-version attribution requires a separate design.
