mod arg_validation;
mod error;
mod json_response;
mod ping;
use core::pin::Pin;
use std::{collections::HashMap, path::PathBuf, sync::Arc, task::Poll};

use anyhow::{Context, Result};
use arg_validation::BamlServeValidate;
use axum::{
    extract::{self},
    http::{HeaderName, HeaderValue, StatusCode},
    middleware::Next,
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse, Response,
    },
    routing::{any, get, post},
};
use axum_extra::{
    headers::{self, authorization::Basic, Authorization, Header},
    TypedHeader,
};
use baml_types::{
    expr::{Expr, ExprMetadata},
    BamlValue, GeneratorDefaultClientMode, GeneratorOutputType,
};
use error::BamlError;
use futures::Stream;
use generators_lib::GeneratorArgs;
use generators_openapi::OpenApiSchema;
use indexmap::IndexMap;
use json_response::Json;
use jsonish::ResponseBamlValue;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{net::TcpListener, sync::RwLock};
use tokio_stream::StreamExt;

use crate::{
    cli::dotenv::DotenvArgs, client_registry::ClientRegistry, errors::ExposedError,
    internal::llm_client::LLMResponse, BamlRuntime, FunctionResult, RuntimeContextManager,
    TripWire,
};

#[derive(clap::Args, Clone, Debug)]
pub struct ServeArgs {
    #[arg(long, help = "path/to/baml_src", default_value = "./baml_src")]
    pub from: PathBuf,
    #[arg(long, help = "port to expose BAML on", default_value = "2024")]
    port: u16,
    #[arg(
        long,
        help = "Generate baml_client without checking for version mismatch",
        default_value_t = false
    )]
    no_version_check: bool,
    #[command(flatten)]
    dotenv: DotenvArgs,
}

#[derive(Deserialize, Clone, Debug)]
pub struct BamlOptions {
    pub client_registry: Option<ClientRegistry>,
    #[serde(default)]
    pub env: HashMap<String, Option<String>>,
}

impl BamlOptions {
    fn env(&self) -> HashMap<String, String> {
        if self.env.is_empty() {
            return std::env::vars().collect();
        }
        let mut env = std::env::vars().collect::<HashMap<String, String>>();
        for (k, v) in &self.env {
            match v {
                Some(v) => env.insert(k.clone(), v.clone()),
                None => env.remove(k),
            };
        }
        env
    }
}

impl ServeArgs {
    pub fn run(
        &self,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<()> {
        self.dotenv.load()?;

        let t: Arc<tokio::runtime::Runtime> = BamlRuntime::get_tokio_singleton()?;

        let (server, tcp_listener) =
            t.block_on(Server::new(self.from.clone(), self.port, feature_flags))?;

        t.block_on(server.serve(tcp_listener))?;

        Ok(())
    }
}

/// State of the server.
///
/// We could maybe use axum's State extractor to pass this around instead, but I
/// don't think that particularly simplifies things and am not sure if it necessarily
/// removes complexity at all.
pub(super) struct Server {
    src_dir: PathBuf,
    port: u16,
    pub(super) b: Arc<RwLock<BamlRuntime>>,
}

#[derive(Debug)]
struct XBamlApiKey(String);

impl Header for XBamlApiKey {
    fn name() -> &'static HeaderName {
        static NAME: HeaderName = HeaderName::from_static("x-baml-api-key");
        &NAME
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, headers::Error>
    where
        I: Iterator<Item = &'i HeaderValue>,
    {
        let value = values.next().ok_or_else(headers::Error::invalid)?;
        let api_key = value.to_str().map_err(|_| headers::Error::invalid())?;
        Ok(Self(api_key.to_owned()))
    }

    fn encode<E>(&self, values: &mut E)
    where
        E: Extend<HeaderValue>,
    {
        if let Ok(value) = HeaderValue::try_from(self.0.as_str()) {
            values.extend(std::iter::once(value));
        }
    }
}

async fn status_handler(
    basic_creds: Option<TypedHeader<Authorization<Basic>>>,
    baml_api_key: Option<TypedHeader<XBamlApiKey>>,
) -> Response {
    match Server::enforce_auth(basic_creds.as_deref(), baml_api_key.as_deref()) {
        AuthEnforcementMode::EnforceAndFail(e) => (
            StatusCode::FORBIDDEN,
            Json(json!({
                "authz": {
                    "enforcement": "active",
                    "outcome": "fail",
                    "reason": e
                },
            })),
        ),
        AuthEnforcementMode::EnforceAndPass => (
            StatusCode::OK,
            Json(json!({
                "authz": {
                    "enforcement": "active",
                    "outcome": "pass"
                },
            })),
        ),
        AuthEnforcementMode::NoEnforcement => (
            StatusCode::OK,
            Json(json!({
                "authz": {
                    "enforcement": "none",
                },
            })),
        ),
    }
    .into_response()
}

enum AuthEnforcementMode {
    NoEnforcement,
    EnforceAndPass,
    EnforceAndFail(String),
}

impl Server {
    pub async fn new(
        src_dir: PathBuf,
        port: u16,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<(Arc<Self>, TcpListener)> {
        let tcp_listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
            .await
            .context(format!(
                "Failed to bind to port {port}; try using --port PORT to specify a different port."
            ))?;
        let baml_runtime =
            BamlRuntime::from_directory(&src_dir, std::env::vars().collect(), feature_flags)?;
        Ok((
            Arc::new(Self {
                src_dir: src_dir.clone(),
                port,
                b: Arc::new(RwLock::new(baml_runtime)),
            }),
            tcp_listener,
        ))
    }

    fn enforce_auth(
        basic_creds: Option<&Authorization<Basic>>,
        baml_api_key: Option<&XBamlApiKey>,
    ) -> AuthEnforcementMode {
        let Ok(password) = std::env::var("BAML_PASSWORD") else {
            log_once::warn_once!("BAML_PASSWORD not set, skipping auth check");
            return AuthEnforcementMode::NoEnforcement;
        };

        if !password.starts_with("sk-baml") {
            baml_log::warn!("We recommend using BAML_PASSWORD=sk-baml-... so that static analysis tools can detect if you accidentally commit and push your password.")
        }

        if let Some(XBamlApiKey(baml_api_key)) = baml_api_key {
            return if *baml_api_key == password {
                AuthEnforcementMode::EnforceAndPass
            } else {
                AuthEnforcementMode::EnforceAndFail("Incorrect x-baml-api-key".to_string())
            };
        }

        if let Some(Authorization(basic_creds)) = basic_creds {
            return if basic_creds.password() == password {
                AuthEnforcementMode::EnforceAndPass
            } else {
                AuthEnforcementMode::EnforceAndFail(
                    "Incorrect password provided in basic auth".to_string(),
                )
            };
        }

        AuthEnforcementMode::EnforceAndFail("No authorization metadata".to_owned())
    }

    async fn auth_middleware(
        basic_auth: Option<TypedHeader<Authorization<Basic>>>,
        baml_api_key: Option<TypedHeader<XBamlApiKey>>,
        request: extract::Request,
        next: Next,
    ) -> Response {
        log::debug!("Handling request for {}", request.uri());

        // Skip auth checks for these endpoints.
        if request.uri() == "/_debug/ping" || request.uri() == "/_debug/status" {
            return next.run(request).await;
        }
        if let AuthEnforcementMode::EnforceAndFail(e) =
            Server::enforce_auth(basic_auth.as_deref(), baml_api_key.as_deref())
        {
            return (StatusCode::FORBIDDEN, format!("{}\n", e.trim())).into_response();
        }

        next.run(request).await
    }

    pub async fn serve(self: Arc<Self>, tcp_listener: TcpListener) -> Result<()> {
        // build our application with a route
        let app = axum::Router::new();

        let app = app.route("/_debug/ping", any(ping::ping_handler));
        let app = app.route("/_debug/status", any(status_handler));

        let s = self.clone();
        let app = app.route(
            "/call/:msg",
            post(
                move |extract::Path(b_fn): extract::Path<String>,
                      extract::Json(b_args): extract::Json<serde_json::Value>| async move {
                    s.clone().baml_call_axum(b_fn, b_args).await
                },
            ),
        );

        let s = self.clone();
        let app = app.route(
            "/stream/:msg",
            post(
                move |extract::Path(b_fn): extract::Path<String>,
                      extract::Json(b_args): extract::Json<serde_json::Value>| async move {
                    s.clone().baml_stream_axum2(b_fn, b_args).await
                },
            ),
        );
        let s = self.clone();
        let app = app.route("/docs", get(move || s.clone().docs_handler()));

        let s = self.clone();
        let app = app.route(
            "/openapi.json",
            get(move || s.clone().openapi_json_handler()),
        );

        let service = axum::serve(
            tcp_listener,
            app.layer(axum::middleware::from_fn(Server::auth_middleware)),
        );
        // TODO: we do not handle this ourselves, because tokio's default
        // handling is pretty good on unix.
        //
        // Not totally sure if this WAI on
        // windows, but there are some potential pitfalls that we can run into
        // if we try to handle this ourselves, see
        // https://docs.rs/tokio/latest/tokio/signal/fn.ctrl_c.html#caveats
        //
        // Namely- we need to ensure resilient delivery of Ctrl-C to everything, and I
        // suspect we need to do a bit of work to ensure that we handle that bookkeeping
        // correctly. Shutting down the BAML runtime, tokio runtime, _and_ axum is not
        // super straightforward, because I don't know how much is handled for us
        // out of the box.
        //
        // .with_graceful_shutdown(signal::ctrl_c());
        baml_log::info!(
            r#"BAML-over-HTTP listening on port {port}, serving from {src_dir}

Tip: test that the server is up using `curl http://localhost:{port}/_debug/ping`

(You may need to replace "localhost" with the container hostname as appropriate.)

Once the server is up, open http://localhost:{port}/docs in the browser to test your routes interactively.

Streaming is available via http://localhost:{port}/stream/{{FunctionName}}, but not added to openapi.yaml (no partial types yet).
"#,
            port = self.port,
            src_dir = self.src_dir.display(),
        );

        service.await?;

        Ok(())
    }

    async fn baml_call(
        self: Arc<Self>,
        b_fn: String,
        b_args: serde_json::Value,
        b_options: Option<BamlOptions>,
    ) -> Response {
        let args = match parse_args(&b_fn, b_args) {
            Ok(args) => args,
            Err(e) => return e.into_response(),
        };

        let client_registry = b_options
            .clone()
            .and_then(|options| options.client_registry);

        let locked = self.b.read().await;
        let env_vars: HashMap<String, String> = b_options
            .as_ref()
            .map_or_else(|| std::env::vars().collect(), |options| options.env());
        let (result, _trace_id) = locked
            .call_function(
                b_fn,
                &args,
                &Default::default(),
                None,
                client_registry.as_ref(),
                None,
                env_vars,
                None, // tags
                TripWire::new(None),
            )
            .await;

        match result {
            Ok(function_result) => match function_result.llm_response() {
                LLMResponse::Success(_) => {
                    match function_result.result_with_constraints_content() {
                        // Just because the LLM returned 2xx doesn't mean that it returned parse-able content!
                        Ok(parsed) => {
                            (StatusCode::OK, Json(parsed.serialize_final())).into_response()
                        }
                        Err(e) => {
                            if let Some(ExposedError::ValidationError {
                                prompt,
                                raw_output: raw_response,
                                message,
                                ..
                            }) = e.downcast_ref::<ExposedError>()
                            {
                                BamlError::ValidationFailure {
                                    message: message.clone(),
                                    prompt: prompt.clone(),
                                    raw_output: raw_response.clone(),
                                }
                                .into_response()
                            } else {
                                BamlError::InternalError {
                                    message: format!("Error parsing: {e:?}"),
                                }
                                .into_response()
                            }
                        }
                    }
                }
                LLMResponse::LLMFailure(failure) => BamlError::ClientError {
                    message: format!("{:?}", failure.message),
                }
                .into_response(),
                LLMResponse::UserFailure(message) => BamlError::InvalidArgument {
                    message: message.clone(),
                }
                .into_response(),
                LLMResponse::InternalFailure(message) => BamlError::InternalError {
                    message: message.clone(),
                }
                .into_response(),
                LLMResponse::Cancelled(message) => BamlError::InternalError {
                    message: format!("Cancelled: {message}"),
                }
                .into_response(),
            },
            Err(e) => BamlError::from_anyhow(e).into_response(),
        }
    }

    async fn baml_call_axum(self: Arc<Self>, b_fn: String, b_args: serde_json::Value) -> Response {
        let mut b_options = None;
        if let Some(options_value) = b_args.get("__baml_options__") {
            match BamlOptions::deserialize(options_value) {
                Ok(opts) => b_options = Some(opts),
                Err(e) => {
                    return BamlError::InvalidArgument {
                        message: format!("Failed to parse __baml_options__: {e}"),
                    }
                    .into_response()
                }
            }
        }
        self.baml_call(b_fn, b_args, b_options).await
    }

    fn baml_stream(
        self: Arc<Self>,
        b_fn: String,
        b_args: serde_json::Value,
        b_options: Option<BamlOptions>,
    ) -> Response {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

        let args = match parse_args(&b_fn, b_args) {
            Ok(args) => args,
            Err(e) => return e.into_response(),
        };

        let client_registry = b_options
            .clone()
            .and_then(|options| options.client_registry);

        tokio::spawn(async move {
            let env_vars: HashMap<String, String> = b_options
                .as_ref()
                .map_or_else(|| std::env::vars().collect(), |options| options.env());

            let result_stream = self.b.read().await.stream_function(
                b_fn,
                &args,
                &Default::default(),
                None,
                client_registry.as_ref(),
                Some(vec![]),
                env_vars,
                TripWire::new(None),
                None, // tags
            );

            match result_stream {
                Ok(mut result_stream) => {
                    let (result, _trace_id) = result_stream
                        .run(
                            None::<fn()>,
                            Some(move |result| {
                                // If the receiver is closed (either because it called close or it was dropped),
                                // we can't really do anything
                                match sender.send(result) {
                                    Ok(_) => (),
                                    Err(e) => {
                                        log::error!("Error sending result to receiver: {e:?}");
                                    }
                                }
                            }),
                            &Default::default(),
                            None,
                            None,
                            HashMap::new(),
                        )
                        .await;

                    match result {
                        Ok(function_result) => match function_result.llm_response() {
                            LLMResponse::Success(_) => {
                                match function_result.result_with_constraints_content() {
                                    // Just because the LLM returned 2xx doesn't mean that it returned parse-able content!
                                    Ok(parsed) => {
                                        (StatusCode::OK, Json(&parsed.serialize_partial()))
                                            .into_response()
                                    }

                                    Err(e) => {
                                        log::debug!("Error parsing content: {e:?}");
                                        if let Some(ExposedError::ValidationError {
                                            prompt,
                                            raw_output: raw_response,
                                            message,
                                            ..
                                        }) = e.downcast_ref::<ExposedError>()
                                        {
                                            BamlError::ValidationFailure {
                                                message: message.clone(),
                                                prompt: prompt.clone(),
                                                raw_output: raw_response.clone(),
                                            }
                                            .into_response()
                                        } else {
                                            BamlError::InternalError {
                                                message: format!("Error parsing: {e:?}"),
                                            }
                                            .into_response()
                                        }
                                    }
                                }
                            }
                            LLMResponse::LLMFailure(failure) => {
                                log::debug!("LLMResponse::LLMFailure: {failure:?}");
                                BamlError::ClientError {
                                    message: format!("{:?}", failure.message),
                                }
                                .into_response()
                            }
                            LLMResponse::UserFailure(message) => BamlError::InvalidArgument {
                                message: message.clone(),
                            }
                            .into_response(),
                            LLMResponse::InternalFailure(message) => BamlError::InternalError {
                                message: message.clone(),
                            }
                            .into_response(),
                            LLMResponse::Cancelled(message) => BamlError::InternalError {
                                message: format!("Cancelled: {message}"),
                            }
                            .into_response(),
                        },
                        Err(e) => BamlError::from_anyhow(e).into_response(),
                    }
                }
                Err(e) => BamlError::InternalError {
                    message: format!("Error starting stream: {e:?}"),
                }
                .into_response(),
            }
        });

        // TODO: streaming is broken. the above should return first.
        let stream = Box::pin(EventStream { receiver }).map(|bv| Event::default().json_data(bv));

        Sse::new(stream)
            .keep_alive(KeepAlive::default())
            .into_response()
    }

    // newline-delimited can be implemented using axum_streams::StreamBodyAs::json_nl(self.baml_stream(path, body))
    async fn baml_stream_axum2(self: Arc<Self>, path: String, body: serde_json::Value) -> Response {
        let mut b_options = None;
        if let Some(options_value) = body.get("__baml_options__") {
            match BamlOptions::deserialize(options_value) {
                Ok(opts) => b_options = Some(opts),
                Err(e) => {
                    return BamlError::InvalidArgument {
                        message: format!("Failed to parse __baml_options__: {e}"),
                    }
                    .into_response()
                }
            }
        }
        self.baml_stream(path, body, b_options)
    }

    /// Serve an HTML page that loads swagger-ui from local static files.
    /// This page will in turn fetch `/openapi.json`, and use the results
    /// to build interactive documentation.
    async fn docs_handler(self: Arc<Self>) -> Response {
        let page = r#"
<html>
    <head>
        <title>
            BAML Function Docs
        </title>
        <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/swagger-ui/5.17.14/swagger-ui.css" integrity="sha512-MvYROlKG3cDBPskMQgPmkNgZh85LIf68y7SZ34TIppaIHQz1M/3S/yYqzIfufdKDJjzB9Qu1BV63SZjimJkPvw==" crossorigin="anonymous" referrerpolicy="no-referrer" />
        <script language="javascript">

            window.onload = function() {
              //<editor-fold desc="Changeable Configuration Block">

              // the following lines will be replaced by docker/configurator, when it runs in a docker-container
              window.ui = SwaggerUIBundle({
                url: "/openapi.json",
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [
                  SwaggerUIBundle.presets.apis,
                  SwaggerUIStandalonePreset
                ],
                plugins: [
                  SwaggerUIBundle.plugins.DownloadUrl
                ],
                layout: "StandaloneLayout"
              });

              //</editor-fold>
            };
        </script>
    </head>
    <body>
        <div id="swagger-ui"></div>
        <script src="https://cdnjs.cloudflare.com/ajax/libs/swagger-ui/5.17.14/swagger-ui-bundle.js" integrity="sha512-mVvFSCxt0sK0FeL8C7n8BcHh10quzdwfxQbjRaw9pRdKNNep3YQusJS5e2/q4GYt4Ma5yWXSJraoQzXPgZd2EQ==" crossorigin="anonymous" referrerpolicy="no-referrer"></script>
        <script src="https://cdnjs.cloudflare.com/ajax/libs/swagger-ui/5.17.14/swagger-ui-standalone-preset.js" integrity="sha512-DgicCd4AI/d7/OdgaHqES3hA+xJ289Kb5NmMEegbN8w/Dxn5mvvqr9szOR6TQC+wjTTMeqPscKE4vj6bmAQn6g==" crossorigin="anonymous" referrerpolicy="no-referrer"></script>
        <script src="./swagger-initializer.js" charset="UTF-8"> </script>
    </body>
</html>
"#;
        Html(page.to_string()).into_response()
    }

    /// Render the openapi spec. This endpoint is used by the swagger ui.
    async fn openapi_json_handler(self: Arc<Self>) -> Result<String, BamlError> {
        let locked = self.b.read().await;
        let fake_generator = GeneratorArgs::new(
            "fake_directory",
            "fake_directory",
            Vec::new(),
            "fake-version".to_string(),
            true,
            GeneratorDefaultClientMode::Sync,
            Vec::new(),
            GeneratorOutputType::OpenApi,
            None,
            None,
        )
        .map_err(|_| BamlError::InternalError {
            message: "Failed to make placeholder generator".to_string(),
        })?;
        let schema: OpenApiSchema = OpenApiSchema::from_ir(locked.ir.as_ref());
        serde_json::to_string(&schema).map_err(|e| {
            log::warn!("Failed to serialize openapi schema: {e}");
            BamlError::InternalError {
                message: "Failed to serialize openapi schema".to_string(),
            }
        })
    }
}

struct EventStream {
    receiver: tokio::sync::mpsc::UnboundedReceiver<FunctionResult>,
}

impl Stream for EventStream {
    type Item = BamlValue;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        match self.receiver.poll_recv(cx) {
            Poll::Ready(Some(item)) => match item.result_with_constraints_content() {
                // TODO: not sure if this is the correct way to implement this.
                Ok(parsed) => Poll::Ready(Some(parsed.0.clone().into())),
                Err(_) => Poll::Pending,
            },
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

fn parse_args(
    b_fn: &str,
    b_args: serde_json::Value,
) -> Result<IndexMap<String, BamlValue>, BamlError> {
    // We do this conversion in a 3-step POST -> JSON -> Map -> Map<String,
    // BamlValue>, instead of a 2-step POST -> BamlValue -> BamlValue::Map,
    // because this approach lets us provide the users with better errors.

    let args: serde_json::Value = match serde_json::from_value(b_args) {
        Ok(v) => v,
        Err(e) => {
            return Err(BamlError::InvalidArgument {
                message: format!("POST data must be valid JSON: {e:?}"),
            });
        }
    };

    let args = match args {
        serde_json::Value::Object(v) => v,
        _ => {
            return Err(BamlError::InvalidArgument {
                message: format!(
                    "POST data must be a JSON map of the arguments for BAML function {b_fn}, from arg name to value"
                ),
            });
        }
    };

    let args: IndexMap<String, BamlValue> = match args
        .into_iter()
        .map(|(k, v)| serde_json::from_value(v).map(|v| (k, v)))
        .collect::<serde_json::Result<_>>()
    {
        Ok(v) => v,
        Err(e) => {
            return Err(BamlError::InvalidArgument {
                message: format!("Arguments must be convertible from JSON to BamlValue: {e:?}"),
            });
        }
    };

    for (_, v) in args.iter() {
        v.validate_for_baml_serve()?;
    }

    Ok(args)
}

#[cfg(test)]
mod tests {
    use baml_types::BamlMap;
    use internal_llm_client::{ClientProvider, OpenAIClientProviderVariant};

    use super::*;
    use crate::client_registry::ClientProperty;

    #[test]
    fn test_parse_baml_options() {
        let baml_options: BamlOptions = serde_json::from_str(
            r#"
        {
            "client_registry": {
                "clients": [
                    {
                        "name": "testing",
                        "provider": "openai",
                        "options": {
                            "model": "gpt-4o",
                            "api_key": "[redacted]",
                            "base_url": "[redacted]"
                        }
                    }
                ],
                "primary": "testing"
            }
        }"#,
        )
        .unwrap();
        assert!(
            baml_options.client_registry.is_some(),
            "client_registry should be Some"
        );
        let client_registry = baml_options.client_registry.unwrap();

        let provider = ClientProvider::OpenAI(OpenAIClientProviderVariant::Base);
        let retry_policy = None;
        let options = BamlMap::from_iter(vec![
            ("model".to_string(), BamlValue::String("gpt-4o".to_string())),
            (
                "api_key".to_string(),
                BamlValue::String("[redacted]".to_string()),
            ),
            (
                "base_url".to_string(),
                BamlValue::String("[redacted]".to_string()),
            ),
        ]);
        let client_property =
            ClientProperty::new("testing".into(), provider, retry_policy, options);

        let expected_client_registry = {
            let mut client_registry = ClientRegistry::new();
            client_registry.add_client(client_property);
            client_registry.set_primary("testing".to_string());
            client_registry
        };
        assert_eq!(client_registry, expected_client_registry);
    }

    #[test]
    fn test_parse_baml_options_with_env() {
        // Set up a dummy env var to test removal
        std::env::set_var("REMOVE_ME", "should_be_removed");
        std::env::set_var("KEEP_ME", "should_be_overwritten");

        let baml_options: BamlOptions = serde_json::from_str(
            r#"
        {
            "env": {
                "NEW_VAR": "new_value",
                "KEEP_ME": "new_value",
                "REMOVE_ME": null
            }
        }"#,
        )
        .unwrap();

        // The env() method should:
        // - Add NEW_VAR
        // - Overwrite KEEP_ME
        // - Remove REMOVE_ME
        let env_map = baml_options.env();

        assert_eq!(env_map.get("NEW_VAR"), Some(&"new_value".to_string()));
        assert_eq!(env_map.get("KEEP_ME"), Some(&"new_value".to_string()));
        assert!(!env_map.contains_key("REMOVE_ME"));
    }
}
