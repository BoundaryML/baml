# BCS API

Cloud wire contracts and native clients shared by the CLI, bridges, packed programs, and telemetry delivery.

- `auth`: device login and endpoint selection.
- `store` (exposed through `auth::Store`): an AWS-style JSON login cache under `$BAML_HOME/login/cache/` (default `~/.baml/login/cache/`), with one owner-only file per canonical Boundary API endpoint. CLI, bridges and packed programs share the same file format. Files contain the persistent session credential and caller profile; writes replace the file atomically. These files are plaintext and readable by processes running as the same OS user.
- `credentials`: redacted credentials, optional request targets, and shared process-local access-token state.
- `query`: blocking NDJSON queries and cancellation.
- `telemetry`: asynchronous upload preparation and heartbeat, plus protobuf uploads authorized by the upload plan.
- `wire` and `proto`: telemetry control contracts and the existing `cloud.proto` envelope.

Create `Authentication::shared` with the canonical endpoint and original credential, then put it in a `RequestAuthorization` with the operation's optional `Target`. Authentication is shared by endpoint and original credential; targets remain specific to each operation.

For a saved `bdry_session_` credential, the first successful operation can return `Boundary-Access-Token` and `Boundary-Access-Expires-At`. Query and telemetry retain that `bdry_access_` token only in process memory and reuse it across authorized operations. Expiry selects the original credential again; a cached token refused with HTTP 401 is invalidated and the operation retries once before admission. HTTP 403 does not trigger renewal. Secret API keys authenticate directly.

Concurrent bootstrap and renewal requests share one in-flight operation across blocking query callers and async telemetry callers. Waiting requests reuse its token, or receive its renewal failure. Transport failures and cancellation release the waiters; later operations can try again. HTTP requests and waiter notifications run outside the cache lock.

Credentials are sent only to their configured API endpoint. Upload bodies retain their protobuf format and use the upload plan's authorization.

## Embedded telemetry in artifacts

`baml pack --embed-telemetry` and `baml generate --embed-telemetry` provision a fresh public ingestion credential. Choose an existing environment with `--telemetry-environment=staging`, or set a producer default:

```toml
[boundary]
project = "publisher/app"
api_url = "https://api.cloud.boundaryml.com"

[pack]
telemetry_environment = "staging"
on_initial_telemetry_failure = "warn"
initial_telemetry_warning_message = "Telemetry unavailable; continuing."

[pack.env_var_names]
BOUNDARY_PROJECT = "ACME_BOUNDARY_PROJECT"
BOUNDARY_API_KEY = "ACME_BOUNDARY_API_KEY"
BAML_TELEMETRY = "ACME_TELEMETRY"

[bridge]
telemetry_environment = "staging"
```

Pack and bridge settings are independent. The environment default is used only when `--embed-telemetry` is supplied. Build-time authentication uses `BOUNDARY_API_KEY` or the saved login for the selected endpoint. The server must require project administrator access or an API key with permission to mint for that target.

An embedded credential fixes the destination by default. A publisher can explicitly name both destination variables, or set both to `false`. With named bindings, a project and API key select caller-owned ingestion; a project alone uses the caller's saved login and personal environment; a key alone lets the API infer its bound ingestion environment. The publisher's public token is never reused for a caller-selected destination. With no binding values set, the embedded credential remains selected. Without an embedded credential, omitted bindings accept the standard Boundary variables and saved login.

The recording-level binding is independently explicit: omission or `false` freezes the default `medium`; a string names the accepted variable. Levels are `off`, `low`, `medium`, and `high`. `off` starts no cloud requests. A permitted API-key variable set to `local` selects local recording. `BOUNDARY_API_URL` always overrides the baked endpoint, including fixed embedded artifacts.

The initial failure policy is `abort` (default), `warn`, or `ignore`. Authorization starts immediately in the background. Initial failure cancels execution for `abort`; `warn` prints the configured message and continues; `ignore` continues silently. After initial success, later revocation disables telemetry and execution continues under every policy. Bridge failure returns an operation error to its host.

### Build provisioning contract

The native client implements this proposed contract. Artifact integration tests exercise it against a mock API; the BCS server must implement minting and public-credential admission before embedded cloud telemetry can be used with a deployed service.

```http
POST /v1/build-tokens
Authorization: Bearer bdry_secret_<builder credential>
Idempotency-Key: <fresh UUID for this invocation>
Content-Type: application/json

{
  "target": {
    "kind": "handle",
    "project": "publisher/app",
    "environment": "staging"
  },
  "bytecodeDigest": {
    "version": 1,
    "algorithm": "sha256",
    "value": "<64 lowercase hex characters>"
  }
}
```

The operation also accepts a user session or its cached access grant. Permissions are checked for this exact project/environment. Each invocation uses a new UUID and gets a fresh independently revocable token. A retry with the same UUID and request recovers the original result; a conflicting request fails. The response is:

```json
{
  "buildId": "build_123",
  "tokenId": "token_456",
  "token": "bdry_public_<random 256-bit credential>",
  "productId": "product_789",
  "destination": {
    "orgId": "org_123",
    "projectId": "project_456",
    "environmentId": "environment_789"
  },
  "bytecodeDigest": {
    "version": 1,
    "algorithm": "sha256",
    "value": "<same requested digest>"
  }
}
```

The credential is opaque, valid until revoked, and limited to ingestion into this destination. Its server registration associates the credential hash with the build, destination, and active state. The returned digest must exactly match the request. The artifact contains the public credential and registration; private minting credentials stay in the build process.

Heartbeat and upload preparation carry the same authorization, with build context alongside the ordinary request target:

```http
POST /v1/recordings/<recording-id>/heartbeat
Authorization: Bearer bdry_public_<embedded credential>
Content-Type: application/json

{
  "buildId": "build_123",
  "target": {
    "orgId": "org_123",
    "projectId": "project_456",
    "environmentId": "environment_789"
  },
  "producer_session_id": "<UUID>",
  "liveness_sequence": 1,
  "state": "RUNNING"
}
```

Upload preparation adds `buildId` and `target` to the existing prepare body. Upload bytes keep the protobuf envelope and use presigned capabilities. Caller overrides use their own credential and target, with the public build context omitted. The data plane must validate public registrations using its periodically refreshed authorization configuration and enforce matching build/destination, ingest-only authority, and active state.

### Local build check

Fingerprint version 1 uses SHA-256 over a domain separator, a length-prefixed artifact payload, and Borsh-encoded publisher policy. Generated bridges hash the complete versioned program artifact. Packed binaries hash the versioned dispatch envelope, including its program, entry points and output format. Versioned artifact bytes include the BAML toolchain fingerprint. Embedded credential/registration fields are removed before hashing, so minting another token can identify the same build.

Hosts compare this digest before starting the artifact. This catches accidental bytecode/credential transplantation in the standard toolchain. The build ID associates reporting with a distributed build; it is not attestation that an HTTP caller executed that build. Embedded credentials are intentionally extractable and distributable.
