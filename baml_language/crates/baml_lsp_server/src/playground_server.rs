//! Lightweight HTTP server for the BAML Playground.
//!
//! Two modes controlled by environment variables:
//!
//! **Dev mode** (`BAML_PLAYGROUND_DEV_PORT` is set):
//!   Reverse-proxies all non-API requests to a local Vite dev server.
//!
//! **Prod mode** (`BAML_PLAYGROUND_DIR` is set):
//!   Serves pre-built static assets with SPA fallback.

use std::{net::SocketAddr, sync::Arc};

use axum::{
    Router,
    body::Body,
    extract::{
        FromRequestParts, State,
        ws::{Message as AxumWsMsg, WebSocket, WebSocketUpgrade},
    },
    http::{Method, Request, StatusCode, header},
    middleware::{self, Next},
    response::Response,
    routing::get,
};
use base64::Engine as _;
use futures::{SinkExt, stream::StreamExt};
use prost::Message;
use tokio::{net::TcpListener, sync::broadcast};

use crate::{
    playground_env::PlaygroundEnvState,
    playground_ws::{WsInMessage, WsOutMessage},
};

fn to_ws_text(msg: &WsOutMessage) -> Option<AxumWsMsg> {
    match serde_json::to_string(msg) {
        Ok(json) => Some(AxumWsMsg::Text(json.into())),
        Err(e) => {
            tracing::error!("Playground WS: failed to serialize message: {e}");
            None
        }
    }
}

/// Find an available TCP port starting from `base_port`.
pub async fn pick_port(base_port: u16, max_attempts: u16) -> anyhow::Result<(TcpListener, u16)> {
    for offset in 0..max_attempts {
        let port = base_port + offset;
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        match TcpListener::bind(addr).await {
            Ok(listener) => return Ok((listener, port)),
            Err(_) => continue,
        }
    }
    anyhow::bail!(
        "Could not find an available port in range {}..{}",
        base_port,
        base_port + max_attempts
    )
}

// ---------------------------------------------------------------------------
// Shared state for Axum handlers
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct WsState {
    bex: Arc<dyn bex_project::BexLsp>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    env_state: Arc<PlaygroundEnvState>,
}

/// Start the playground server on the given listener.
pub async fn run(
    listener: TcpListener,
    bex: Arc<dyn bex_project::BexLsp>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    env_state: Arc<PlaygroundEnvState>,
) -> anyhow::Result<()> {
    let app = build_router(bex, broadcast_tx, env_state)?;

    tracing::info!(
        "Playground: http://localhost:{}",
        listener.local_addr()?.port()
    );

    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("Playground server error: {e}"))
}

fn build_router(
    bex: Arc<dyn bex_project::BexLsp>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    env_state: Arc<PlaygroundEnvState>,
) -> anyhow::Result<Router> {
    let ws_state = WsState {
        bex,
        broadcast_tx,
        env_state,
    };

    let api = Router::new()
        .route("/api/ws", get(playground_ws_handler))
        .with_state(ws_state);

    let fallback = if let Ok(dev_port) = std::env::var("BAML_PLAYGROUND_DEV_PORT") {
        let dev_port: u16 = dev_port
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid BAML_PLAYGROUND_DEV_PORT: {e}"))?;
        tracing::info!("Playground: dev proxy -> http://localhost:{dev_port}");
        dev_proxy_router(format!("http://localhost:{dev_port}"))
    } else if let Ok(dir) = std::env::var("BAML_PLAYGROUND_DIR") {
        tracing::info!("Playground: serving static files from {dir}");
        static_router(dir)
    } else {
        anyhow::bail!(
            "Playground server requires either BAML_PLAYGROUND_DEV_PORT or BAML_PLAYGROUND_DIR"
        )
    };

    Ok(api
        .fallback_service(fallback)
        .layer(middleware::from_fn(cors_middleware)))
}

// ---------------------------------------------------------------------------
// WebSocket handler
// ---------------------------------------------------------------------------

async fn playground_ws_handler(State(state): State<WsState>, ws: WebSocketUpgrade) -> Response {
    tracing::info!("Playground: /api/ws upgrade request received");
    ws.on_upgrade(move |socket| playground_ws_session(socket, state))
}

async fn playground_ws_session(socket: WebSocket, state: WsState) {
    tracing::info!("Playground: WS session started");
    let (mut sink, mut stream) = socket.split();

    if let Some(ready) = to_ws_text(&WsOutMessage::Ready) {
        if sink.send(ready).await.is_err() {
            return;
        }
    } else {
        return;
    }

    // Send current playground state.
    state.bex.request_playground_state();

    let mut broadcast_rx = state.broadcast_tx.subscribe();

    loop {
        tokio::select! {
            client_msg = stream.next() => {
                match client_msg {
                    Some(Ok(AxumWsMsg::Text(text))) => {
                        let text_str: &str = &text;
                        match serde_json::from_str::<WsInMessage>(text_str) {
                            Ok(msg) => {
                                handle_ws_in_message(msg, &state, &mut sink).await;
                            }
                            Err(e) => {
                                tracing::warn!("Playground WS: invalid message: {e}");
                            }
                        }
                    }
                    Some(Ok(AxumWsMsg::Close(_))) | None => break,
                    _ => {}
                }
            }
            broadcast_msg = broadcast_rx.recv() => {
                match broadcast_msg {
                    Ok(msg) => {
                        if let Some(ws_msg) = to_ws_text(&msg)
                            && sink.send(ws_msg).await.is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Playground WS: broadcast lagged by {n} messages");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    tracing::debug!("Playground WS session ended");
}

async fn handle_ws_in_message(
    msg: WsInMessage,
    state: &WsState,
    sink: &mut futures::stream::SplitSink<WebSocket, AxumWsMsg>,
) {
    match msg {
        WsInMessage::CallFunction {
            id,
            project,
            name,
            args_proto,
        } => {
            let decoded = match base64::engine::general_purpose::STANDARD.decode(&args_proto) {
                Ok(d) => d,
                Err(e) => {
                    let err_msg = WsOutMessage::CallFunctionError {
                        id,
                        error: format!("Invalid base64: {e}"),
                    };
                    if let Some(ws_msg) = to_ws_text(&err_msg) {
                        let _ = sink.send(ws_msg).await;
                    }
                    return;
                }
            };

            let args = match bridge_ctypes::baml::cffi::CallFunctionArgs::decode(decoded.as_slice())
            {
                Ok(a) => a,
                Err(e) => {
                    let err_msg = WsOutMessage::CallFunctionError {
                        id,
                        error: format!("Failed to decode arguments: {e}"),
                    };
                    if let Some(ws_msg) = to_ws_text(&err_msg) {
                        let _ = sink.send(ws_msg).await;
                    }
                    return;
                }
            };

            let kwargs = match bridge_ctypes::kwargs_to_bex_values(
                args.kwargs,
                &bridge_ctypes::HANDLE_TABLE,
            ) {
                Ok(k) => k,
                Err(e) => {
                    let err_msg = WsOutMessage::CallFunctionError {
                        id,
                        error: format!("Failed to convert arguments: {e}"),
                    };
                    if let Some(ws_msg) = to_ws_text(&err_msg) {
                        let _ = sink.send(ws_msg).await;
                    }
                    return;
                }
            };

            let broadcast_tx = state.broadcast_tx.clone();
            let call_id = sys_types::CallId(id);
            let fs_path = bex_project::FsPath::from_str(project);

            let function_call_ctx = bex_project::FunctionCallContextBuilder::new(call_id);

            let bex = match state.bex.get_bex_for_project(&fs_path).map_err(|e| {
                WsOutMessage::CallFunctionError {
                    id,
                    error: format!("Failed to get Bex for project: {e}"),
                }
            }) {
                Ok(bex) => bex,
                Err(e) => {
                    if let Some(ws_msg) = to_ws_text(&e) {
                        let _ = sink.send(ws_msg).await;
                    }
                    return;
                }
            };

            tokio::spawn(async move {
                let handle_options = bridge_ctypes::HandleTableOptions::for_wire();
                let out = match bex
                    .call_function(&name, kwargs.into(), function_call_ctx.build())
                    .await
                {
                    Ok(result) => {
                        match bridge_ctypes::external_to_baml_value(&result, &handle_options) {
                            Ok(baml_val) => {
                                let b64 = base64::engine::general_purpose::STANDARD
                                    .encode(baml_val.encode_to_vec());
                                WsOutMessage::CallFunctionResult { id, result: b64 }
                            }
                            Err(e) => WsOutMessage::CallFunctionError {
                                id,
                                error: format!("Failed to encode result: {e}"),
                            },
                        }
                    }
                    Err(e) => WsOutMessage::CallFunctionError {
                        id,
                        error: format!("{e}"),
                    },
                };
                let _ = broadcast_tx.send(out);
            });
        }

        WsInMessage::EnvVarResponse { id, value, .. } => {
            state.env_state.resolve(id, value);
        }

        WsInMessage::RequestState => {
            state.bex.request_playground_state();
        }
    }
}

// ---------------------------------------------------------------------------
// CORS middleware
// ---------------------------------------------------------------------------

async fn cors_middleware(req: Request<Body>, next: Next) -> Response {
    if req.method() == Method::OPTIONS {
        return Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, POST, OPTIONS")
            .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
            .body(Body::empty())
            .unwrap();
    }
    let mut resp = next.run(req).await;
    resp.headers_mut()
        .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
    resp
}

// ---------------------------------------------------------------------------
// Dev proxy mode — reverse-proxy to a local Vite dev server
// ---------------------------------------------------------------------------

fn dev_proxy_router(upstream: String) -> Router {
    Router::new().fallback(move |req: Request<Body>| {
        let upstream = upstream.clone();
        async move { proxy_request(upstream, req).await }
    })
}

async fn proxy_request(upstream: String, req: Request<Body>) -> Response {
    use axum::body::to_bytes;

    let is_ws_upgrade = req
        .headers()
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    if is_ws_upgrade {
        return proxy_ws(upstream, req).await;
    }

    let method = req.method().clone();
    let uri_path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let target_url = format!("{upstream}{uri_path_and_query}");

    let mut fwd = reqwest::Client::new().request(method, &target_url);
    for (name, value) in req.headers() {
        if name == header::HOST {
            continue;
        }
        fwd = fwd.header(name.clone(), value.clone());
    }

    let body_bytes = match to_bytes(req.into_body(), 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("Dev proxy: failed to read request body: {e}");
            return Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Body::from("proxy error"))
                .unwrap();
        }
    };
    if !body_bytes.is_empty() {
        fwd = fwd.body(body_bytes);
    }

    let upstream_resp = match fwd.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Dev proxy: upstream error: {e}");
            return Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Body::from(format!("upstream error: {e}")))
                .unwrap();
        }
    };

    let mut builder = Response::builder().status(upstream_resp.status());
    for (name, value) in upstream_resp.headers() {
        builder = builder.header(name.clone(), value.clone());
    }

    let resp_bytes = upstream_resp.bytes().await.unwrap_or_default();
    builder.body(Body::from(resp_bytes)).unwrap()
}

/// Proxy a WebSocket upgrade request (e.g. Vite HMR) to the upstream dev server.
async fn proxy_ws(upstream: String, req: Request<Body>) -> Response {
    let uri_path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let ws_url = format!(
        "ws://{}",
        upstream
            .strip_prefix("http://")
            .unwrap_or(upstream.strip_prefix("https://").unwrap_or(&upstream))
    ) + uri_path_and_query;

    let (mut parts, _body) = req.into_parts();
    let ws_upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(upgrade) => upgrade,
        Err(e) => {
            tracing::warn!("Dev proxy: WS upgrade extraction failed: {e}");
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from("ws upgrade error"))
                .unwrap();
        }
    };

    ws_upgrade.on_upgrade(move |client_socket| async move {
        let upstream_ws = match tokio_tungstenite::connect_async(&ws_url).await {
            Ok((stream, _)) => stream,
            Err(e) => {
                tracing::warn!("Dev proxy: failed to connect to upstream WS {ws_url}: {e}");
                return;
            }
        };

        let (mut client_sink, mut client_stream) = client_socket.split();
        let (mut upstream_sink, mut upstream_stream) = upstream_ws.split();

        use tokio_tungstenite::tungstenite::Message as TungMsg;

        let client_to_upstream = async {
            while let Some(Ok(msg)) = client_stream.next().await {
                let tung_msg = match msg {
                    AxumWsMsg::Text(t) => TungMsg::Text(t.to_string().into()),
                    AxumWsMsg::Binary(b) => TungMsg::Binary(b.to_vec().into()),
                    AxumWsMsg::Ping(p) => TungMsg::Ping(p.to_vec().into()),
                    AxumWsMsg::Pong(p) => TungMsg::Pong(p.to_vec().into()),
                    AxumWsMsg::Close(_) => {
                        let _ = upstream_sink.send(TungMsg::Close(None)).await;
                        break;
                    }
                };
                if upstream_sink.send(tung_msg).await.is_err() {
                    break;
                }
            }
        };

        let upstream_to_client = async {
            while let Some(Ok(msg)) = upstream_stream.next().await {
                let axum_msg = match msg {
                    TungMsg::Text(t) => AxumWsMsg::Text(t.to_string().into()),
                    TungMsg::Binary(b) => AxumWsMsg::Binary(b.to_vec().into()),
                    TungMsg::Ping(p) => AxumWsMsg::Ping(p.to_vec().into()),
                    TungMsg::Pong(p) => AxumWsMsg::Pong(p.to_vec().into()),
                    TungMsg::Close(_) => {
                        let _ = client_sink.send(AxumWsMsg::Close(None)).await;
                        break;
                    }
                    _ => continue,
                };
                if client_sink.send(axum_msg).await.is_err() {
                    break;
                }
            }
        };

        tokio::select! {
            _ = client_to_upstream => {}
            _ = upstream_to_client => {}
        }
    })
}

// ---------------------------------------------------------------------------
// Prod static-file mode
// ---------------------------------------------------------------------------

fn static_router(dir: String) -> Router {
    use tower_http::services::{ServeDir, ServeFile};
    let index = format!("{dir}/index.html");
    Router::new().fallback_service(ServeDir::new(&dir).not_found_service(ServeFile::new(index)))
}
