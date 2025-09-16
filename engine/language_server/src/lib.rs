use std::num::NonZeroUsize;

use anyhow::Context;
pub use edit::{DocumentKey, PositionEncoding, TextDocument};
use playground_server::{WebviewNotification, WebviewRouterMessage};
pub use session::{ClientSettings, DocumentQuery, DocumentSnapshot, Session};
use tokio::sync::broadcast;

use crate::server::{Server, ServerArgs};

#[macro_use]
mod message;

pub mod cors_bypass_proxy;
pub mod edit;
pub mod logging;
pub mod server;
pub mod session;
#[cfg(test)]
mod tests;

// additional baml modules
mod baml_project;
mod baml_source_file;
mod baml_text_size;

pub(crate) const SERVER_NAME: &str = "baml-lsp";
pub(crate) const DIAGNOSTIC_NAME: &str = "BAML";

pub(crate) fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn run_server() -> anyhow::Result<()> {
    let tokio_runtime = tokio::runtime::Runtime::new()?;

    let (webview_router_to_websocket_tx, webview_router_to_websocket_rx) = broadcast::channel(1000);
    let (to_webview_router_tx, to_webview_router_rx) = broadcast::channel(1000);

    let port_config = playground_server::PortConfiguration {
        base_port: 3700,
        max_attempts: 100,
    };
    let port_picks = tokio_runtime.block_on(playground_server::pick_ports(port_config))?;

    {
        let webview_router_to_websocket_tx = webview_router_to_websocket_tx.clone();
        let to_webview_router_tx = to_webview_router_tx.clone();
        tokio_runtime.spawn(futures::future::join(
            async move {
                eprintln!("Playground server started");
                let server = playground_server::PlaygroundServer {
                    app_state: playground_server::AppState {
                        webview_router_to_websocket_rx,
                        to_webview_router_tx: to_webview_router_tx.clone(),
                        playground_port: port_picks.playground_port,
                        proxy_port: port_picks.proxy_port,
                        editor_config: std::sync::Arc::new(std::sync::RwLock::new(
                            playground_server::config::EditorConfig::default(),
                        )),
                        file_access: playground_server::fs::WorkspaceFileAccess::new(vec![
                            std::env::current_dir()
                                .unwrap_or_else(|_| std::path::PathBuf::from(".")),
                        ]),
                    },
                };
                let fut = server.run(port_picks.playground_listener).await;
                eprintln!("Playground server finished");
                fut
            },
            cors_bypass_proxy::ProxyServer {}.run(port_picks.proxy_listener),
        ));
    }

    eprintln!(
        "Playground started on: http://localhost:{}",
        port_picks.playground_port
    );
    eprintln!(
        "Proxy started on: http://localhost:{}",
        port_picks.proxy_port
    );

    let four = NonZeroUsize::new(4).unwrap();

    // by default, we set the number of worker threads to `num_cpus`, with a maximum of 4.
    let worker_threads = std::thread::available_parallelism()
        .unwrap_or(four)
        .max(four);

    Server::new(
        worker_threads,
        ServerArgs {
            tokio_runtime,
            webview_router_to_websocket_tx,
            to_webview_router_rx,
            to_webview_router_tx: to_webview_router_tx.clone(),
            playground_port: port_picks.playground_port,
            proxy_port: port_picks.proxy_port,
        },
    )
    .context("Failed to start server")?
    .run()
    .context("Failed to run server")?;
    Ok(())
}
