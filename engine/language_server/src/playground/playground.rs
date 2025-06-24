/// Script that runs the playground server.
/// On the input port
use crate::playground::definitions::PlaygroundState;
use crate::playground::playground_server::create_server_routes;
use crate::session::Session;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

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
