use std::{env, sync::Arc};

use anyhow::Result;
use tokio::sync::RwLock;

/// Script that runs the playground server.
/// On the input port
use crate::playground::definitions::PlaygroundState;
use crate::{
    playground::playground_server_helpers::{create_server_routes, get_playground_dist},
    playground::proxy::ProxyServer,
    session::Session,
};

// Defines where the playground server will look for to fetch the frontend
const GITHUB_REPO: &str = "BoundaryML/baml";

#[derive(Debug, Clone)]
pub struct PlaygroundServer {
    state: Arc<RwLock<PlaygroundState>>,
    session: Arc<Session>,
}

impl PlaygroundServer {
    pub fn new(state: Arc<RwLock<PlaygroundState>>, session: Arc<Session>) -> Self {
        Self { state, session }
    }

    pub async fn run(self, port: u16) -> Result<()> {
        // Sets debug mode using the VSCODE_DEBUG_MODE enviroment variable.
        // Otherwise defaults to retrieving the playground from github releases
        let dist_dir = if env::var("VSCODE_DEBUG_MODE_DONT_USE_THIS")
            .map(|v| v == "true")
            .unwrap_or(false)
        {
            // Use cargo-relative path for local dist
            let local_dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../typescript/apps/playground/dist");
            tracing::info!(
                "VSCODE_DEBUG_MODE is set. Using local playground dist at {}",
                local_dist.display()
            );
            Some(local_dist)
        } else {
            let version = env!("CARGO_PKG_VERSION");
            // Test release
            // let version = "test-zed";

            match get_playground_dist(GITHUB_REPO, version).await {
                Ok(dir) => Some(std::path::PathBuf::from(dir)),
                Err(e) => {
                    tracing::error!(
                        "Failed to prepare playground web UI: {e}. Serving error page instead."
                    );
                    None
                }
            }
        };

        // TODO REMOVE FOR PRODUCTION

        // let local_dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        //     .join("../../typescript/apps/playground/dist");
        // tracing::info!(
        //     "VSCODE_DEBUG_MODE is set. Using local playground dist at {}",
        //     local_dist.display()
        // );
        // let dist_dir = Some(local_dist);

        let routes = create_server_routes(self.state, self.session, dist_dir);

        // Start the proxy server on a different port
        let proxy_port = port + 1; // Use playground port + 1 for proxy
        let proxy_server = ProxyServer::new(proxy_port);

        // Spawn the proxy server in a separate task
        let proxy_handle = tokio::spawn(async move {
            if let Err(e) = proxy_server.start().await {
                tracing::error!("Proxy server failed: {}", e);
            }
        });

        // Start the main playground server
        tracing::info!("Starting main playground server on port {}", port);
        tracing::info!("Starting proxy server on port {}", proxy_port);
        warp::serve(routes).try_bind(([127, 0, 0, 1], port)).await;

        // If we get here, the main server has stopped
        tracing::info!("Main playground server stopped");

        Ok(())
    }
}
