use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use include_dir::{include_dir, Dir};
use mime_guess::from_path;
use tokio::sync::RwLock;
use warp::{http::Response, ws::Message, Filter};

use crate::{
    playground::definitions::{FrontendMessage, PlaygroundState},
    session::Session,
};

/// Embed at compile time everything in dist/
// WARNING: this is a relative path, will easily break if file structure changes
static STATIC_DIR: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/../../typescript/vscode-ext/packages/web-panel/dist");

/// Helper to send all projects/files to a websocket client
pub async fn send_all_projects_to_client(
    ws_tx: &mut (impl SinkExt<Message> + Unpin),
    session: &Arc<Session>,
) {
    let projects = {
        let projects = session.baml_src_projects.lock().unwrap();
        projects
            .iter()
            .map(|(root_path, project)| {
                let project = project.lock().unwrap();
                let files = project.baml_project.files.clone();
                let root_path = root_path.to_string_lossy().to_string();
                let files_map: HashMap<String, String> = files
                    .into_iter()
                    .map(|(path, doc)| (path.path().to_string_lossy().to_string(), doc.contents))
                    .collect();
                (root_path, files_map)
            })
            .collect::<Vec<_>>()
    };
    for (root_path, files_map) in projects {
        let add_project_msg = FrontendMessage::add_project {
            root_path,
            files: files_map,
        };
        if let Ok(msg_str) = serde_json::to_string(&add_project_msg) {
            let _ = ws_tx.send(Message::text(msg_str)).await;
        }
    }
}

pub async fn start_client_connection(
    ws: warp::ws::WebSocket,
    state: Arc<RwLock<PlaygroundState>>,
    session: Arc<Session>,
) {
    let (mut ws_tx, mut ws_rx) = ws.split();
    let mut rx = {
        let state = state.read().await;
        state.tx.subscribe()
    };

    // Send initial project state using the helper
    send_all_projects_to_client(&mut ws_tx, &session).await;

    // Handle incoming messages and broadcast updates
    tokio::spawn(async move {
        loop {
            tokio::select! {
                // Handle incoming messages from the client
                Some(result) = ws_rx.next() => {
                    match result {
                        Ok(msg) => {
                            if msg.is_close() {
                                tracing::info!("Client disconnected");
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::error!("WebSocket error: {}", e);
                            break;
                        }
                    }
                }
                // Handle broadcast messages
                Ok(msg) = rx.recv() => {
                    if let Err(e) = ws_tx.send(Message::text(msg)).await {
                        tracing::error!("Failed to send broadcast message: {}", e);
                        break;
                    }
                }
                else => break,
            }
        }
    });
}

/// Adds a "/" route which servers the static files of the frontend
/// and a "/ws" route which handles the websocket connection.
pub fn create_server_routes(
    state: Arc<RwLock<PlaygroundState>>,
    session: Arc<Session>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    // WebSocket handler with error handling
    let ws_route = warp::path("ws")
        .and(warp::ws())
        .map(move |ws: warp::ws::Ws| {
            let state = state.clone();
            let session = session.clone();
            ws.on_upgrade(move |socket| async move {
                start_client_connection(socket, state, session).await;
            })
        });

    // Static file serving needed to serve the frontend files
    let spa =
        warp::path::full()
            .and(warp::get())
            .and_then(|full: warp::path::FullPath| async move {
                let path = full.as_str().trim_start_matches('/');
                let file = if path.is_empty() { "index.html" } else { path };
                match STATIC_DIR.get_file(file) {
                    Some(f) => {
                        let body = f.contents();
                        let mime = from_path(file).first_or_octet_stream();
                        Ok::<_, warp::Rejection>(
                            Response::builder()
                                .header("content-type", mime.as_ref())
                                .body(body.to_vec()),
                        )
                    }
                    None => Ok::<_, warp::Rejection>(
                        Response::builder().status(404).body(b"Not Found".to_vec()),
                    ),
                }
            });

    ws_route.or(spa).with(warp::log("playground-server"))
}

// Helper function to broadcast project updates with better error handling
pub async fn broadcast_project_update(
    state: &Arc<RwLock<PlaygroundState>>,
    root_path: &str,
    files: HashMap<String, String>,
) -> Result<()> {
    let add_project_msg = FrontendMessage::add_project {
        root_path: root_path.to_string(),
        files,
    };

    let msg_str = serde_json::to_string(&add_project_msg)?;
    if let Err(e) = state.read().await.broadcast_update(msg_str) {
        tracing::error!("Failed to broadcast project update: {}", e);
    }
    Ok(())
}

// Helper function to broadcast function changes
pub async fn broadcast_function_change(
    state: &Arc<RwLock<PlaygroundState>>,
    root_path: &str,
    function_name: String,
) -> Result<()> {
    tracing::debug!("Broadcasting function change for: {}", function_name);

    let select_function_msg = FrontendMessage::select_function {
        root_path: root_path.to_string(),
        function_name,
    };

    let msg_str = serde_json::to_string(&select_function_msg)?;
    if let Err(e) = state.read().await.broadcast_update(msg_str) {
        tracing::error!("Failed to broadcast function change: {}", e);
    }
    Ok(())
}
