use std::{io::Cursor, path::PathBuf, time::Duration};

use anyhow::Context;
use axum::{
    routing::{get, post},
    Router,
};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::Archive;
use tokio::{net::TcpListener, sync::broadcast};
use tower_http::services::ServeDir;

use crate::definitions::{WebviewNotification, WebviewRouterMessage};

#[derive(Debug)]
pub struct AppState {
    pub webview_router_to_websocket_rx: broadcast::Receiver<WebviewNotification>,
    pub to_webview_router_tx: broadcast::Sender<WebviewRouterMessage>,
    pub playground_port: u16,
    pub proxy_port: u16,
    pub editor_config: crate::config::SharedConfig,
    pub file_access: crate::fs::WorkspaceFileAccess,
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            webview_router_to_websocket_rx: self.webview_router_to_websocket_rx.resubscribe(),
            to_webview_router_tx: self.to_webview_router_tx.clone(),
            playground_port: self.playground_port,
            proxy_port: self.proxy_port,
            editor_config: self.editor_config.clone(),
            file_access: self.file_access.clone(),
        }
    }
}

pub struct PlaygroundServer {
    pub app_state: AppState,
}

impl PlaygroundServer {
    pub async fn run(self, listener: TcpListener) -> Result<(), Box<dyn std::error::Error + Send>> {
        let dist_dir = playground_static_assets().await?;

        let app = Router::new()
            .route("/ping", get(crate::handlers::ping_handler))
            .route("/ws", get(crate::handlers::ws_handler))
            // commands proxied to the IDE
            .route(
                "/webview/{command}",
                post(crate::handlers::webview_rpc_handler),
            )
            // proxied commands from the IDE to the webview
            .fallback_service(ServeDir::new(dist_dir))
            .with_state(self.app_state);

        tracing::info!(
            "Starting Playground server on {}",
            listener.local_addr().unwrap()
        );
        axum::serve(listener, app)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)
    }
}

async fn playground_static_assets() -> anyhow::Result<PathBuf> {
    const GITHUB_REPO: &str = "BoundaryML/baml";

    if std::env::var("VSCODE_DEBUG_MODE")
        .is_ok_and(|v| v.to_lowercase() == "true" || v.to_lowercase() == "1")
    {
        // Use cargo-relative path for local dist
        let local_dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../typescript/apps/playground/dist");
        eprintln!(
            "VSCODE_DEBUG_MODE is set. Using local playground dist at {}",
            local_dist.display()
        );
        Ok(local_dist)
    } else {
        let version = env!("CARGO_PKG_VERSION");

        eprintln!("Loading playground dist for version: {}", version);

        match get_playground_dist(GITHUB_REPO, version).await {
            Ok(dir) => Ok(std::path::PathBuf::from(dir)),
            Err(e) => {
                tracing::error!(
                    "Failed to prepare playground web UI: {e}. Serving error page instead."
                );
                Err(e)
            }
        }
    }
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

    // Fetch release metadata with timeout
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to create HTTP client: {e}"))?;

    tracing::info!("Sending request to GitHub API...");
    let resp = client
        .get(&api_url)
        .header("User-Agent", "baml-playground-server")
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch release metadata: {e}"))?;

    tracing::info!("Received response with status: {}", resp.status());

    // Check if the response is successful
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read response body".to_string());
        return Err(anyhow::anyhow!(
            "GitHub API request failed with status {}: {}",
            status,
            body
        ));
    }

    tracing::info!("Parsing JSON response...");
    let release: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse JSON response: {e}"))?;

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

    tracing::info!(
        "Found assets - main: {}, checksum: {}",
        download_url,
        checksum_url
    );

    // Compute extraction directory using the provided version
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    let extract_root: PathBuf = home
        .join(".baml/playground")
        .join(format!("web-panel-dist-{version}"));
    let dist_dir = extract_root.join("dist");

    // If already extracted, return the correct directory
    if dist_dir.exists() && dist_dir.read_dir()?.next().is_some() {
        tracing::info!("Web panel already extracted at: {}", dist_dir.display());
        return Ok(dist_dir.to_string_lossy().to_string());
    } else if extract_root.exists() {
        std::fs::remove_dir_all(&extract_root).with_context(|| {
            format!(
                "Failed to remove old extraction directory: {}",
                extract_root.display()
            )
        })?;
    }
    std::fs::create_dir_all(&extract_root).with_context(|| {
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

async fn verify_sha256_checksum(
    file_bytes: &[u8],
    checksum_url: &str,
    client: &reqwest::Client,
) -> anyhow::Result<()> {
    tracing::info!("Downloading SHA256 checksum from: {}", checksum_url);

    // Download the checksum file
    tracing::info!("Downloading checksum file...");
    let checksum_resp = client
        .get(checksum_url)
        .header("User-Agent", "baml-playground-server")
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to download checksum file: {e}"))?;

    if !checksum_resp.status().is_success() {
        return Err(anyhow::anyhow!(
            "Checksum download failed with status: {}",
            checksum_resp.status()
        ));
    }

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
