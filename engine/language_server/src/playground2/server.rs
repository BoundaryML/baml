use playground_server::{
    GitHubReleaseAssetManager, PlaygroundServer, AppState, LangServerToWasmMessage,
    AssetManager,
};
use crate::{playground::FrontendMessage, session::PreSendToWasmMessage};

// Type alias for language server specific message type
pub type LanguageServerMessage = LangServerToWasmMessage<lsp_server::Message>;

#[derive(Debug)]
pub struct Playground2Server {
    pub app_state: AppState<LanguageServerMessage>,
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