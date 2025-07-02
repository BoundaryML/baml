use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;

/// Script that runs the playground server.
/// On the input port
use crate::playground::definitions::PlaygroundState;
use crate::playground::proxy::ProxyServer;
use crate::{playground::playground_server_helpers::create_server_routes, session::Session};
use crate::playground::playground_server_helpers::ensure_web_panel_dist;

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
        let dist_dir = ensure_web_panel_dist(Some("test-zed")).await?;

        tracing::info!("Hosting playground frontend at: {}", dist_dir);

        let routes = create_server_routes(self.state, self.session, dist_dir);

        warp::serve(routes).try_bind(([127, 0, 0, 1], port)).await;

        Ok(())
    }
}
