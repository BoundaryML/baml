use playground_server::{PlaygroundServer, AppState};

/// Simple helper function to create and run a PlaygroundServer
/// Replaces the duplicate Playground2Server struct
pub async fn run_playground_server(
    app_state: AppState, 
    listener: tokio::net::TcpListener
) -> Result<(), Box<dyn std::error::Error + Send>> {
    let server = PlaygroundServer { app_state };
    server.run(listener).await
}