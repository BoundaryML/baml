#![allow(dead_code)]
#![allow(unreachable_pub)]

//! Reusable fixtures + mock `TokenIo` shared across the GCP auth integration
//! tests. Other test files depend on this exact surface — do not change method
//! names/signatures without updating them.

use std::{collections::HashMap, sync::Mutex};

use async_trait::async_trait;
use forked_google_cloud_auth::{AuthError, HttpResponse, TokenIo};

/// A throwaway 2048-bit PKCS#8 RSA private key, used only to build
/// service-account JSON fixtures so JWT signing exercises a real key. Not a
/// credential for anything.
pub const TEST_SA_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCrhCKMxHDCtU92
YdqaXty3U9JohFzWzjbtFjoH/WjmXAlEM+eAvjKjOWqTjsZmiSQQPx6B56eakvPE
hmLSzdxGe+t5OClBy+Wy2xpo7OsTxYo00LsqVvGuiFAQFbUdESfHuShYqHJRoBVo
BfMt44P5gE9aTDI0svw/3Yt9pEaN5I7/jM55jsU0XAqKYPas2w+zAswUHPmLw4JC
xLAZTuTKSsK1ytIqsHgKr4lLLdw9GBRgOlRnAvnVxo//PcazjssjdEp3N87E083h
2f3Mgj9sAGAepzoQorecQFa1PLJniQ5jRR73ZUxGWt65/eIh7M6e0r1EwEM8lWP0
RIjB91E9AgMBAAECggEABu/vgM+SOwHf3qsrDxrepQieJk2SPrr1BEZlnvycMVMQ
KeLKjo3C2RDBs4mvEychXwnah0kSIaGne++ukBW0/uHUvqCrpIZlekQ773oDsRdI
lYXKyDXfjR5k1J24J16SBBVEYT+g7hXCP+SbtyOwaxdKPl3+Gt0RcFja40BRfTwr
/rLxdTfNxGIFDVXsjJmQvHP5VtHZfH//9AgqQpK+6B2E4QecQnHM5Bp9n9MOa+E6
3zJXEEyZEX6dC29g+K2A8D2LUXTX9hhWoos+3wlbpX5VUQTwGtHWjyGsTwhdFTQr
K7kqOibpAbYRCv8IiLlf1H29DcINcvBB+2EzLRmhqQKBgQDyHsUu1sZw0bis/ZIe
vOZuOnEaiu02/InhHkWX4ZAQpteDh7lztXvMoYVhB+Hs9StWgcW3FJHabYvhJ63V
1gsysqkeYfjZ/W8ISYbXF27ucgUJeUxC/Dlps/m3bxoFXF348K+QWv6UdKz1c4Nw
9zDbC9ewQHCtl4QIgz6QopcIJQKBgQC1WTdxpmD+iPOHuDitonEiNKGmEC6k2kQv
WSrgOourUv9CUGCtt7eL6XpXjg2xCynf2MkLpazYBzmLXqG+fsZkGMQRc7TJkRzl
krwjUvSvFJcu0odOT8mHwnGavTjBtFiPKyItWhzGAQsCsWe6Oe3/OX74TTESD21B
4j3bgcgtOQKBgFWWmP+wvp9dE5pbXM7u2co3cIoAeFCKvzbMG6/P9bxdLiv5y43i
pqu0oVCml6/LDxHaeAj7BYAgX2UtQJ8ptfWrAGuUGIL+usREMZ1RVE6IEc3CijnX
rXf3Phwg8yLX/wQkGPu/nuTdxdJSjjFdwHB+ZDWS4gILYIod0v0P7LHdAoGBAJ8H
aL6KN96ePGlVHKbvn6RuYR8ea7j3Cvo2iInv7VFFTEFb+Rv90sCn8zhagxkxf/wj
wFItbEBZPZZBWzeRNurKaQ4g2HY2gg+0OLYFZjsupFFUH4GGKGWcF0GqE96SB2Mt
YSBCOJ9OhNhMuHivmkzJn9Wg45pB2v7+pl4bFm7JAoGAPbGTgXye0yojEYrPgAG4
WncAmADNT3STn86JmQZ0YcyNHyn7NrAmcA6/aFtyw7LMXOcBqo113iwOFI2Dkem5
3lpNkLS74D027vpxSvZJVN0222HBQO/gg/qLuwequMh3OfGGOIHuoS7eK4HFkhKb
tst4y1NcOyflFIpR9oPNncg=
-----END PRIVATE KEY-----
";

/// Build a `service_account` ADC JSON document around [`TEST_SA_PRIVATE_KEY`].
pub fn service_account_json(client_email: &str, token_uri: &str) -> String {
    serde_json::json!({
        "type": "service_account",
        "project_id": "test-project",
        "private_key_id": "key-id",
        "private_key": TEST_SA_PRIVATE_KEY,
        "client_email": client_email,
        "client_id": "123",
        "token_uri": token_uri,
    })
    .to_string()
}

/// Build an `authorized_user` ADC JSON document.
pub fn authorized_user_json(client_id: &str, refresh_token: &str, token_uri: &str) -> String {
    serde_json::json!({
        "type": "authorized_user",
        "client_id": client_id,
        "client_secret": "secret",
        "refresh_token": refresh_token,
        "token_uri": token_uri,
    })
    .to_string()
}

/// A standard `OAuth2` token-endpoint success body.
pub fn token_response(access_token: &str) -> String {
    serde_json::json!({
        "access_token": access_token,
        "token_type": "Bearer",
        "expires_in": 3600,
    })
    .to_string()
}

type HttpHandler = Box<
    dyn Fn(&str, &str, &[(String, String)], &str) -> Result<HttpResponse, AuthError> + Send + Sync,
>;

/// Configurable mock [`TokenIo`].
pub struct MockIo {
    env: HashMap<String, String>,
    exact_files: Vec<(String, String)>,
    contains_files: Vec<(String, String)>,
    http: Option<HttpHandler>,
    tracking: Mutex<(usize, Option<String>, Option<String>)>, // (calls, last_url, last_body)
}

impl MockIo {
    pub fn new() -> MockIo {
        MockIo {
            env: HashMap::new(),
            exact_files: Vec::new(),
            contains_files: Vec::new(),
            http: None,
            tracking: Mutex::new((0, None, None)),
        }
    }

    pub fn env(mut self, key: &str, val: &str) -> MockIo {
        self.env.insert(key.to_string(), val.to_string());
        self
    }

    pub fn file(mut self, path: &str, contents: &str) -> MockIo {
        self.exact_files
            .push((path.to_string(), contents.to_string()));
        self
    }

    /// Return `contents` for any `read_file` path containing `substr`.
    pub fn file_contains(mut self, substr: &str, contents: &str) -> MockIo {
        self.contains_files
            .push((substr.to_string(), contents.to_string()));
        self
    }

    /// Install an HTTP handler `(method, url, headers, body) -> HttpResponse`.
    pub fn http<F>(mut self, handler: F) -> MockIo
    where
        F: Fn(&str, &str, &[(String, String)], &str) -> Result<HttpResponse, AuthError>
            + Send
            + Sync
            + 'static,
    {
        self.http = Some(Box::new(handler));
        self
    }

    pub fn http_calls(&self) -> usize {
        self.tracking.lock().unwrap().0
    }
    pub fn last_http_url(&self) -> Option<String> {
        self.tracking.lock().unwrap().1.clone()
    }
    pub fn last_http_body(&self) -> Option<String> {
        self.tracking.lock().unwrap().2.clone()
    }
}

impl Default for MockIo {
    fn default() -> Self {
        MockIo::new()
    }
}

#[async_trait]
impl TokenIo for MockIo {
    async fn env(&self, key: &str) -> Option<String> {
        self.env.get(key).cloned()
    }

    async fn read_file(&self, path: &str) -> Option<String> {
        for (p, c) in &self.exact_files {
            if p == path {
                return Some(c.clone());
            }
        }
        for (substr, c) in &self.contains_files {
            if path.contains(substr.as_str()) {
                return Some(c.clone());
            }
        }
        None
    }

    async fn http(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<HttpResponse, AuthError> {
        {
            let mut t = self.tracking.lock().unwrap();
            t.0 += 1;
            t.1 = Some(url.to_string());
            t.2 = Some(body.to_string());
        }
        match &self.http {
            Some(h) => h(method, url, headers, body),
            None => Err(AuthError::Io("no http handler".into())),
        }
    }
}
