use std::{
    collections::HashMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use dirs::home_dir;
use flate2::read::GzDecoder;
use futures_util::{SinkExt, StreamExt};
use include_dir::{include_dir, Dir};
use mime_guess::from_path;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tar::Archive;
use tokio::{fs as async_fs, sync::RwLock};
use warp::{http::Response, ws::Message, Filter};

use crate::{
    playground::definitions::{FrontendMessage, PlaygroundState},
    session::Session,
};

// Embed at compile time everything in dist/
// WARNING: this is a relative path, will easily break if file structure changes
// WARNING: works as a macro so any build script executes after this is evaluated
// static STATIC_DIR: Dir<'_> =
//     include_dir!("$CARGO_MANIFEST_DIR/../../typescript/vscode-ext/packages/web-panel/dist");

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

    // --- SEND BUFFERED EVENTS (if any) ---
    {
        let mut st = state.write().await;
        let buffered_events = st.drain_event_buffer();
        for event in buffered_events.clone() {
            let _ = ws_tx.send(Message::text(event)).await;
        }
        tracing::info!("Sent {} buffered events", buffered_events.len());
        st.mark_first_client_connected();
    }
    // --- END BUFFERED EVENTS ---

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
/// If dist_dir is None, serves an error page indicating playground is unavailable.
pub fn create_server_routes(
    state: Arc<RwLock<PlaygroundState>>,
    session: Arc<Session>,
    dist_dir: Option<std::path::PathBuf>,
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

    // Static file serving - either real files or error page
    let spa = warp::path::full()
        .and(warp::get())
        .and_then(move |full: warp::path::FullPath| {
            let dist_dir = dist_dir.clone();
            async move {
                match dist_dir {
                    Some(dir) => {
                        // Normal file serving
                        let path = full.as_str().trim_start_matches('/');
                        let file = if path.is_empty() { "index.html" } else { path };
                        let file_path = dir.join(file);
                        match tokio::fs::read(&file_path).await {
                            Ok(body) => {
                                let mime = from_path(file).first_or_octet_stream();
                                Ok::<_, warp::Rejection>(
                                    Response::builder()
                                        .header("content-type", mime.as_ref())
                                        .body(body)
                                        .unwrap(),
                                )
                            }
                            Err(_) => {
                                // File not found, serve error page
                                Ok::<_, warp::Rejection>(serve_error_page())
                            }
                        }
                    }
                    None => {
                        // No dist directory available, serve error page
                        Ok::<_, warp::Rejection>(serve_error_page())
                    }
                }
            }
        });

    ws_route.or(spa).with(warp::log("playground-server"))
}

/// Creates a nice HTML error page when playground assets are not available
fn serve_error_page() -> Response<Vec<u8>> {
    let error_html = include_str!("error_page.html");

    Response::builder()
        .header("content-type", "text/html; charset=utf-8")
        .status(503) // Service Unavailable
        .body(error_html.as_bytes().to_vec())
        .unwrap()
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
    let mut st = state.write().await;
    if !st.first_client_connected {
        st.buffer_event(msg_str);
    } else if let Err(e) = st.broadcast_update(msg_str) {
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

    // broadcast to all connected clients
    let select_function_msg = FrontendMessage::select_function {
        root_path: root_path.to_string(),
        function_name,
    };

    let msg_str = serde_json::to_string(&select_function_msg)?;
    let mut st = state.write().await;
    if !st.first_client_connected {
        st.buffer_event(msg_str);
    } else if let Err(e) = st.broadcast_update(msg_str) {
        tracing::error!("Failed to broadcast function change: {}", e);
    }
    Ok(())
}

// Helper function to broadcast test runs
pub async fn broadcast_test_run(
    state: &Arc<RwLock<PlaygroundState>>,
    test_name: String,
) -> Result<()> {
    tracing::debug!("Broadcasting test run for: {}", test_name);

    // broadcast to all connected clients
    let run_test_msg = FrontendMessage::run_test { test_name };

    let msg_str = serde_json::to_string(&run_test_msg)?;
    let mut st = state.write().await;
    if !st.first_client_connected {
        st.buffer_event(msg_str);
    } else if let Err(e) = st.broadcast_update(msg_str) {
        tracing::error!("Failed to broadcast test run: {}", e);
    }
    Ok(())
}

/// Verifies the SHA256 checksum of a downloaded file against the expected checksum
async fn verify_sha256_checksum(
    file_bytes: &[u8],
    checksum_url: &str,
    client: &Client,
) -> anyhow::Result<()> {
    tracing::info!("Downloading SHA256 checksum from: {}", checksum_url);

    // Download the checksum file
    let checksum_resp = client
        .get(checksum_url)
        .header("User-Agent", "baml-playground-server")
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to download checksum file: {e}"))?;

    let checksum_text = checksum_resp.text().await?;

    // Parse the expected checksum (format: "hash filename" or just "hash")
    let expected_checksum = checksum_text
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Invalid checksum file format"))?
        .to_lowercase();

    // Calculate actual checksum
    let mut hasher = Sha256::new();
    hasher.update(file_bytes);
    let actual_checksum = format!("{:x}", hasher.finalize());

    // Verify checksums match
    if actual_checksum != expected_checksum {
        return Err(anyhow::anyhow!(
            "SHA256 checksum verification failed. Expected: {}, Actual: {}",
            expected_checksum,
            actual_checksum
        ));
    }

    tracing::info!("SHA256 checksum verification passed");
    Ok(())
}

/// Downloads and extracts the playground frontend from the baml GitHub release.
/// Uses the provided version for both asset name construction and release tag lookup.
/// Returns the path to the directory containing the static files to serve (may be a nested 'dist' directory).
pub async fn get_playground_dist(github_repo: &str, version: &str) -> anyhow::Result<String> {
    // Construct versioned asset names
    let web_panel_asset_name = format!("playground-dist-{version}.tar.gz");
    let checksum_asset_name = format!("playground-dist-{version}.tar.gz.sha256");

    // Build the GitHub API URL using the version as the release tag
    let api_url = format!("https://api.github.com/repos/{github_repo}/releases/tags/{version}");
    tracing::info!("Fetching web-panel release metadata from: {}", api_url);

    // Fetch release metadata
    let client = Client::new();
    let resp = client
        .get(&api_url)
        .header("User-Agent", "baml-playground-server")
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch release metadata: {e}"))?;
    let release: serde_json::Value = resp.json().await?;

    // Find the main asset
    let assets = release["assets"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No assets in release metadata"))?;
    let asset = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(&web_panel_asset_name))
        .ok_or_else(|| anyhow::anyhow!("No asset named '{}' in release", web_panel_asset_name))?;
    let download_url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No download URL for asset"))?;

    // Find the checksum asset
    let checksum_asset = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(&checksum_asset_name))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No checksum asset named '{}' in release",
                checksum_asset_name
            )
        })?;
    let checksum_url = checksum_asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No download URL for checksum asset"))?;

    // Compute extraction directory using the provided version
    let home = home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    let extract_root: PathBuf = home
        .join(".baml/playground")
        .join(format!("web-panel-dist-{version}"));
    let dist_dir = extract_root.join("dist");

    // If already extracted, return the correct directory
    if dist_dir.exists() && dist_dir.read_dir()?.next().is_some() {
        tracing::info!("Web panel already extracted at: {}", dist_dir.display());
        return Ok(dist_dir.to_string_lossy().to_string());
    } else if extract_root.exists() {
        fs::remove_dir_all(&extract_root).with_context(|| {
            format!(
                "Failed to remove old extraction directory: {}",
                extract_root.display()
            )
        })?;
    }
    fs::create_dir_all(&extract_root).with_context(|| {
        format!(
            "Failed to create extraction directory: {}",
            extract_root.display()
        )
    })?;

    // Download the tar.gz asset
    tracing::info!("Downloading web-panel asset from: {}", download_url);
    let resp = client
        .get(download_url)
        .header("User-Agent", "baml-playground-server")
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to download asset: {e}"))?;
    let bytes = resp.bytes().await?;

    // Verify SHA256 checksum
    verify_sha256_checksum(&bytes, checksum_url, &client).await?;

    // Extract the verified archive
    let tar = GzDecoder::new(Cursor::new(bytes));
    let mut archive = Archive::new(tar);
    archive
        .unpack(&extract_root)
        .with_context(|| format!("Failed to extract archive to: {}", extract_root.display()))?;

    // Return the path to the actual dist directory if it exists, else the extraction root
    if dist_dir.exists() && dist_dir.read_dir()?.next().is_some() {
        Ok(dist_dir.to_string_lossy().to_string())
    } else {
        Ok(extract_root.to_string_lossy().to_string())
    }
}

/// Returns the expected extraction directory for a given version (not the nested dist directory)
pub fn web_panel_extract_root(version: &str) -> String {
    let home = home_dir().expect("Could not determine home directory");
    home.join(".baml/playground")
        .join(format!("web-panel-dist-{version}"))
        .to_string_lossy()
        .to_string()
}
