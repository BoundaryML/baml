//! Opt-in cloud delivery for native hosts. Credentials never appear in errors.
use std::ffi::OsStr;

use reqwest::Url;

use crate::delivery::{DeliveryConfig, DeliveryError};

impl DeliveryConfig {
    /// Select cloud delivery when `BOUNDARY_URL` is set. A nonempty
    /// `BOUNDARY_API_KEY` is then required. Without a URL, an existing key alone
    /// does not opt into cloud delivery. HTTP is allowed only for a loopback
    /// endpoint; other endpoints must use HTTPS.
    pub fn from_env() -> Result<Option<Self>, DeliveryError> {
        from_values(
            std::env::var_os("BOUNDARY_URL").as_deref(),
            std::env::var_os("BOUNDARY_API_KEY").as_deref(),
        )
    }
}

fn from_values(
    endpoint: Option<&OsStr>,
    token: Option<&OsStr>,
) -> Result<Option<DeliveryConfig>, DeliveryError> {
    let Some(endpoint) = endpoint else {
        return Ok(None);
    };
    let endpoint = endpoint.to_str().ok_or(DeliveryError::InvalidConfig)?;
    let url = Url::parse(endpoint).map_err(|_| DeliveryError::InvalidConfig)?;
    let token = token
        .and_then(OsStr::to_str)
        .filter(|token| !token.trim().is_empty())
        .ok_or(DeliveryError::InvalidConfig)?;
    let mut config = DeliveryConfig::new(url);
    config.allow_http = config.prepare_base_url.scheme() == "http"
        && config.prepare_base_url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        });
    config.bearer_token = Some(token.to_owned());
    config.validate()?;
    Ok(Some(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(endpoint: &str, token: Option<&str>) -> Result<DeliveryConfig, DeliveryError> {
        from_values(Some(OsStr::new(endpoint)), token.map(OsStr::new)).map(|config| config.unwrap())
    }

    #[test]
    fn a_key_alone_does_not_enable_cloud_delivery() {
        assert!(from_values(None, None).unwrap().is_none());
        assert!(
            from_values(None, Some(OsStr::new("existing-key")))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn selects_remote_and_loopback_endpoints_with_path_prefixes() {
        for endpoint in [
            "https://data.example.test/customer/",
            "https://localhost:8080",
            "http://localhost:8080/api",
            "http://127.0.0.1:8080",
            "http://127.0.0.2:8080",
            "http://[::1]:8080",
        ] {
            let delivery = config(endpoint, Some("test-key")).unwrap();
            assert_eq!(
                delivery.prepare_base_url.as_str(),
                Url::parse(endpoint).unwrap().as_str()
            );
            assert_eq!(delivery.bearer_token.as_deref(), Some("test-key"));
            assert_eq!(delivery.allow_http, endpoint.starts_with("http://"));
        }
    }

    #[test]
    fn rejects_unsafe_or_incomplete_configuration_without_echoing_secrets() {
        for endpoint in [
            "",
            "not a url",
            "http://data.example.test",
            "http://localhost.example.test",
            "http://192.168.1.1",
            "https://user:private-password@data.example.test",
            "https://data.example.test?key=private-key",
            "https://data.example.test#private-fragment",
        ] {
            assert_eq!(
                config(endpoint, Some("private-key")).err(),
                Some(DeliveryError::InvalidConfig)
            );
        }
        for token in [None, Some(""), Some("  "), Some("private-key\nheader")] {
            assert_eq!(
                config("https://data.example.test", token).err(),
                Some(DeliveryError::InvalidConfig)
            );
        }
        let error = config(
            "https://data.example.test?key=private-key",
            Some("private-key"),
        )
        .err()
        .unwrap();
        assert!(!error.to_string().contains("private-key"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_unicode_environment_values() {
        use std::os::unix::ffi::OsStrExt;
        let invalid = OsStr::from_bytes(&[0xff]);
        assert_eq!(
            from_values(Some(invalid), Some(OsStr::new("key"))).err(),
            Some(DeliveryError::InvalidConfig)
        );
        assert_eq!(
            from_values(Some(OsStr::new("https://data.example.test")), Some(invalid)).err(),
            Some(DeliveryError::InvalidConfig)
        );
    }
}
