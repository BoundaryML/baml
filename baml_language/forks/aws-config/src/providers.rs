//! Individual credential providers used by the default chain.

use sha1::{Digest, Sha1};

use crate::{
    ConfigError, CredentialIo, Credentials, credential_process_credentials_from_value,
    credentials_from_json, credentials_from_value, profile,
};

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

/// Static credentials from `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`
/// (with the legacy `SECRET_ACCESS_KEY` fallback) and optional
/// `AWS_SESSION_TOKEN`.
pub(crate) async fn from_env(io: &dyn CredentialIo) -> Option<Credentials> {
    let access_key_id = io
        .env("AWS_ACCESS_KEY_ID")
        .await
        .filter(|s| !s.trim().is_empty())?;
    let secret_access_key = match io
        .env("AWS_SECRET_ACCESS_KEY")
        .await
        .filter(|s| !s.trim().is_empty())
    {
        Some(s) => s,
        None => io
            .env("SECRET_ACCESS_KEY")
            .await
            .filter(|s| !s.trim().is_empty())?,
    };
    let session_token = io.env("AWS_SESSION_TOKEN").await.and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    Some(Credentials::new(
        access_key_id,
        secret_access_key,
        session_token,
    ))
}

// ---------------------------------------------------------------------------
// Shared profile
// ---------------------------------------------------------------------------

/// Credentials from the active profile: static keys, then `credential_process`,
/// then SSO. Returns `Ok(None)` when the profile defines none of these.
pub(crate) async fn from_profile(
    io: &dyn CredentialIo,
    profile_override: Option<&str>,
) -> Result<Option<Credentials>, ConfigError> {
    let profiles = profile::load_profiles(io).await;
    let name = profile::active_profile_name(io, profile_override).await;
    let Some(p) = profiles.get(&name) else {
        return Ok(None);
    };

    // Static credentials.
    if let (Some(access), Some(secret)) =
        (p.get("aws_access_key_id"), p.get("aws_secret_access_key"))
    {
        if !access.is_empty() && !secret.is_empty() {
            let token = p
                .get("aws_session_token")
                .filter(|s| !s.is_empty())
                .cloned();
            return Ok(Some(Credentials::new(
                access.clone(),
                secret.clone(),
                token,
            )));
        }
    }

    // credential_process.
    if let Some(command) = p.get("credential_process").filter(|s| !s.is_empty()) {
        let out = io.run_command(command).await?;
        if out.status != 0 {
            return Err(ConfigError::Io(format!(
                "credential_process exited with status {}",
                out.status
            )));
        }
        return Ok(Some(parse_credential_process(&out.stdout)?));
    }

    // SSO.
    if p.contains_key("sso_account_id") && p.contains_key("sso_role_name") {
        return Ok(Some(from_sso(io, p).await?));
    }

    Ok(None)
}

/// Parse `credential_process` output: a JSON object with `Version == 1` and the
/// PascalCase credential fields.
fn parse_credential_process(stdout: &str) -> Result<Credentials, ConfigError> {
    let value: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|e| ConfigError::Parse(format!("invalid credential_process output: {e}")))?;
    match number_field(&value, "Version") {
        Some(1) => {}
        _ => {
            return Err(ConfigError::Parse(
                "credential_process output must have Version == 1".into(),
            ));
        }
    }
    credential_process_credentials_from_value(&value)
}

fn number_field(value: &serde_json::Value, field: &str) -> Option<u64> {
    value.as_object()?.iter().find_map(|(key, value)| {
        if key.eq_ignore_ascii_case(field) {
            value.as_u64()
        } else {
            None
        }
    })
}

// ---------------------------------------------------------------------------
// SSO
// ---------------------------------------------------------------------------

async fn from_sso(
    io: &dyn CredentialIo,
    p: &std::collections::HashMap<String, String>,
) -> Result<Credentials, ConfigError> {
    let account_id = p.get("sso_account_id").unwrap();
    let role_name = p.get("sso_role_name").unwrap();

    // Determine start URL / region and the token cache key. With an
    // `sso_session`, the cache key is the session name and the start URL/region
    // come from the `[sso-session NAME]` section; otherwise they are inline and
    // the cache key is the start URL.
    let (start_url, region, cache_key) = if let Some(session) = p.get("sso_session") {
        let sessions = profile::load_sso_sessions(io).await;
        let s = sessions.get(session).ok_or_else(|| {
            ConfigError::Parse(format!("sso-session '{session}' not found in config"))
        })?;
        let start_url = s
            .get("sso_start_url")
            .ok_or_else(|| ConfigError::Parse("sso-session missing sso_start_url".into()))?
            .clone();
        let region = s
            .get("sso_region")
            .ok_or_else(|| ConfigError::Parse("sso-session missing sso_region".into()))?
            .clone();
        (start_url, region, session.clone())
    } else {
        let start_url = p
            .get("sso_start_url")
            .ok_or_else(|| ConfigError::Parse("profile missing sso_start_url".into()))?
            .clone();
        let region = p
            .get("sso_region")
            .ok_or_else(|| ConfigError::Parse("profile missing sso_region".into()))?
            .clone();
        (start_url.clone(), region, start_url)
    };

    // Read the cached access token.
    let home = home_dir(io)
        .await
        .ok_or_else(|| ConfigError::Io("cannot determine home directory for SSO cache".into()))?;
    let hash = hex::encode(Sha1::digest(cache_key.as_bytes()));
    let cache_path = format!("{home}/.aws/sso/cache/{hash}.json");
    let token_json = io.read_file(&cache_path).await.ok_or_else(|| {
        ConfigError::Io(format!(
            "SSO token cache not found at {cache_path}; run `aws sso login`"
        ))
    })?;
    let token: serde_json::Value = serde_json::from_str(&token_json)
        .map_err(|e| ConfigError::Parse(format!("invalid SSO token cache: {e}")))?;
    let access_token = token
        .get("accessToken")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ConfigError::Parse("SSO token cache missing accessToken".into()))?;

    let _ = &start_url; // start_url is validated above; not needed for the call.

    // Call GetRoleCredentials on the SSO portal.
    let url = format!(
        "https://portal.sso.{region}.amazonaws.com/federation/credentials?role_name={}&account_id={}",
        encode_query_value(role_name),
        encode_query_value(account_id)
    );
    let headers = vec![(
        "x-amz-sso_bearer_token".to_string(),
        access_token.to_string(),
    )];
    let resp = io.http("GET", &url, &headers).await?;
    if resp.status != 200 {
        return Err(ConfigError::Io(format!(
            "SSO GetRoleCredentials returned HTTP {}",
            resp.status
        )));
    }
    let body: serde_json::Value = serde_json::from_str(&resp.body)
        .map_err(|e| ConfigError::Parse(format!("invalid SSO credentials response: {e}")))?;
    let role_creds = body
        .get("roleCredentials")
        .ok_or_else(|| ConfigError::Parse("SSO response missing roleCredentials".into()))?;
    credentials_from_value(role_creds)
}

fn encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push('%');
                out.push(hex_digit(byte >> 4));
                out.push(hex_digit(byte & 0x0f));
            }
        }
    }
    out
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'A' + (nibble - 10)) as char,
        _ => unreachable!("nibble is masked to four bits"),
    }
}

async fn home_dir(io: &dyn CredentialIo) -> Option<String> {
    if let Some(h) = io.env("HOME").await.filter(|s| !s.is_empty()) {
        return Some(h);
    }
    io.env("USERPROFILE").await.filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// ECS / container endpoint
// ---------------------------------------------------------------------------

const CONTAINER_BASE_HOST: &str = "http://169.254.170.2";

/// Credentials from the ECS/container metadata endpoint. Returns `Ok(None)`
/// when neither the relative nor full URI env var is set.
pub(crate) async fn from_container(
    io: &dyn CredentialIo,
) -> Result<Option<Credentials>, ConfigError> {
    let url = if let Some(rel) = io
        .env("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI")
        .await
        .filter(|s| !s.is_empty())
    {
        format!("{CONTAINER_BASE_HOST}{rel}")
    } else if let Some(full) = io
        .env("AWS_CONTAINER_CREDENTIALS_FULL_URI")
        .await
        .filter(|s| !s.is_empty())
    {
        full
    } else {
        return Ok(None);
    };

    // Optional authorization token. Upstream gives the token file precedence
    // and errors if the file cannot be read.
    let mut headers = Vec::new();
    if let Some(token_file) = io
        .env("AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE")
        .await
        .filter(|s| !s.is_empty())
    {
        let token = io.read_file(&token_file).await.ok_or_else(|| {
            ConfigError::Io(format!(
                "container authorization token file could not be read: {token_file}"
            ))
        })?;
        headers.push(("Authorization".to_string(), token.trim().to_string()));
    } else if let Some(token) = io
        .env("AWS_CONTAINER_AUTHORIZATION_TOKEN")
        .await
        .filter(|s| !s.is_empty())
    {
        headers.push(("Authorization".to_string(), token));
    }

    let resp = io.http("GET", &url, &headers).await?;
    if resp.status != 200 {
        return Err(ConfigError::Io(format!(
            "container credentials endpoint returned HTTP {}",
            resp.status
        )));
    }
    Ok(Some(credentials_from_json(&resp.body)?))
}

// ---------------------------------------------------------------------------
// EC2 IMDS
// ---------------------------------------------------------------------------

const IMDS_BASE: &str = "http://169.254.169.254";

/// Credentials from the EC2 instance metadata service (IMDSv2). Returns
/// `Ok(None)` when IMDS is disabled via `AWS_EC2_METADATA_DISABLED=true`.
pub(crate) async fn from_imds(io: &dyn CredentialIo) -> Result<Option<Credentials>, ConfigError> {
    if io
        .env("AWS_EC2_METADATA_DISABLED")
        .await
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return Ok(None);
    }

    // IMDSv2: fetch a session token.
    let token_resp = io
        .http(
            "PUT",
            &format!("{IMDS_BASE}/latest/api/token"),
            &[(
                "x-aws-ec2-metadata-token-ttl-seconds".to_string(),
                "21600".to_string(),
            )],
        )
        .await?;
    let token_header: Vec<(String, String)> = if token_resp.status == 200 {
        vec![(
            "x-aws-ec2-metadata-token".to_string(),
            token_resp.body.trim().to_string(),
        )]
    } else {
        // Fall back to IMDSv1 (no token).
        Vec::new()
    };

    // Discover the instance profile (role) name.
    let role_path = format!("{IMDS_BASE}/latest/meta-data/iam/security-credentials/");
    let role_resp = io.http("GET", &role_path, &token_header).await?;
    if role_resp.status != 200 {
        return Err(ConfigError::Io(format!(
            "IMDS role lookup returned HTTP {}",
            role_resp.status
        )));
    }
    let role = role_resp.body.trim();
    if role.is_empty() {
        return Err(ConfigError::NoCredentials("IMDS returned no role".into()));
    }

    // Fetch the role credentials.
    let creds_resp = io
        .http("GET", &format!("{role_path}{role}"), &token_header)
        .await?;
    if creds_resp.status != 200 {
        return Err(ConfigError::Io(format!(
            "IMDS credentials returned HTTP {}",
            creds_resp.status
        )));
    }
    Ok(Some(credentials_from_json(&creds_resp.body)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_credential_process_v1() {
        let out =
            r#"{"Version":1,"AccessKeyId":"AKID","SecretAccessKey":"SEC","SessionToken":"TOK"}"#;
        let creds = parse_credential_process(out).unwrap();
        assert_eq!(creds.access_key_id, "AKID");
        assert_eq!(creds.secret_access_key, "SEC");
        assert_eq!(creds.session_token.as_deref(), Some("TOK"));
    }

    #[test]
    fn rejects_credential_process_wrong_version() {
        let out = r#"{"Version":2,"AccessKeyId":"A","SecretAccessKey":"B"}"#;
        assert!(parse_credential_process(out).is_err());
    }
}
