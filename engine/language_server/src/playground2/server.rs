use axum::{
    response::Html,
    routing::get,
    Router,
};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use crate::playground2::ping_handler::ping_handler;

#[derive(Debug, Clone)]
pub struct Playground2Server {
    port: u16,
}

impl Playground2Server {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let app = Router::new()
            .route("/", get(handler))
            .route("/health", get(health_check))
            .route("/ping", get(ping_handler));

        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        
        tracing::info!("Starting Playground2 server on {}", addr);
        
        let listener = TcpListener::bind(addr).await?;
        
        axum::serve(listener, app).await?;
        
        Ok(())
    }
}

async fn handler() -> Html<&'static str> {
    Html("<h1>Playground2 Server</h1><p>Axum HTTP server is running!</p>")
}

async fn health_check() -> &'static str {
    "OK"
}