//! Live system-test harness for `forked_google_cloud_auth`.
//!
//! Mints a Google Cloud access token through one of the fork's credential flows
//! (Application Default Credentials, or a service-account JSON file) and then
//! proves the token is accepted by real Google APIs:
//!   * `tokeninfo`            — the token is valid and carries the expected scope
//!   * Cloud Resource Manager — the token authorizes a real API call (project get)
//!   * Vertex AI (optional)   — the real BAML use case, when a model is available
//!
//! On a GCE VM with no on-disk credentials, the ADC flow falls through to the
//! metadata server, so running this binary on a VM exercises that flow.

use std::time::Duration;

use async_trait::async_trait;
use google_cloud_auth::{
    AuthError, CLOUD_PLATFORM_SCOPE, HttpResponse, TokenIo, adc_available, token_from_adc,
    token_from_service_account_json,
};
use serde_json::{Value, json};

struct NativeIo {
    client: reqwest::Client,
}

impl NativeIo {
    fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .connect_timeout(Duration::from_secs(3))
                .build()
                .expect("reqwest client"),
        }
    }
}

#[async_trait]
impl TokenIo for NativeIo {
    async fn env(&self, key: &str) -> Option<String> {
        // Set-but-empty vars are passed through: the fork honors them so
        // misconfiguration is visible, and the harness must not mask that.
        std::env::var(key).ok()
    }

    async fn read_file(&self, path: &str) -> Option<String> {
        tokio::fs::read_to_string(path).await.ok()
    }

    async fn http(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<HttpResponse, AuthError> {
        let m = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|e| AuthError::Io(format!("bad method {method}: {e}")))?;
        let mut req = self.client.request(m, url);
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if !body.is_empty() {
            req = req.body(body.to_string());
        }
        let resp = req
            .send()
            .await
            .map_err(|e| AuthError::Io(format!("http send: {e}")))?;
        let status = resp.status().as_u16();
        let text = resp
            .text()
            .await
            .map_err(|e| AuthError::Io(format!("http body: {e}")))?;
        Ok(HttpResponse { status, body: text })
    }
}

/// GET helper for the validation calls (separate from the fork's TokenIo).
async fn get(client: &reqwest::Client, url: &str, bearer: Option<&str>) -> (u16, String) {
    let mut req = client.get(url);
    if let Some(t) = bearer {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    match req.send().await {
        Ok(r) => {
            let s = r.status().as_u16();
            (s, r.text().await.unwrap_or_default())
        }
        Err(e) => (0, format!("request error: {e}")),
    }
}

async fn post_json(
    client: &reqwest::Client,
    url: &str,
    bearer: &str,
    body: Value,
) -> (u16, String) {
    match client
        .post(url)
        .header("Authorization", format!("Bearer {bearer}"))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
    {
        Ok(r) => {
            let s = r.status().as_u16();
            (s, r.text().await.unwrap_or_default())
        }
        Err(e) => (0, format!("request error: {e}")),
    }
}

struct Args {
    flow: String,
    sa_file: Option<String>,
    scope: String,
    project: Option<String>,
    vertex_model: Option<String>,
    vertex_region: Option<String>,
}

fn parse_args() -> Args {
    let mut a = Args {
        flow: "adc".into(),
        sa_file: None,
        scope: CLOUD_PLATFORM_SCOPE.into(),
        project: None,
        vertex_model: None,
        vertex_region: None,
    };
    let argv: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        let next = |i: &mut usize, flag: &str| -> String {
            match argv.get(*i + 1).filter(|arg| !arg.starts_with("--")) {
                Some(value) => {
                    *i += 1;
                    value.clone()
                }
                None => {
                    eprintln!("{flag} requires a value");
                    std::process::exit(2);
                }
            }
        };
        match argv[i].as_str() {
            "--flow" => a.flow = next(&mut i, "--flow"),
            "--service-account-file" => a.sa_file = Some(next(&mut i, "--service-account-file")),
            "--scope" => a.scope = next(&mut i, "--scope"),
            "--project" => a.project = Some(next(&mut i, "--project")),
            "--vertex-model" => a.vertex_model = Some(next(&mut i, "--vertex-model")),
            "--vertex-region" => a.vertex_region = Some(next(&mut i, "--vertex-region")),
            "--json" => {}
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    a
}

#[tokio::main]
async fn main() {
    let args = parse_args();
    let io = NativeIo::new();
    let validate = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("validate client");
    let mut report = json!({ "flow": args.flow });

    // 1. Mint the token through the selected fork flow.
    report["adc_available"] = json!(adc_available(&io).await);
    let token_res = match args.flow.as_str() {
        "service-account" => match &args.sa_file {
            Some(path) => match tokio::fs::read_to_string(path).await {
                Ok(json_str) => token_from_service_account_json(&io, &json_str, &args.scope).await,
                Err(e) => Err(AuthError::Io(format!("cannot read {path}: {e}"))),
            },
            None => Err(AuthError::NoCredentials(
                "--service-account-file required".into(),
            )),
        },
        _ => token_from_adc(&io, &args.scope).await,
    };

    let token = match token_res {
        Ok(t) => {
            report["token"] = json!({ "ok": true, "prefix": t.chars().take(6).collect::<String>(), "len": t.len() });
            t
        }
        Err(e) => {
            report["token"] = json!({ "ok": false, "error": e.to_string() });
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
            std::process::exit(1);
        }
    };

    // 2. tokeninfo — token validity + scope.
    let (ti_status, ti_body) = get(
        &validate,
        &format!("https://oauth2.googleapis.com/tokeninfo?access_token={token}"),
        None,
    )
    .await;
    let ti: Value = serde_json::from_str(&ti_body).unwrap_or(json!({}));
    report["tokeninfo"] = json!({
        "status": ti_status,
        "scope": ti.get("scope"),
        "email": ti.get("email"),
        "aud": ti.get("aud"),
        "expires_in": ti.get("expires_in"),
    });

    // 3. Cloud Resource Manager — a real API call authorized by the token.
    if let Some(project) = &args.project {
        let (st, body) = get(
            &validate,
            &format!("https://cloudresourcemanager.googleapis.com/v1/projects/{project}"),
            Some(&token),
        )
        .await;
        let v: Value = serde_json::from_str(&body).unwrap_or(json!({}));
        report["resourcemanager"] = json!({
            "status": st,
            "project_id": v.get("projectId"),
            "project_number": v.get("projectNumber"),
        });
    }

    // 4. Vertex AI (optional, real use case) — only when a model is supplied.
    if let (Some(model), Some(region), Some(project)) =
        (&args.vertex_model, &args.vertex_region, &args.project)
    {
        let url = format!(
            "https://{region}-aiplatform.googleapis.com/v1/projects/{project}/locations/{region}/publishers/google/models/{model}:generateContent"
        );
        let (st, body) = post_json(
            &validate,
            &url,
            &token,
            json!({
                "contents": [{"role": "user", "parts": [{"text": "reply with the single word ok"}]}],
                "generationConfig": {"maxOutputTokens": 8}
            }),
        )
        .await;
        report["vertex"] =
            json!({ "status": st, "snippet": body.chars().take(160).collect::<String>() });
    }

    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
