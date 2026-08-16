// AWS authentication for the Bedrock client.
//
// SigV4 signing needs HMAC-SHA256, which the stdlib has no primitive for, so it
// is one of the two Rust seams that survive the sys_llm-native migration
// (`crates/sys_auth`, exposed as `ai.internal.*`). Building the request — URL,
// headers, body — stays in BAML.

/// Credential and region inputs for `sign_request` / `resolve_region`.
///
/// Each field mirrors a client option. Everything except `service` is optional;
/// unset fields fall through to the AWS provider chain.
class AwsSignOptions {
    /// Explicit region; `null` resolves from `AWS_REGION`, `AWS_DEFAULT_REGION`,
    /// or the active profile.
    region: string?,
    /// Named profile to read from the shared config/credentials files;
    /// `null` uses `AWS_PROFILE`, else `default`.
    profile: string?,
    /// Explicit access key. Only used together with `secret_access_key` — a
    /// lone key id falls through to the chain rather than signing with half an
    /// identity.
    access_key_id: string?,
    secret_access_key: string?,
    /// Session token for temporary credentials (STS/SSO), sent as
    /// `x-amz-security-token`.
    session_token: string?,
    /// The AWS service the signature is scoped to, e.g. `"bedrock"`.
    service: string,
}

/// SigV4-signs `req` and returns it with the signature headers applied
/// (`authorization`, `x-amz-date`, and `x-amz-security-token` for session
/// credentials). Raises if no credentials or no region can be resolved.
///
/// The signature covers the request's method, URL, headers, and body, so this
/// MUST be the last mutation of the request — a header or body change made
/// afterwards invalidates it and the call is rejected with a 403.
///
/// Credentials come from `opts.access_key_id` + `opts.secret_access_key` (+
/// optional `opts.session_token`) when both halves are present, else from the
/// AWS provider chain: environment, the shared config/credentials files (static
/// keys, `credential_process`, SSO cache), the container endpoint, then EC2
/// IMDS.
function sign_request(req: baml.http.Request, opts: AwsSignOptions) -> baml.http.Request {
    ai.internal._aws_sign_request(
        req,
        opts.service,
        opts.region,
        opts.profile,
        opts.access_key_id,
        opts.secret_access_key,
        opts.session_token,
    )
}

/// Resolves the AWS region: `region` when set, else `AWS_REGION`,
/// `AWS_DEFAULT_REGION`, or the active profile's `region` key. `null` means
/// "not discoverable" — the caller decides how to report it (endpoint
/// construction usually needs it before signing does).
function resolve_region(region: string?, profile: string?) -> string? {
    ai.internal._aws_resolve_region(region, profile)
}
