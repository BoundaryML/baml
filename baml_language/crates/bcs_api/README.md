# BCS API

Cloud wire contracts and native clients shared by the CLI, bridges, packed programs, and telemetry delivery.

- `auth`: device login and endpoint selection.
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

`baml pack main --embed-telemetry BAML_WRAPPER_BOUNDARY_API_KEY` and `baml generate --embed-telemetry BAML_WRAPPER_BOUNDARY_API_KEY` read the named environment variable at build time and embed its value as the telemetry API key. The variable must be set to a non-empty Unicode value. Embedding makes no cloud requests and does not read saved logins. It reads `BOUNDARY_API_KEY` only if that is the variable explicitly named. The resulting artifact contains the key itself; it does not need the source variable at runtime. The API key determines the ingestion destination. Configure the endpoint and runtime policy in `baml.toml`:

```toml
[boundary]
project = "publisher/app"
api_url = "https://api.prod.bcs.boundaryml.com"

[pack]
on_initial_telemetry_failure = "warn"
initial_telemetry_warning_message = "Telemetry unavailable; continuing."
http_bodies = false

[pack.env_var_names]
BOUNDARY_PROJECT = "ACME_BOUNDARY_PROJECT"
BOUNDARY_API_KEY = "ACME_BOUNDARY_API_KEY"
BAML_TELEMETRY = "ACME_TELEMETRY"

[bridge]
on_initial_telemetry_failure = "warn"
```

Pack and bridge settings are independent. An unset, empty, or non-Unicode source variable makes the pack or generate command fail before writing the artifact. Without `--embed-telemetry <ENV_VAR>`, no API key is embedded.

An embedded credential fixes the destination by default. A publisher can explicitly name both destination variables, or set both to `false`. With named bindings, a project and API key select caller-owned ingestion; a project alone uses the caller's saved login and personal environment; a key alone lets the API infer its bound ingestion environment. The publisher's embedded key is never reused for a caller-selected destination. With no binding values set, the embedded credential remains selected. Without an embedded credential, omitted bindings accept the standard Boundary variables and saved login.

The recording-level binding is independently explicit: omission or `false` freezes the default `medium`; a string names the accepted variable. That variable takes `off`, `low`, `medium`, or `high`, case-insensitive; `false`, `0`, and `no` also mean `off`, and `true`, `1`, `yes`, and `on` keep the default. Empty means unset, and any other value turns recording off. `off` starts no cloud requests. A permitted API-key variable set to `local` selects local recording; empty destination variables are unset. `BOUNDARY_API_URL` always overrides the baked endpoint, including fixed embedded artifacts; an invalid URL records locally instead of stopping the artifact.

Newly packed binaries and generated bridges default to `warn` for initial telemetry failures; publishers can explicitly choose `abort`, `warn`, or `ignore`. Authorization starts immediately in the background. Initial failure cancels BAML execution for `abort`; `warn` prints the configured message and continues; `ignore` continues silently. Bridge `abort` returns an operation error while the host process stays running. Ordinary caller-configured CLI cloud recording defaults to `abort` and exits nonzero on initial failure. After initial success, later telemetry failures remain nonfatal under every policy.

At `medium` and `high`, network spans record request and response bodies, streamed events included. `http_bodies = false` keeps every body out of them at every level, for an API whose responses carry secrets such as session tokens. They still record the method, the sanitized URL and headers, the status, timing and whether the request failed, but not its error, which can quote the body. Model calls are network spans too, so this also keeps rendered prompts and raw model responses out; token usage is still recorded. Function spans are unchanged: they record arguments, results and errors as their trace policy says, and an error can carry a body, such as a model error's `raw_body`. Omission or `true` records bodies.
