use std::time::Duration;

use async_trait::async_trait;
use aws_config::{
    CommandOutput, ConfigError, CredentialIo, Credentials, HttpResponse, resolve_credentials,
    resolve_region,
};
use aws_sigv4::sign_request;
use serde_json::{Value, json};
use web_time::SystemTime;

const DEFAULT_PROFILE: &str = "boundaryml-dev";
const DEFAULT_REGION: &str = "us-east-1";
// On-demand-invokable model in the test accounts (no inference-profile required).
const DEFAULT_BEDROCK_MODEL: &str = "amazon.nova-micro-v1:0";

/// Native implementation of the `CredentialIo` trait backed by a shared
/// reqwest client, the process environment, the local filesystem, and `sh -c`.
struct NativeIo {
    client: reqwest::Client,
}

impl NativeIo {
    fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(4))
            .connect_timeout(Duration::from_secs(2))
            .build()
            .expect("failed to build reqwest client");
        Self { client }
    }
}

#[async_trait]
impl CredentialIo for NativeIo {
    async fn env(&self, key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|s| !s.is_empty())
    }

    async fn read_file(&self, path: &str) -> Option<String> {
        tokio::fs::read_to_string(path).await.ok()
    }

    async fn http(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
    ) -> Result<HttpResponse, ConfigError> {
        let m = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|e| ConfigError::Io(format!("invalid method {method}: {e}")))?;
        let mut req = self.client.request(m, url);
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ConfigError::Io(format!("http send failed: {e}")))?;
        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .map_err(|e| ConfigError::Io(format!("http body read failed: {e}")))?;
        Ok(HttpResponse { status, body })
    }

    async fn run_command(&self, command: &str) -> Result<CommandOutput, ConfigError> {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .map_err(|e| ConfigError::Io(format!("run_command failed: {e}")))?;
        Ok(CommandOutput {
            status: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        })
    }
}

fn extract_tag<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(&xml[start..end])
}

/// Sign and send one request via the NativeIo HTTP path. `extra_headers` is the
/// set of headers (e.g. content-type) that participate in signing and must also
/// be sent on the wire alongside every signed header returned by the signer.
async fn signed_request(
    io: &NativeIo,
    method: &str,
    url: &str,
    extra_headers: &[(&str, &str)],
    body: &[u8],
    creds: &Credentials,
    region: &str,
    service: &str,
) -> Result<HttpResponse, String> {
    let signed = sign_request(
        method,
        url,
        extra_headers,
        body,
        creds,
        region,
        service,
        SystemTime::now(),
    )
    .map_err(|e| format!("signing failed: {e}"))?;

    let mut send_headers: Vec<(String, String)> = extra_headers
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    send_headers.extend(signed);

    // For non-empty bodies we must send the body. NativeIo::http does not take a
    // body argument, so send it by extending the reqwest request directly.
    send_with_body(io, method, url, &send_headers, body)
        .await
        .map_err(|e| format!("{e}"))
}

/// Like NativeIo::http but also sends a request body.
async fn send_with_body(
    io: &NativeIo,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<HttpResponse, ConfigError> {
    let m = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|e| ConfigError::Io(format!("invalid method {method}: {e}")))?;
    let mut req = io.client.request(m, url);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    req = req.body(body.to_vec());
    let resp = req
        .send()
        .await
        .map_err(|e| ConfigError::Io(format!("http send failed: {e}")))?;
    let status = resp.status().as_u16();
    let text = resp
        .text()
        .await
        .map_err(|e| ConfigError::Io(format!("http body read failed: {e}")))?;
    Ok(HttpResponse { status, body: text })
}

async fn do_work(
    profile: Option<&str>,
    region_override: Option<&str>,
    sign_sts: bool,
    sign_bedrock: bool,
    bedrock_model: &str,
) -> Value {
    let io = NativeIo::new();
    let mut report = json!({});

    let creds = match resolve_credentials(&io, profile).await {
        Ok(c) => {
            report["credentials"] = json!({
                "ok": true,
                "access_key_prefix": c.access_key_id.chars().take(5).collect::<String>(),
                "has_session_token": c.session_token.is_some(),
            });
            Some(c)
        }
        Err(e) => {
            report["credentials"] = json!({ "ok": false, "error": e.to_string() });
            None
        }
    };

    let region = match region_override {
        Some(r) => r.to_string(),
        None => resolve_region(&io, profile)
            .await
            .unwrap_or_else(|| DEFAULT_REGION.to_string()),
    };
    report["region"] = json!(region);

    let creds = match creds {
        Some(c) => c,
        None => return report,
    };

    if sign_sts {
        let url = format!("https://sts.{region}.amazonaws.com/");
        let body = b"Action=GetCallerIdentity&Version=2011-06-15";
        let headers = [("content-type", "application/x-www-form-urlencoded")];
        match signed_request(&io, "POST", &url, &headers, body, &creds, &region, "sts").await {
            Ok(resp) => {
                report["sts"] = json!({
                    "status": resp.status,
                    "account": extract_tag(&resp.body, "Account"),
                    "arn": extract_tag(&resp.body, "Arn"),
                });
            }
            Err(e) => {
                report["sts"] = json!({ "error": e });
            }
        }
    }

    if sign_bedrock {
        let url = format!(
            "https://bedrock-runtime.{region}.amazonaws.com/model/{bedrock_model}/converse"
        );
        let body_val = json!({
            "messages": [{"role": "user", "content": [{"text": "reply with the single word ok"}]}],
            "inferenceConfig": {"maxTokens": 12}
        });
        let body = serde_json::to_vec(&body_val).unwrap();
        let headers = [("content-type", "application/json")];
        match signed_request(
            &io, "POST", &url, &headers, &body, &creds, &region, "bedrock",
        )
        .await
        {
            Ok(resp) => {
                let snippet: String = resp.body.chars().take(300).collect();
                report["bedrock"] = json!({
                    "status": resp.status,
                    "snippet": snippet,
                });
            }
            Err(e) => {
                report["bedrock"] = json!({ "error": e });
            }
        }
    }

    report
}

async fn lambda_loop() {
    let api = std::env::var("AWS_LAMBDA_RUNTIME_API").expect("AWS_LAMBDA_RUNTIME_API not set");
    let client = reqwest::Client::builder()
        .build()
        .expect("failed to build lambda client");

    loop {
        let next_url = format!("http://{api}/2018-06-01/runtime/invocation/next");
        let resp = match client.get(&next_url).send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("failed to get next invocation: {e}");
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        };
        let reqid = resp
            .headers()
            .get("Lambda-Runtime-Aws-Request-Id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let _ = resp.text().await;

        // Allow the deployed function to override the model via env (set on the
        // Lambda function configuration); fall back to the on-demand default.
        let model = std::env::var("BEDROCK_MODEL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_BEDROCK_MODEL.to_string());
        let report = do_work(None, None, true, true, &model).await;

        let response_url = format!("http://{api}/2018-06-01/runtime/invocation/{reqid}/response");
        if let Err(e) = client.post(&response_url).json(&report).send().await {
            let err_url = format!("http://{api}/2018-06-01/runtime/invocation/{reqid}/error");
            let _ = client
                .post(&err_url)
                .json(&json!({ "errorMessage": e.to_string(), "errorType": "PostResponseError" }))
                .send()
                .await;
        }
    }
}

#[tokio::main]
async fn main() {
    if std::env::var("AWS_LAMBDA_RUNTIME_API").is_ok() {
        lambda_loop().await;
        return;
    }

    let args: Vec<String> = std::env::args().collect();
    let mut profile: Option<String> = None;
    let mut region: Option<String> = None;
    let mut sign_sts = false;
    let mut sign_bedrock = false;
    let mut bedrock_model = DEFAULT_BEDROCK_MODEL.to_string();
    let mut _json_flag = false;

    let value_after = |i: usize, flag: &str| -> String {
        match args.get(i + 1).filter(|arg| !arg.starts_with("--")) {
            Some(value) => value.clone(),
            None => {
                eprintln!("{flag} requires a value");
                std::process::exit(2);
            }
        }
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--profile" => {
                profile = Some(value_after(i, "--profile"));
                i += 1;
            }
            "--region" => {
                region = Some(value_after(i, "--region"));
                i += 1;
            }
            "--sign-sts" => sign_sts = true,
            "--sign-bedrock" => sign_bedrock = true,
            "--bedrock-model" => {
                bedrock_model = value_after(i, "--bedrock-model");
                i += 1;
            }
            "--json" => _json_flag = true,
            other => {
                eprintln!("unknown flag: {other}");
                std::process::exit(2);
            }
        }
        i += 1;
    }

    // Default profile when none supplied/inferable.
    let effective_profile = profile.clone().or_else(|| {
        if std::env::var("AWS_PROFILE").is_ok() {
            None
        } else {
            Some(DEFAULT_PROFILE.to_string())
        }
    });

    let report = do_work(
        effective_profile.as_deref(),
        region.as_deref(),
        sign_sts,
        sign_bedrock,
        &bedrock_model,
    )
    .await;

    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
