# BCS API

Cloud wire contracts and native clients shared by the CLI, bridges, packed programs, and telemetry delivery.

- `auth`: device login, endpoint selection, and protected storage of the persistent session credential.
- `credentials`: redacted credentials, optional request targets, and shared process-local access-token state.
- `query`: blocking NDJSON queries and cancellation.
- `telemetry`: asynchronous upload preparation and heartbeat, plus protobuf uploads authorized by the upload plan.
- `wire` and `proto`: telemetry control contracts and the existing `cloud.proto` envelope.

Create `Authentication::shared` with the canonical endpoint and original credential, then put it in a `RequestAuthorization` with the operation's optional `Target`. Authentication is shared by endpoint and original credential; targets remain specific to each operation.

For a saved `bdry_session_` credential, the first successful operation can return `Boundary-Access-Token` and `Boundary-Access-Expires-At`. Query and telemetry retain that `bdry_access_` token only in process memory and reuse it across authorized operations. Expiry selects the original credential again; a cached token refused with HTTP 401 is invalidated and the operation retries once before admission. HTTP 403 does not trigger renewal. Secret API keys authenticate directly.

Concurrent bootstrap and renewal requests share one in-flight operation across blocking query callers and async telemetry callers. Waiting requests reuse its token, or receive its renewal failure. Transport failures and cancellation release the waiters; later operations can try again. HTTP requests and waiter notifications run outside the cache lock.

Credentials are sent only to their configured API endpoint. Upload bodies retain their protobuf format and use the upload plan's authorization.
