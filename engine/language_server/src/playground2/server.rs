use playground_server::{
    PlaygroundServer, AppState, LangServerToWasmMessage,
};
use crate::{playground::FrontendMessage, session::PreSendToWasmMessage};

// Type alias removed - use LangServerToWasmMessage directly

#[derive(Debug)]
pub struct Playground2Server {
    pub app_state: AppState,
}

impl Playground2Server {
    pub async fn run(self, listener: tokio::net::TcpListener) -> Result<(), Box<dyn std::error::Error + Send>> {
        let server = PlaygroundServer {
            app_state: self.app_state,
        };
        
        server.run(listener).await
    }
}