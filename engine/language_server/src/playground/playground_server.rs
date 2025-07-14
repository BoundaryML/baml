use std::{env, sync::Arc};

use anyhow::Result;
use tokio::sync::RwLock;

/// Script that runs the playground server.
/// On the input port
use crate::playground::definitions::PlaygroundState;
use crate::{
    playground::playground_server_helpers::{create_server_routes, get_playground_dist},
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

        tracing::info!("Hosted playground at http://localhost:{}...", port);

        let routes = create_server_routes(self.state, self.session, dist_dir);

        warp::serve(routes).try_bind(([127, 0, 0, 1], port)).await;

        Ok(())
    }
}
