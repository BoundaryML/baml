// Provider authentication primitives, implemented in Rust (`crates/sys_auth`)
// because they need cryptography the stdlib does not have: RS256 JWT signing
// for Google Cloud, HMAC-SHA256 for AWS SigV4.
//
// These are the seams the built-in `google.*` / `aws.*` clients wrap
// (`google.internal.gcp_access_token`, `aws.internal.sign_request`); everything
// else about a provider request — URLs, bodies, headers, parsing — is BAML.
// Internal to the `ai` package; not intended to be called directly.

/// Mints a Google Cloud OAuth2 access token for `scope`, returning the bare
/// token (the caller builds the `Bearer` header).
///
/// `credentials_json` is an inline credential JSON document — service account,
/// authorized user, workload identity federation, or impersonated service
/// account: the same documents `GOOGLE_APPLICATION_CREDENTIALS` accepts. It is
/// used as-is: a broken document raises instead of silently falling back.
/// `null` runs the Application Default Credentials chain
/// (`GOOGLE_APPLICATION_CREDENTIALS`, the well-known ADC config file, the
/// gcloud config file, then the GCE metadata server).
///
/// Tokens are cached process-wide until shortly before expiry, so repeated
/// calls with the same credentials and scope do not re-mint.
function _gcp_access_token(
    credentials_json: string?,
    scope: string,
) -> string throws baml.errors.Io | baml.errors.AccessError {
    $rust_io_function
}

/// Resolves the Google Cloud project id from `credentials_json` when it carries
/// one, else from the google-auth chain (`GOOGLE_CLOUD_PROJECT` /
/// `GCLOUD_PROJECT`, the ADC and gcloud config files, the metadata server).
/// `null` means "not discoverable"; the caller decides whether that is fatal.
function _gcp_project_id(
    credentials_json: string?,
) -> string? throws baml.errors.Io | baml.errors.AccessError {
    $rust_io_function
}

/// Resolves the quota/billing project for the `x-goog-user-project` header,
/// matching google-auth's `Credentials.apply`: `GOOGLE_CLOUD_QUOTA_PROJECT`
/// always wins, then the credential document's `quota_project_id`.
function _gcp_quota_project_id(
    credentials_json: string?,
) -> string? throws baml.errors.Io | baml.errors.AccessError {
    $rust_io_function
}

/// SigV4-signs `request` for `service` and returns it with the signature
/// headers applied (`authorization`, `x-amz-date`, and `x-amz-security-token`
/// for session credentials).
///
/// The signature covers the request's method, URL, headers, and body, so this
/// must be the LAST mutation of the request — anything added afterwards
/// invalidates it.
///
/// Credentials come from `access_key_id` + `secret_access_key` (+ optional
/// `session_token`) when both halves are present, else from the AWS provider
/// chain: environment, shared config/credentials files (static keys,
/// `credential_process`, SSO cache), the container endpoint, then EC2 IMDS.
function _aws_sign_request(
    request: baml.http.Request,
    service: string,
    region: string?,
    profile: string?,
    access_key_id: string?,
    secret_access_key: string?,
    session_token: string?,
) -> baml.http.Request throws baml.errors.Io | baml.errors.AccessError {
    $rust_io_function
}

/// Resolves the AWS region: `region` when set, else `AWS_REGION`,
/// `AWS_DEFAULT_REGION`, or the active profile's `region` key. `null` means
/// "not discoverable".
function _aws_resolve_region(
    region: string?,
    profile: string?,
) -> string? throws baml.errors.Io | baml.errors.AccessError {
    $rust_io_function
}
