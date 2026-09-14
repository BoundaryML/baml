//! AWS `SigV4` request signing and region resolution.
//!
//! ## Credential resolution order
//!
//! 1. Explicit `access_key_id` + `secret_access_key` (+ optional
//!    `session_token`) from the client's options.
//! 2. The AWS provider chain via the `aws-config` fork: environment variables,
//!    the shared config/credentials files (static keys, `credential_process`,
//!    SSO cache), the ECS/container endpoint, then EC2 IMDS.
//!
//! Both halves of an explicit key pair must be present for it to be used; a
//! lone `access_key_id` falls through to the chain rather than signing with a
//! half-configured identity.
//!
//! **Signing must be the last mutation of a request**: the signature covers the
//! final method, URL, headers, and body. Anything added afterwards (another
//! header, a body tweak) invalidates it.

use std::sync::Arc;

use aws_sigv4::Credentials;
use sys_types::runtime_io::RuntimeIo;
use web_time::SystemTime;

use crate::AuthError;

/// Everything the signer needs beyond the request itself.
///
/// Mirrors the BAML-side `aws.internal.SignOptions`.
#[derive(Debug, Clone, Default)]
pub struct AwsSignOptions {
    pub region: Option<String>,
    pub profile: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub session_token: Option<String>,
    /// The AWS service name the signature is scoped to (e.g. `bedrock`).
    pub service: String,
}

/// SigV4-sign a request, returning the headers to apply to it
/// (`authorization`, `x-amz-date`, and `x-amz-security-token` for session
/// credentials).
///
/// `headers` and `body` must already be final — see the module note.
pub async fn sign_request(
    io: Arc<dyn RuntimeIo>,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
    opts: &AwsSignOptions,
) -> Result<Vec<(String, String)>, AuthError> {
    let credentials = resolve_credentials(io.clone(), opts).await?;
    let region = resolve_region(io, opts.region.clone(), opts.profile.clone())
        .await
        .ok_or_else(|| {
            AuthError::Access(
                "AWS region not found: set the client's region option, AWS_REGION, \
                 AWS_DEFAULT_REGION, or a region in the active profile."
                    .to_string(),
            )
        })?;

    let header_pairs: Vec<(&str, &str)> = headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    aws_sigv4::sign_request(
        method,
        url,
        &header_pairs,
        body,
        &credentials,
        &region,
        &opts.service,
        SystemTime::now(),
    )
    .map_err(|e| AuthError::Access(format!("AWS SigV4 signing: {e}")))
}

/// Resolve the AWS region: the explicit option, else `AWS_REGION` /
/// `AWS_DEFAULT_REGION` / the active profile's `region` key.
///
/// `None` means "not discoverable"; the caller decides whether that is fatal
/// (it is for signing, but a client may want to report it differently).
pub async fn resolve_region(
    io: Arc<dyn RuntimeIo>,
    region: Option<String>,
    profile: Option<String>,
) -> Option<String> {
    if let Some(region) = region {
        return Some(region);
    }
    let adapter = crate::bridge(io);
    aws_config::resolve_region(&adapter, profile.as_deref()).await
}

/// Explicit options first, then the AWS provider chain.
async fn resolve_credentials(
    io: Arc<dyn RuntimeIo>,
    opts: &AwsSignOptions,
) -> Result<Credentials, AuthError> {
    if let Some(creds) = credentials_from_options(opts) {
        return Ok(creds);
    }
    let adapter = crate::bridge(io);
    aws_config::resolve_credentials(&adapter, opts.profile.as_deref())
        .await
        .map_err(map_config_error)
}

/// Build credentials from explicit options, or `None` if the key pair is
/// incomplete.
fn credentials_from_options(opts: &AwsSignOptions) -> Option<Credentials> {
    let access_key_id = opts.access_key_id.as_ref()?;
    let secret_access_key = opts.secret_access_key.as_ref()?;
    Some(Credentials::new(
        access_key_id.clone(),
        secret_access_key.clone(),
        opts.session_token.clone(),
    ))
}

/// Transport failures stay retry-safe (`Io`); a missing or malformed credential
/// is not something a retry fixes.
fn map_config_error(err: aws_config::ConfigError) -> AuthError {
    match err {
        aws_config::ConfigError::Io(m) => AuthError::Io(format!("AWS credentials: {m}")),
        other => AuthError::Access(format!("AWS credentials: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::StubIo;

    fn explicit_opts() -> AwsSignOptions {
        AwsSignOptions {
            region: Some("us-east-1".to_string()),
            access_key_id: Some("AKIAIOSFODNN7EXAMPLE".to_string()),
            secret_access_key: Some("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string()),
            service: "bedrock".to_string(),
            ..AwsSignOptions::default()
        }
    }

    fn request_headers() -> Vec<(String, String)> {
        vec![("content-type".to_string(), "application/json".to_string())]
    }

    const URL: &str = "https://bedrock-runtime.us-east-1.amazonaws.com/model/m/converse";

    fn header<'a>(signed: &'a [(String, String)], name: &str) -> Option<&'a str> {
        signed
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    #[tokio::test]
    async fn explicit_credentials_sign_without_touching_io() {
        let io = StubIo::new();
        let calls = io.call_counter();
        let signed = sign_request(
            io.arc(),
            "POST",
            URL,
            &request_headers(),
            b"{}",
            &explicit_opts(),
        )
        .await
        .unwrap();

        assert!(header(&signed, "authorization").is_some());
        assert!(header(&signed, "x-amz-date").is_some());
        assert_eq!(calls.total(), 0, "explicit credentials must not do any IO");
    }

    /// A lone access key id is a half-configured identity: fall through to the
    /// chain instead of signing with it.
    #[tokio::test]
    async fn partial_explicit_credentials_fall_through_to_the_chain() {
        let opts = AwsSignOptions {
            access_key_id: Some("AKID".to_string()),
            ..explicit_opts()
        };
        let opts = AwsSignOptions {
            secret_access_key: None,
            ..opts
        };
        assert!(credentials_from_options(&opts).is_none());

        let io = StubIo::new();
        let calls = io.call_counter();
        // The chain then finds nothing (env, files, container, IMDS all empty),
        // so signing fails rather than proceeding with half an identity.
        sign_request(io.arc(), "POST", URL, &request_headers(), b"{}", &opts)
            .await
            .unwrap_err();
        assert!(calls.total() > 0, "the provider chain must have been tried");
    }

    #[tokio::test]
    async fn credentials_from_the_environment_chain() {
        let io = StubIo::new()
            .env("AWS_ACCESS_KEY_ID", "AKIAIOSFODNN7EXAMPLE")
            .env(
                "AWS_SECRET_ACCESS_KEY",
                "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            )
            .env("AWS_REGION", "us-east-1");
        let opts = AwsSignOptions {
            service: "bedrock".to_string(),
            ..AwsSignOptions::default()
        };
        let signed = sign_request(io.arc(), "POST", URL, &request_headers(), b"{}", &opts)
            .await
            .unwrap();
        assert!(header(&signed, "authorization").is_some());
    }

    /// Session credentials must carry the security token into the request.
    #[tokio::test]
    async fn session_token_is_applied() {
        let opts = AwsSignOptions {
            session_token: Some("FwoGZXIvYXdzEBYaDH".to_string()),
            ..explicit_opts()
        };
        let signed = sign_request(
            StubIo::new().arc(),
            "POST",
            URL,
            &request_headers(),
            b"{}",
            &opts,
        )
        .await
        .unwrap();
        assert_eq!(
            header(&signed, "x-amz-security-token"),
            Some("FwoGZXIvYXdzEBYaDH")
        );
    }

    /// Signing must be the *last* mutation: the signature covers the body, so a
    /// changed body produces a different signature.
    #[tokio::test]
    async fn signature_covers_the_body() {
        let a = sign_request(
            StubIo::new().arc(),
            "POST",
            URL,
            &request_headers(),
            b"{\"a\":1}",
            &explicit_opts(),
        )
        .await
        .unwrap();
        let b = sign_request(
            StubIo::new().arc(),
            "POST",
            URL,
            &request_headers(),
            b"{\"a\":2}",
            &explicit_opts(),
        )
        .await
        .unwrap();
        assert_ne!(header(&a, "authorization"), header(&b, "authorization"));
    }

    /// ...and the headers: a header added after signing would not appear in
    /// `SignedHeaders`, which is exactly why it must be added before.
    #[tokio::test]
    async fn signature_covers_the_headers() {
        let mut headers = request_headers();
        headers.push(("x-custom".to_string(), "v".to_string()));
        let signed = sign_request(
            StubIo::new().arc(),
            "POST",
            URL,
            &headers,
            b"{}",
            &explicit_opts(),
        )
        .await
        .unwrap();
        let auth = header(&signed, "authorization").unwrap();
        assert!(auth.contains("x-custom"), "got: {auth}");
    }

    #[tokio::test]
    async fn region_option_wins_over_the_environment() {
        let io = StubIo::new().env("AWS_REGION", "ap-southeast-1");
        assert_eq!(
            resolve_region(io.arc(), Some("eu-west-1".to_string()), None).await,
            Some("eu-west-1".to_string())
        );
    }

    #[tokio::test]
    async fn region_from_the_environment() {
        let io = StubIo::new().env("AWS_REGION", "ap-southeast-1");
        assert_eq!(
            resolve_region(io.arc(), None, None).await,
            Some("ap-southeast-1".to_string())
        );
    }

    #[tokio::test]
    async fn region_is_none_when_undiscoverable() {
        assert_eq!(resolve_region(StubIo::new().arc(), None, None).await, None);
    }

    /// An undiscoverable region fails signing with an actionable message.
    #[tokio::test]
    async fn missing_region_is_an_actionable_error() {
        let opts = AwsSignOptions {
            region: None,
            ..explicit_opts()
        };
        let err = sign_request(
            StubIo::new().arc(),
            "POST",
            URL,
            &request_headers(),
            b"{}",
            &opts,
        )
        .await
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("AWS_REGION"), "got: {msg}");
    }
}
