// Google Cloud authentication for the Vertex AI client.
//
// Minting an OAuth2 token needs RS256 JWT signing, which the stdlib has no
// primitive for, so it is one of the two Rust seams that survive the
// sys_llm-native migration (`crates/sys_auth`, exposed as `ai.internal.*`).
// Everything else — reading a credentials file, `GOOGLE_APPLICATION_CREDENTIALS`
// / `GOOGLE_APPLICATION_CREDENTIALS_CONTENT`, location and project options — is
// ordinary IO and stays in BAML, in the client.

/// Mints a Google Cloud OAuth2 access token for `scope`, returning the bare
/// token (build `Bearer ${token}` yourself). Raises if no usable credential is
/// found.
///
/// `credentials_json` is an inline credential JSON document — service account,
/// authorized user, workload identity federation, or impersonated service
/// account: the same documents `GOOGLE_APPLICATION_CREDENTIALS` accepts. It is
/// used as-is, so a broken document raises rather than silently falling back.
/// `null` runs the Application Default Credentials chain
/// (`GOOGLE_APPLICATION_CREDENTIALS`, the well-known ADC config file, the
/// gcloud config file, then the GCE metadata server).
///
/// Tokens are cached process-wide until shortly before expiry, so calling this
/// per request is cheap.
function gcp_access_token(credentials_json: string?, scope: string) -> string {
    ai.internal._gcp_access_token(credentials_json, scope)
}

/// Resolves the project id from `credentials_json` when it carries one, else
/// from the google-auth chain (`GOOGLE_CLOUD_PROJECT` / `GCLOUD_PROJECT`, the
/// ADC and gcloud config files, the GCE metadata server). `null` means "not
/// discoverable" — the caller reports it.
function gcp_project_id(credentials_json: string?) -> string? {
    ai.internal._gcp_project_id(credentials_json)
}

/// Resolves the quota/billing project for the `x-goog-user-project` header,
/// matching google-auth's `Credentials.apply`: `GOOGLE_CLOUD_QUOTA_PROJECT`
/// always wins, then the credential document's `quota_project_id`.
function gcp_quota_project_id(credentials_json: string?) -> string? {
    ai.internal._gcp_quota_project_id(credentials_json)
}
