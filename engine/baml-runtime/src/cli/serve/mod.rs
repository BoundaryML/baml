mod arg_validation;
mod error;
mod json_response;
mod ping;
use error::BamlError;
use indexmap::IndexMap;
use json_response::Json;

use anyhow::{Context, Result};
use arg_validation::BamlServeValidate;
use axum::{
    extract::{self},
    http::{HeaderName, HeaderValue, StatusCode},
    middleware::Next,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{any, post},
};
use axum_extra::{
    headers::{self, authorization::Basic, Authorization, Header},
    TypedHeader,
};
use baml_types::BamlValue;
use core::pin::Pin;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{path::PathBuf, sync::Arc, task::Poll};
use tokio::{net::TcpListener, sync::RwLock};
use tokio_stream::StreamExt;

use crate::{
    client_registry::ClientRegistry, internal::llm_client::LLMResponse, BamlRuntime,
    FunctionResult, RuntimeContextManager,
};

#[derive(clap::Args, Clone, Debug)]
pub struct ServeArgs {
    #[arg(long, help = "path/to/baml_src", default_value = "./baml_src")]
    pub(super) from: PathBuf,
    #[arg(long, help = "port to expose BAML on", default_value = "2024")]
    port: u16,
    #[arg(long, help = "turn on preview features", default_value = "false")]
    preview: bool,
    #[arg(
        long,
        help = "Generate baml_client without checking for version mismatch",
        default_value_t = false
    )]
    no_version_check: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BamlOptions {
    pub client_registry: Option<ClientRegistry>,
}

impl ServeArgs {
    pub fn run(&self) -> Result<()> {
        if !self.preview {
            log::warn!(
                r#"BAML-over-HTTP API is a preview feature.
                
Please run with --preview, like so:

    {} serve --preview

Please provide feedback and let us know if you run into any issues:

    - join our Discord at https://docs.boundaryml.com/discord, or
    - comment on https://github.com/BoundaryML/baml/issues/892

We expect to stabilize this feature over the next few weeks, but we need
your feedback to do so.

Thanks for trying out BAML!
"#,
                if matches!(
                    std::env::var("npm_lifecycle_event").ok().as_deref(),
                    Some("npx")
                ) {
                    "npx @boundaryml/baml"
                } else {
                    "baml-cli"
                }
            );
            anyhow::bail!("--preview is not set")
        }

        log::warn!(
            r#"BAML-over-HTTP is a preview feature.

Please provide feedback and let us know if you run into any issues:

    - join our Discord at https://docs.boundaryml.com/discord, or
    - comment on https://github.com/BoundaryML/baml/issues/892

We expect to stabilize this feature over the next few weeks, but we need
your feedback to do so.

Thanks for trying out BAML!
"#
        );

        let t: Arc<tokio::runtime::Runtime> = BamlRuntime::get_tokio_singleton()?;

        let (server, tcp_listener) = t.block_on(Server::new(self.from.clone(), self.port))?;

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
    pub async fn new(src_dir: PathBuf, port: u16) -> Result<(Arc<Self>, TcpListener)> {
        let tcp_listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
            .await
            .context(format!(
                "Failed to bind to port {}; try using --port PORT to specify a different port.",
                port
            ))?;

        Ok((
            Arc::new(Self {
                src_dir: src_dir.clone(),
                port,
                b: Arc::new(RwLock::new(BamlRuntime::from_directory(
                    &src_dir,
                    std::env::vars().collect(),
                )?)),
            }),
            tcp_listener,
        ))
    }

    fn enforce_auth(
        basic_creds: Option<&Authorization<Basic>>,
        baml_api_key: Option<&XBamlApiKey>,
    ) -> AuthEnforcementMode {
        let Ok(password) = std::env::var("BAML_PASSWORD") else {
            log::warn!("BAML_PASSWORD not set, skipping auth check");
            return AuthEnforcementMode::NoEnforcement;
        };

        if !password.starts_with("sk-baml") {
            log::warn!("We recommend using BAML_PASSWORD=sk-baml-... so that static analysis tools can detect if you accidentally commit and push your password.")
        }

        if let Some(XBamlApiKey(baml_api_key)) = baml_api_key {
            return if *baml_api_key == password {
                AuthEnforcementMode::EnforceAndPass
            } else {
                AuthEnforcementMode::EnforceAndFail(format!("Incorrect x-baml-api-key"))
            };
        }

        if let Some(Authorization(basic_creds)) = basic_creds {
            return if basic_creds.password() == password {
                AuthEnforcementMode::EnforceAndPass
            } else {
                AuthEnforcementMode::EnforceAndFail(format!(
                    "Incorrect password provided in basic auth"
                ))
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

        // log::info!(
        //     "incoming request triggering middleware, basic auth is {:?} and x-baml-api-key is {:?}",
        //     basic_auth,
        //     baml_api_key
        // );

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
            post(move |b_fn, b_args| s.clone().baml_call_axum(b_fn, b_args)),
        );

        let s = self.clone();
        let app = app.route(
            "/stream/:msg",
            post(move |b_fn, b_args| s.clone().baml_stream_axum2(b_fn, b_args)),
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
        log::info!(
            r#"BAML-over-HTTP listening on port {}, serving from {}

Tip: test that the server is up using `curl http://localhost:{}/_debug/ping`

(You may need to replace "localhost" with the container hostname as appropriate.)
"#,
            self.port,
            self.src_dir.display(),
            self.port,
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

        let ctx_mgr = RuntimeContextManager::new_from_env_vars(std::env::vars().collect(), None);
        let client_registry = b_options.and_then(|options| options.client_registry);

        let locked = self.b.read().await;
        let (result, _trace_id) = locked
            .call_function(b_fn, &args, &ctx_mgr, None, client_registry.as_ref())
            .await;

        match result {
            Ok(function_result) => match function_result.llm_response() {
                LLMResponse::Success(_) => match function_result.parsed_content() {
                    // Just because the LLM returned 2xx doesn't mean that it returned parse-able content!
                    Ok(parsed) => {
                        (StatusCode::OK, Json::<BamlValue>(parsed.into())).into_response()
                    }
                    Err(e) => BamlError::ValidationFailure(format!("{:?}", e)).into_response(),
                },
                LLMResponse::LLMFailure(failure) => {
                    BamlError::ClientError(format!("{:?}", failure.message)).into_response()
                }
                LLMResponse::UserFailure(message) => {
                    BamlError::InvalidArgument(message.clone()).into_response()
                }
                LLMResponse::InternalFailure(message) => {
                    BamlError::InternalError(message.clone()).into_response()
                }
            },
            Err(e) => BamlError::from_anyhow(e).into_response(),
        }
    }

    async fn baml_call_axum(
        self: Arc<Self>,
        extract::Path(b_fn): extract::Path<String>,
        extract::Json(b_args): extract::Json<serde_json::Value>,
    ) -> Response {
        let b_options = match b_args.get("__baml_options") {
            Some(options_value) => {
                match serde_json::from_value::<BamlOptions>(options_value.clone()) {
                    Ok(options) => Some(options),
                    Err(e) => {
                        return BamlError::InvalidArgument(format!(
                            "Failed to parse __baml_options: {:?}",
                            e
                        ))
                        .into_response()
                    }
                }
            }
            None => None,
        };
        log::info!("Received client registry: {:?}", b_options);

        self.baml_call(b_fn, b_args, b_options).await
    }

    fn baml_stream(self: Arc<Self>, b_fn: String, b_args: serde_json::Value) -> Response {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

        let args = match parse_args(&b_fn, b_args) {
            Ok(args) => args,
            Err(e) => return e.into_response(),
        };

        tokio::spawn(async move {
            let ctx_mgr =
                RuntimeContextManager::new_from_env_vars(std::env::vars().collect(), None);

            let result_stream = self
                .b
                .read()
                .await
                .stream_function(b_fn, &args, &ctx_mgr, None, None);

            match result_stream {
                Ok(mut result_stream) => {
                    let (result, _trace_id) = result_stream
                        .run(
                            Some(move |result| {
                                // If the receiver is closed (either because it called close or it was dropped),
                                // we can't really do anything
                                match sender.send(result) {
                                    Ok(_) => (),
                                    Err(e) => {
                                        log::error!("Error sending result to receiver: {:?}", e);
                                    }
                                }
                            }),
                            &ctx_mgr,
                            None,
                            None,
                        )
                        .await;

                    match result {
                        Ok(function_result) => match function_result.llm_response() {
                            LLMResponse::Success(_) => match function_result.parsed_content() {
                                // Just because the LLM returned 2xx doesn't mean that it returned parse-able content!
                                Ok(parsed) => {
                                    dbg!(parsed);
                                    (StatusCode::OK, Json::<BamlValue>(parsed.into()))
                                        .into_response()
                                }

                                Err(e) => {
                                    log::debug!("Error parsing content: {:?}", e);
                                    BamlError::ValidationFailure(format!("{:?}", e)).into_response()
                                }
                            },
                            LLMResponse::LLMFailure(failure) => {
                                log::debug!("LLMResponse::LLMFailure: {:?}", failure);
                                BamlError::ClientError(format!("{:?}", failure.message))
                                    .into_response()
                            }
                            LLMResponse::UserFailure(message) => {
                                BamlError::InvalidArgument(message.clone()).into_response()
                            }
                            LLMResponse::InternalFailure(message) => {
                                BamlError::InternalError(message.clone()).into_response()
                            }
                        },
                        Err(e) => BamlError::from_anyhow(e).into_response(),
                    }
                }
                Err(e) => BamlError::InternalError(format!("Error starting stream: {:?}", e))
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
    async fn baml_stream_axum2(
        self: Arc<Self>,
        extract::Path(path): extract::Path<String>,
        extract::Json(body): extract::Json<serde_json::Value>,
    ) -> Response {
        self.baml_stream(path, body)
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
            Poll::Ready(Some(item)) => match item.parsed_content() {
                // TODO: not sure if this is the correct way to implement this.
                Ok(parsed) => Poll::Ready(Some(parsed.into())),
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
            return Err(BamlError::InvalidArgument(
                format!("POST data must be valid JSON: {:?}", e).into(),
            ));
        }
    };

    let args = match args {
        serde_json::Value::Object(v) => v,
        _ => {
            return Err(BamlError::InvalidArgument(
                format!("POST data must be a JSON map of the arguments for BAML function {b_fn}, from arg name to value").into()
            ));
        }
    };

    let args: IndexMap<String, BamlValue> = match args
        .into_iter()
        .map(|(k, v)| serde_json::from_value(v).map(|v| (k, v)))
        .collect::<serde_json::Result<_>>()
    {
        Ok(v) => v,
        Err(e) => {
            return Err(BamlError::InvalidArgument(
                format!(
                    "Arguments must be convertible from JSON to BamlValue: {:?}",
                    e
                )
                .into(),
            ));
        }
    };

    for (_, v) in args.iter() {
        v.validate_for_baml_serve()?;
    }

    Ok(args)
}
