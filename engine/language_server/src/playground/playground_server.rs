use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;

/// Script that runs the playground server.
/// On the input port
use crate::playground::definitions::PlaygroundState;
use crate::{playground::playground_server_helpers::create_server_routes, session::Session};

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
        let routes = create_server_routes(self.state, self.session);

        warp::serve(routes).try_bind(([127, 0, 0, 1], port)).await;

        Ok(())
    }
}
