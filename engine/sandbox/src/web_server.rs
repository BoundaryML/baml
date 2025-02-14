use anyhow::{Context, Result};
use axum::{routing::get, Router};
use std::rc::Rc;

fn main() {
    let msg = Rc::new("my name is cabbage".to_string());

    println!("starting server");
    let localset = tokio::task::LocalSet::new();
    localset.spawn_local(async move {
        if let Err(e) = run().await {
            tracing::error!("server error: {}", e);
        }
    });

    println!("shutting down server");
}

async fn run() -> Result<()> {
    let port = 4000;
    let tcp_listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .context(format!(
            "Failed to bind to port {}; try using --port PORT to specify a different port.",
            port
        ))?;

    let app = Router::new().route("/", get(handler));

    axum::serve(tcp_listener, app)
        .await
        .context("Failed to start server")?;

    Ok(())
}
// Basic handler that returns a greeting
async fn handler() -> &'static str {
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    "Hello from sandbox server!"
}
