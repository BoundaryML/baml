//! LLM clients resolve their environment at request time, on every request.
//!
//! One engine runs every client twice. Between the rounds the test rewrites
//! the environment: every credential changes, and every endpoint moves to a
//! second server. Each request must carry the values set for its own round —
//! a client that read its environment once (when the top-level `client`
//! singleton was built during `$init`, or on its first request) would send
//! round one's values again.
//!
//! The clients cover each provider and both ways a client gets a credential:
//! an explicit `env.NAME` reference, and the provider's own variable when the
//! option is left out (including `"provider/model"` shorthands).

#![allow(unsafe_code)]

use std::sync::Arc;

use baml_tests::{engine::OptLevel, stdlib_prefix::compile_source_with_opt};
use bex_engine::{BexCallArg, BexEngine, BexExternalValue, FunctionCallContextBuilder};
use sys_native::SysOpsExt as _;
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

const SOURCE: &str = r#"
// An explicit `env.NAME` for the credential and the endpoint.
client ResponsesRef = openai.ResponsesClient.new(model = "m", api_key = env.LATE_KEY, base_url = env.LATE_BASE_URL);
client ChatRef = openai.ChatClient.new(model = "m", api_key = env.LATE_KEY, base_url = env.LATE_BASE_URL);
client GenericRef = openai.GenericClient.new(model = "m", api_key = env.LATE_KEY, base_url = env.LATE_BASE_URL);
client AzureRef = openai.AzureClient.new(model = "m", api_key = env.LATE_KEY, base_url = env.LATE_BASE_URL);
client AnthropicRef = anthropic.Client.new(model = "m", api_key = env.LATE_KEY, base_url = env.LATE_BASE_URL);
client GeminiRef = google.GeminiClient.new(model = "m", api_key = env.LATE_KEY, base_url = env.LATE_BASE_URL);
client VertexRef = google.VertexClient.new(model = "m", api_key = env.LATE_KEY, base_url = env.LATE_BASE_URL);

// No credential option: the provider's own variable.
client ResponsesDefault = openai.ResponsesClient.new(model = "m", base_url = env.LATE_BASE_URL);
client ChatDefault = openai.ChatClient.new(model = "m", base_url = env.LATE_BASE_URL);
client AzureDefault = openai.AzureClient.new(model = "m");
client AnthropicDefault = anthropic.Client.new(model = "m", base_url = env.LATE_BASE_URL);
client GeminiDefault = google.GeminiClient.new(model = "m", base_url = env.LATE_BASE_URL);
client BedrockDefault = aws.BedrockClient.new(model = "m", endpoint_url = env.LATE_BASE_URL);

function AskResponsesRef() -> string { client: ResponsesRef prompt: `ping` }
function AskChatRef() -> string { client: ChatRef prompt: `ping` }
function AskGenericRef() -> string { client: GenericRef prompt: `ping` }
function AskAzureRef() -> string { client: AzureRef prompt: `ping` }
function AskAnthropicRef() -> string { client: AnthropicRef prompt: `ping` }
function AskGeminiRef() -> string { client: GeminiRef prompt: `ping` }
function AskVertexRef() -> string { client: VertexRef prompt: `ping` }
function AskResponsesDefault() -> string { client: ResponsesDefault prompt: `ping` }
function AskChatDefault() -> string { client: ChatDefault prompt: `ping` }
function AskAzureDefault() -> string { client: AzureDefault prompt: `ping` }
function AskAnthropicDefault() -> string { client: AnthropicDefault prompt: `ping` }
function AskGeminiDefault() -> string { client: GeminiDefault prompt: `ping` }
function AskBedrockDefault() -> string { client: BedrockDefault prompt: `ping` }
function AskAzureShorthand() -> string { client: "azure/m" prompt: `ping` }
"#;

/// Each function, and the variable its credential comes from.
const CASES: &[(&str, &str)] = &[
    ("AskResponsesRef", "LATE_KEY"),
    ("AskChatRef", "LATE_KEY"),
    ("AskGenericRef", "LATE_KEY"),
    ("AskAzureRef", "LATE_KEY"),
    ("AskAnthropicRef", "LATE_KEY"),
    ("AskGeminiRef", "LATE_KEY"),
    ("AskVertexRef", "LATE_KEY"),
    ("AskResponsesDefault", "OPENAI_API_KEY"),
    ("AskChatDefault", "OPENAI_API_KEY"),
    ("AskAzureDefault", "AZURE_OPENAI_API_KEY"),
    ("AskAnthropicDefault", "ANTHROPIC_API_KEY"),
    ("AskGeminiDefault", "GOOGLE_API_KEY"),
    ("AskBedrockDefault", "AWS_ACCESS_KEY_ID"),
    ("AskAzureShorthand", "AZURE_OPENAI_API_KEY"),
];

/// Every credential variable the cases read.
const CREDENTIALS: &[&str] = &[
    "LATE_KEY",
    "OPENAI_API_KEY",
    "AZURE_OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GOOGLE_API_KEY",
    "AWS_ACCESS_KEY_ID",
];

/// Ambient configuration that would otherwise change what a client resolves.
const CLEARED: &[&str] = &[
    "AWS_PROFILE",
    "AWS_SESSION_TOKEN",
    "AWS_DEFAULT_REGION",
    "AWS_CONFIG_FILE",
    "AWS_SHARED_CREDENTIALS_FILE",
];

/// A successful one-turn reply saying "pong", in the shape of whichever API the
/// request was sent to.
fn reply(request: &Request) -> ResponseTemplate {
    let path = request.url.path();
    let body = if path.ends_with("/responses") {
        r#"{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"pong"}]}],"usage":{"input_tokens":1,"output_tokens":1}}"#
    } else if path.ends_with("/chat/completions") {
        r#"{"id":"c","object":"chat.completion","model":"m","choices":[{"index":0,"message":{"role":"assistant","content":"pong"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#
    } else if path.ends_with("/messages") {
        r#"{"id":"msg","type":"message","role":"assistant","model":"m","content":[{"type":"text","text":"pong"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#
    } else if path.ends_with(":generateContent") {
        r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"pong"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":2}}"#
    } else if path.ends_with("/converse") {
        r#"{"output":{"message":{"role":"assistant","content":[{"text":"pong"}]}},"stopReason":"end_turn","usage":{"inputTokens":1,"outputTokens":1,"totalTokens":2}}"#
    } else {
        return ResponseTemplate::new(404).set_body_string(format!("no reply for {path}"));
    };
    ResponseTemplate::new(200)
        .insert_header("content-type", "application/json")
        .set_body_string(body)
}

async fn server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(reply)
        .mount(&server)
        .await;
    server
}

/// The credential a request carries, however its provider sends it.
fn credential(request: &Request) -> Option<String> {
    let header = |name: &str| {
        request
            .headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    };
    if let Some(authorization) = header("authorization") {
        // SigV4: `AWS4-HMAC-SHA256 Credential=<access key id>/<scope>, ...`.
        if let Some(scope) = authorization.split("Credential=").nth(1) {
            return scope.split('/').next().map(str::to_string);
        }
        return authorization.strip_prefix("Bearer ").map(str::to_string);
    }
    header("x-api-key")
        .or_else(|| header("api-key"))
        .or_else(|| header("x-goog-api-key"))
        .or_else(|| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == "key")
                .map(|(_, value)| value.into_owned())
        })
}

/// Point every client at `base_url` and give every credential a value naming
/// `round`.
fn set_environment(round: u32, base_url: &str) {
    // SAFETY: this file holds one test, which nextest runs in its own process,
    // and no request is in flight while the environment changes.
    unsafe {
        for name in CREDENTIALS {
            std::env::set_var(name, format!("{name}-{round}"));
        }
        std::env::set_var("LATE_BASE_URL", base_url);
        std::env::set_var("AZURE_OPENAI_ENDPOINT", base_url);
        std::env::set_var("AWS_SECRET_ACCESS_KEY", format!("secret-{round}"));
        std::env::set_var("AWS_REGION", "us-east-1");
    }
}

async fn call(engine: &Arc<BexEngine>, function: &str) -> Result<BexExternalValue, String> {
    engine
        .call_function_bound_args(
            &format!("user.{function}"),
            // An LLM function's two implicit parameters, left at their defaults.
            vec![BexCallArg::OmittedDefault, BexCallArg::OmittedDefault],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .map_err(|error| error.to_string())
}

#[tokio::test(flavor = "multi_thread")]
async fn llm_clients_read_the_environment_on_every_request() {
    for name in CLEARED {
        // SAFETY: as in `set_environment`.
        unsafe { std::env::remove_var(name) };
    }
    let servers = [server().await, server().await];

    let engine = Arc::new(
        BexEngine::new_with_runtime_compiler(
            compile_source_with_opt(SOURCE, OptLevel::One),
            Arc::new(sys_ops::SysOps::native()),
            Vec::new(),
            bex_project::runtime_compiler(),
        )
        .expect("engine starts"),
    );

    let mut failures = Vec::new();
    for (round, server) in (1..).zip(&servers) {
        set_environment(round, &server.uri());
        for &(function, variable) in CASES {
            let seen_before = server.received_requests().await.unwrap_or_default().len();
            let result = call(&engine, function).await;
            let requests = server.received_requests().await.unwrap_or_default();
            let sent = requests.get(seen_before..).unwrap_or_default();

            let expected = format!("{variable}-{round}");
            let credentials: Vec<_> = sent.iter().map(credential).collect();
            if result != Ok(BexExternalValue::String("pong".into()))
                || credentials != [Some(expected.clone())]
            {
                failures.push(format!(
                    "round {round} {function}: expected one request to server {round} with \
                     {expected}, got {credentials:?} at {:?}; result {result:?}",
                    sent.iter().map(|r| r.url.path()).collect::<Vec<_>>(),
                ));
            }
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
