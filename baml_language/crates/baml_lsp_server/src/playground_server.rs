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
use bex_project::{is_cancelled_engine_error, is_cancelled_runtime_error};
use futures::{SinkExt, stream::StreamExt};
use prost::Message;
use tokio::{net::TcpListener, sync::broadcast};

use crate::{
    playground_env::PlaygroundEnvState,
    playground_io::PlaygroundIoState,
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
    io_state: Arc<PlaygroundIoState>,
}

/// Start the playground server on the given listener.
pub async fn run(
    listener: TcpListener,
    bex: Arc<dyn bex_project::BexLsp>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    env_state: Arc<PlaygroundEnvState>,
    io_state: Arc<PlaygroundIoState>,
) -> anyhow::Result<()> {
    let app = build_router(bex, broadcast_tx, env_state, io_state)?;

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
    io_state: Arc<PlaygroundIoState>,
) -> anyhow::Result<Router> {
    let ws_state = WsState {
        bex,
        broadcast_tx,
        env_state,
        io_state,
    };

    let api = Router::new()
        .route("/api/ws", get(playground_ws_handler))
        .with_state(ws_state);

    let app = if let Ok(dev_port) = std::env::var("BAML_PLAYGROUND_DEV_PORT") {
        let dev_port: u16 = dev_port
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid BAML_PLAYGROUND_DEV_PORT: {e}"))?;
        tracing::info!("Playground: dev proxy -> http://localhost:{dev_port}");
        api.fallback_service(dev_proxy_router(format!("http://localhost:{dev_port}")))
    } else if let Ok(dir) = std::env::var("BAML_PLAYGROUND_DIR") {
        tracing::info!("Playground: serving static files from {dir}");
        api.fallback_service(static_router(dir))
    } else {
        tracing::info!(
            "Playground: no BAML_PLAYGROUND_DIR or BAML_PLAYGROUND_DEV_PORT set; serving /api/ws only"
        );
        api
    };

    Ok(app.layer(middleware::from_fn(cors_middleware)))
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

    // Send all process env vars so the UI can display them immediately.
    {
        let vars: std::collections::HashMap<String, String> = std::env::vars().collect();
        if let Some(msg) = to_ws_text(&WsOutMessage::ProcessEnvVars { vars })
            && sink.send(msg).await.is_err()
        {
            return;
        }
    }

    // Send env var names referenced in BAML source code.
    {
        let names = state.bex.all_env_var_names();
        if let Some(msg) = to_ws_text(&WsOutMessage::KnownEnvVarNames { names })
            && sink.send(msg).await.is_err()
        {
            return;
        }
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
                        cancelled: None,
                    };
                    if let Some(ws_msg) = to_ws_text(&err_msg) {
                        let _ = sink.send(ws_msg).await;
                    }
                    return;
                }
            };

            let args = match bridge_ctypes::baml_core::cffi::CallFunctionArgs::decode(
                decoded.as_slice(),
            ) {
                Ok(a) => a,
                Err(e) => {
                    let err_msg = WsOutMessage::CallFunctionError {
                        id,
                        error: format!("Failed to decode arguments: {e}"),
                        cancelled: None,
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
                        cancelled: None,
                    };
                    if let Some(ws_msg) = to_ws_text(&err_msg) {
                        let _ = sink.send(ws_msg).await;
                    }
                    return;
                }
            };

            let broadcast_tx = state.broadcast_tx.clone();
            let call_id = sys_types::CallId(id);
            let fs_path = bex_project::FsPath::from_str(project.clone());

            let function_call_ctx = bex_project::FunctionCallContextBuilder::new(call_id);

            let bex = match state.bex.get_bex_for_project(&fs_path).map_err(|e| {
                WsOutMessage::CallFunctionError {
                    id,
                    error: format!("Failed to get Bex for project: {e}"),
                    cancelled: None,
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

            let echo_msg = WsOutMessage::CallFunction {
                id,
                project,
                name: name.clone(),
                args_proto,
            };

            tokio::spawn(async move {
                let handle_options = bridge_ctypes::CffiHandleTableOptions::for_wire();
                let _ = broadcast_tx.send(echo_msg);
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
                                cancelled: None,
                            },
                        }
                    }
                    Err(e) => {
                        let is_cancelled = is_cancelled_runtime_error(&e);
                        WsOutMessage::CallFunctionError {
                            id,
                            error: format!("{e}"),
                            cancelled: if is_cancelled { Some(true) } else { None },
                        }
                    }
                };
                let _ = broadcast_tx.send(out);
            });
        }

        WsInMessage::CancelCall { id, project } => {
            let fs_path = bex_project::FsPath::from_str(project);
            match state.bex.get_bex_for_project(&fs_path) {
                Ok(bex) => {
                    if let Err(e) = bex.cancel_function_call(sys_types::CallId(id)) {
                        tracing::warn!("cancel_function_call failed: {e}");
                    }
                }
                Err(e) => {
                    tracing::warn!("CancelCall: no bex for project: {e}");
                }
            }
        }

        WsInMessage::CallTestFunction {
            id,
            project,
            generation,
            test_name,
        } => {
            let call_id = sys_types::CallId(id);
            let ctx = bex_project::FunctionCallContextBuilder::new(call_id).build();
            let broadcast_tx = state.broadcast_tx.clone();
            let bex = state.bex.clone();

            tokio::spawn(async move {
                let out = match bex
                    .call_test_function(&project, generation, &test_name, ctx)
                    .await
                {
                    Ok(result) => {
                        let handle_options = bridge_ctypes::CffiHandleTableOptions::for_wire();
                        match bridge_ctypes::external_to_baml_value(&result, &handle_options) {
                            Ok(baml_val) => {
                                let b64 = base64::engine::general_purpose::STANDARD
                                    .encode(baml_val.encode_to_vec());
                                WsOutMessage::CallFunctionResult { id, result: b64 }
                            }
                            Err(e) => WsOutMessage::CallFunctionError {
                                id,
                                error: format!("Failed to encode result: {e}"),
                                cancelled: None,
                            },
                        }
                    }
                    Err(e) => {
                        let is_cancelled = is_cancelled_engine_error(&e);
                        WsOutMessage::CallFunctionError {
                            id,
                            error: format!("{e}"),
                            cancelled: if is_cancelled { Some(true) } else { None },
                        }
                    }
                };
                let _ = broadcast_tx.send(out);
            });
        }

        WsInMessage::ExpandTestSet {
            project,
            generation,
            testset_name,
        } => {
            state
                .bex
                .expand_test_set(&project, generation, &testset_name);
        }

        WsInMessage::EnvVarResponse { id, value, .. } => {
            state.env_state.resolve(id, value);
        }

        WsInMessage::InputResponse { id, value, call_id } => {
            state.io_state.resolve(id, call_id, value);
        }

        WsInMessage::RequestState => {
            state.bex.request_playground_state();
        }

        WsInMessage::RequestCollectTests { project } => {
            state.bex.request_collect_tests(&project);
        }

        WsInMessage::RequestControlFlowGraph {
            project: _,
            function_name,
        } => {
            let graph = state.bex.ast_control_flow_graph(&function_name);
            let graph = graph.map(|g| {
                baml_compiler2_visualization::control_flow::prepare_control_flow_graph_for_visualization(&g)
            });
            let graph_json = graph.as_ref().and_then(|g| serde_json::to_value(g).ok());
            let msg = WsOutMessage::ControlFlowGraphResult {
                function_name,
                graph: graph_json,
            };
            if let Some(ws_msg) = to_ws_text(&msg)
                && sink.send(ws_msg).await.is_err()
            {
                tracing::warn!("Failed to send control flow graph result");
            }
        }

        WsInMessage::CursorPosition { file, line, column } => {
            let ctx = state.bex.playground_cursor_context(&file, line, column);
            let ctx_json = serde_json::to_value(&ctx).unwrap_or(serde_json::Value::Null);
            let msg = WsOutMessage::CursorContext { context: ctx_json };
            if let Some(ws_msg) = to_ws_text(&msg)
                && sink.send(ws_msg).await.is_err()
            {
                tracing::warn!("Failed to send cursor context");
            }
        }

        WsInMessage::SetEnvVar { key, value } => {
            state.env_state.set_override(key, value);
        }

        WsInMessage::DeleteEnvVar { key } => {
            state.env_state.remove_override(&key);
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
        .map(axum::http::uri::PathAndQuery::as_str)
        .unwrap_or("/");
    let target_url = format!("{upstream}{uri_path_and_query}");

    ensure_rustls_crypto_provider();
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

#[cfg(feature = "ring-crypto")]
fn ensure_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(all(not(feature = "ring-crypto"), feature = "aws-crypto"))]
fn ensure_rustls_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[cfg(all(not(feature = "ring-crypto"), not(feature = "aws-crypto")))]
fn ensure_rustls_crypto_provider() {}

/// Proxy a WebSocket upgrade request (e.g. Vite HMR) to the upstream dev server.
async fn proxy_ws(upstream: String, req: Request<Body>) -> Response {
    let uri_path_and_query = req
        .uri()
        .path_and_query()
        .map(axum::http::uri::PathAndQuery::as_str)
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bex_project::{BamlVFS, FsPath, InMemoryFs, new_lsp_with_initial_project};
    use tokio_tungstenite::tungstenite;

    use super::*;
    use crate::{
        no_op_lsp_sender::NoOpLspSender,
        playground_sender::NativePlaygroundSender,
        playground_setup::PlaygroundWiring,
    };

    /// When neither `BAML_PLAYGROUND_DIR` nor `BAML_PLAYGROUND_DEV_PORT` is set,
    /// `build_router` used to `bail!` and kill the server task. Now it falls
    /// through to `/api/ws`-only mode so bridge_cffi hosts that don't ship
    /// playground assets still get a working WS endpoint.
    #[tokio::test(flavor = "multi_thread")]
    async fn api_ws_only_fallback_when_no_env_vars() {
        // SAFETY: env vars are process-global. baml_lsp_server has no other
        // tests today, so this won't race with anything.
        unsafe {
            std::env::remove_var("BAML_PLAYGROUND_DIR");
            std::env::remove_var("BAML_PLAYGROUND_DEV_PORT");
        }

        let wiring = PlaygroundWiring::build();

        let sources: HashMap<FsPath, String> = HashMap::from([(
            FsPath::from_str("/baml_src/main.baml".to_string()),
            String::new(),
        )]);
        let mut files: HashMap<String, Vec<u8>> = HashMap::new();
        files.insert("/baml_src/main.baml".to_string(), Vec::new());
        let vfs = BamlVFS::new(std::sync::Arc::new(Box::new(InMemoryFs::new(files))));
        let root = vfs::VfsPath::from(vfs.clone()).join("baml_src").unwrap();

        let lsp_sender: std::sync::Arc<dyn bex_project::LspClientSenderTrait + Send + Sync> =
            std::sync::Arc::new(NoOpLspSender);
        let playground_sender = std::sync::Arc::new(NativePlaygroundSender::new(
            wiring.broadcast_tx.clone(),
            lsp_sender.clone(),
            0,
            false,
        ));

        let bex = new_lsp_with_initial_project(
            wiring.sys_op_factory.clone(),
            lsp_sender,
            playground_sender,
            vfs,
            Some(wiring.event_sink.clone()),
            bex_project::BackgroundSpawner::new(),
            root,
            sources,
        )
        .expect("project registration");
        let bex: Arc<dyn bex_project::BexLsp> = Arc::new(bex);

        // Bind an OS-assigned ephemeral port; pick_port returns its argument
        // verbatim, not the listener's actual local_addr.
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind ephemeral port");
        let port = listener.local_addr().expect("local_addr").port();
        let btx = wiring.broadcast_tx.clone();
        let es = wiring.env_state.clone();
        let ios = wiring.io_state.clone();
        tokio::spawn(async move {
            let _ = run(listener, bex, btx, es, ios).await;
        });

        // Give the server a moment to come up.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 1. WS upgrade on /api/ws succeeds.
        let ws_url = format!("ws://127.0.0.1:{port}/api/ws");
        let (mut stream, response) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .expect("WS handshake should succeed when no fallback env vars are set");
        assert_eq!(
            response.status(),
            tungstenite::http::StatusCode::SWITCHING_PROTOCOLS
        );
        // The handler immediately sends a Ready message after the upgrade.
        let first = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
            .await
            .expect("first WS message arrives within 2s")
            .expect("WS stream not closed")
            .expect("WS frame ok");
        match first {
            tungstenite::Message::Text(_) => { /* ready frame */ }
            other => panic!("expected text Ready frame, got {other:?}"),
        }
        drop(stream);

        // 2. Non-/api GET 404s cleanly (no fallback service mounted).
        let resp = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{port}/index.html"))
            .send()
            .await
            .expect("HTTP request completes");
        assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
    }
}
