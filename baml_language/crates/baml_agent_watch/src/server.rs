//! HTTP and WebSocket endpoints, on loopback only.
//!
//! - `GET /api/agent-activity/ws`: the live stream (see [`crate::protocol`]).
//! - `GET /api/agent-activity/state`: the current `hello`, for debugging.
//! - `GET /api/agent-activity/media/{id}`: an image a change refers to.
//! - `POST /api/agent-activity/runs`: another tool reports a run it started
//!   or finished (a `RunInfo`; post again with the same `id` to update it).
//! - everything else: the web bundle, whose `/agent-activity` page is the UI.

use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    body::Bytes,
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use baml_lsp_server::playground_server::{is_allowed_api_origin, web_bundle_router};
use tokio::{net::TcpListener, sync::broadcast::error::RecvError};

use crate::{protocol::RunInfo, session::Session};

pub(crate) async fn serve(
    listener: TcpListener,
    session: Arc<Session>,
    bundle_dir: Option<PathBuf>,
) -> anyhow::Result<()> {
    let api = Router::new()
        .route("/api/agent-activity/ws", get(ws_handler))
        .route("/api/agent-activity/state", get(state_handler))
        .route("/api/agent-activity/media/{id}", get(media_handler))
        .route("/api/agent-activity/runs", post(runs_handler))
        .with_state(session);
    let app = Router::new()
        .route(
            "/",
            get(|| async { Redirect::temporary("/agent-activity") }),
        )
        .merge(api)
        .fallback_service(web_bundle_router(bundle_dir)?);
    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("agent watch server error: {e}"))
}

fn forbidden(headers: &HeaderMap) -> Option<Response> {
    (!is_allowed_api_origin(headers.get(header::ORIGIN)))
        .then(|| (StatusCode::FORBIDDEN, "Forbidden origin").into_response())
}

async fn ws_handler(
    State(session): State<Arc<Session>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if let Some(denied) = forbidden(&headers) {
        return denied;
    }
    ws.on_upgrade(move |socket| stream(socket, session))
}

async fn stream(mut socket: WebSocket, session: Arc<Session>) {
    let (hello, mut rx) = session.subscribe_with_hello();
    if socket.send(Message::Text(hello.into())).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            next = rx.recv() => match next {
                Ok(text) => {
                    if socket.send(Message::Text(text.to_string().into())).await.is_err() {
                        return;
                    }
                }
                // Fell behind the broadcast buffer: start over with a fresh hello.
                Err(RecvError::Lagged(_)) => {
                    let (hello, fresh) = session.subscribe_with_hello();
                    rx = fresh;
                    if socket.send(Message::Text(hello.into())).await.is_err() {
                        return;
                    }
                }
                Err(RecvError::Closed) => return,
            },
            incoming = socket.recv() => match incoming {
                None | Some(Err(_) | Ok(Message::Close(_))) => return,
                Some(Ok(_)) => {}
            },
        }
    }
}

async fn state_handler(State(session): State<Arc<Session>>, headers: HeaderMap) -> Response {
    if let Some(denied) = forbidden(&headers) {
        return denied;
    }
    let (hello, _) = session.subscribe_with_hello();
    ([(header::CONTENT_TYPE, "application/json")], hello).into_response()
}

async fn media_handler(
    State(session): State<Arc<Session>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Some(denied) = forbidden(&headers) {
        return denied;
    }
    let Some((mime, bytes)) = session.media(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    (
        [
            (header::CONTENT_TYPE, mime),
            // The id is the content's hash, so a response never goes stale.
            (
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable",
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes.to_vec(),
    )
        .into_response()
}

async fn runs_handler(
    State(session): State<Arc<Session>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(denied) = forbidden(&headers) {
        return denied;
    }
    match serde_json::from_slice::<RunInfo>(&body) {
        Ok(run) => {
            session.upsert_run(run);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => (StatusCode::BAD_REQUEST, format!("invalid run: {err}")).into_response(),
    }
}
