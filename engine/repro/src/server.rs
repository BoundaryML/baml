use playground_server::{
    GitHubReleaseAssetManager, PlaygroundServer, AppState, LangServerToWasmMessage,
};
use crate::definitions::{FrontendMessage, PreSendToWasmMessage};

// Type alias for repro specific message type  
pub type ReproMessage = LangServerToWasmMessage<lsp_server::Message>;

#[derive(Debug)]
pub struct Playground2Server {
    pub app_state: AppState<ReproMessage>,
}

impl Playground2Server {
    pub async fn run(self, listener: tokio::net::TcpListener) -> Result<(), Box<dyn std::error::Error + Send>> {
        let asset_manager = GitHubReleaseAssetManager {
            github_repo: "BoundaryML/baml",
            version_env_var: "CARGO_PKG_VERSION",
        };
        
        let server = PlaygroundServer {
            app_state: self.app_state,
            asset_manager,
        };
        
        server.run(listener).await
    }
}