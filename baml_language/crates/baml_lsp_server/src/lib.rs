//! `baml_lsp_server2` — Native LSP server for BAML using `bex_project`.
//!
//! This crate provides a native (stdio) LSP server that delegates all
//! LSP logic to `bex_project::BexLsp`. It acts as the native counterpart
//! to `bridge_wasm`, providing:
//!
//! - Stdio transport for LSP messages
//! - Native filesystem (VFS) for project file access
//! - Playground HTTP/WS server for webview communication
//! - Fetch log interception for the playground
//! - Env var resolution via the playground webview
//!
//! # Architecture
//!
//! ```text
//!  ┌────────────┐   stdio    ┌──────────────────┐
//!  │  LSP Client│ <--------> │  baml_lsp_server2 │
//!  │  (VS Code) │            │                    │
//!  └────────────┘            │  ┌──────────────┐  │
//!                            │  │  bex_project  │  │
//!  ┌────────────┐   ws      │  │  (BexLsp)     │  │
//!  │  Playground│ <--------> │  └──────────────┘  │
//!  │  Webview   │            │                    │
//!  └────────────┘            └──────────────────────┘
//! ```
//!
//! `bex_project` handles all LSP protocol logic. This crate only provides:
//! - Transport (stdio reader/writer, WS server)
//! - Native implementations of `SysOps` (with playground interception)
//! - `LspClientSenderTrait` and `PlaygroundSender` implementations
//!
//! **TLS:** Enable exactly one of `native-tls` or `rustls`. CI may build with
//! `--all-features` (both enabled); prefer one when building the LSP binary.

mod native_lsp_sender;
mod native_vfs;
pub mod playground_env;
pub mod playground_http;
pub mod playground_sender;
pub mod playground_server;
pub mod playground_ws;

use std::sync::Arc;

use playground_env::{PlaygroundEnv, PlaygroundEnvState};
use playground_http::{PlaygroundHttp, PlaygroundHttpState};
use playground_ws::WsOutMessage;
use tokio::net::TcpListener;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Build `SysOps` for a playground-connected project.
///
/// Uses native FS/sys/net but intercepts HTTP (for fetch logs) and env
/// (for webview-resolved env vars).
fn build_playground_sys_ops(
    broadcast_tx: &tokio::sync::broadcast::Sender<WsOutMessage>,
    env_state: &Arc<PlaygroundEnvState>,
) -> sys_types::SysOps {
    let http_state = Arc::new(PlaygroundHttpState::new(broadcast_tx.clone()));
    sys_types::SysOpsBuilder::new()
        .with_fs::<sys_native::NativeSysOps>()
        .with_sys::<sys_native::NativeSysOps>()
        .with_net::<sys_native::NativeSysOps>()
        .with_http_instance(Arc::new(PlaygroundHttp(http_state)))
        .with_env_instance(Arc::new(PlaygroundEnv(env_state.clone())))
        .build()
}

/// Run the native BAML LSP server.
///
/// This is the main entry point. It:
/// 1. Creates the tokio runtime and broadcast channel
/// 2. Sets up native VFS and playground-intercepting SysOps
/// 3. Creates `bex_project::BexLsp` via `bex_project::new_lsp`
/// 4. Starts the playground HTTP/WS server
/// 5. Runs the stdio LSP event loop
pub fn run_server(playground_via_browser: bool) -> anyhow::Result<()> {
    // Set up tracing → stderr so vscode-languageclient captures it
    // in the "BAML Language Server" output channel.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
        )
        .with_ansi(false)
        .init();

    tracing::info!("baml-lsp v{} starting", version());

    let tokio_runtime = tokio::runtime::Runtime::new()?;

    // Broadcast channel for playground WS messages (fetch logs, env requests, etc.)
    let (broadcast_tx, _) = tokio::sync::broadcast::channel::<WsOutMessage>(64);
    let env_state = Arc::new(PlaygroundEnvState::new(broadcast_tx.clone()));

    // Build SysOps with playground interception.
    // The factory creates the same ops for every project.
    let broadcast_tx_for_factory = broadcast_tx.clone();
    let env_state_for_factory = env_state.clone();
    #[allow(clippy::type_complexity)]
    let sys_op_factory: Arc<dyn Fn(&vfs::VfsPath) -> Arc<sys_types::SysOps> + Send + Sync> =
        Arc::new(move |_path: &vfs::VfsPath| {
            Arc::new(build_playground_sys_ops(
                &broadcast_tx_for_factory,
                &env_state_for_factory,
            ))
        });

    // Native VFS
    let vfs: Arc<Box<dyn bex_project::BulkReadFileSystem>> =
        Arc::new(Box::new(native_vfs::NativeVfs::new()));
    let baml_vfs = bex_project::BamlVFS::new(vfs);

    // Stdio sender (LSP client sender)
    let (writer_tx, writer_rx) = crossbeam::channel::unbounded::<lsp_server::Message>();
    let writer_tx = Arc::new(writer_tx);
    let lsp_sender: Arc<dyn bex_project::LspClientSenderTrait + Send + Sync> =
        Arc::new(native_lsp_sender::NativeLspSender::new(&writer_tx));

    // Pick the playground port early so we can pass it to the sender.
    let (playground_listener, playground_port): (Option<TcpListener>, u16) =
        match tokio_runtime.block_on(playground_server::pick_port(3700, 100)) {
            Ok((listener, port)) => (Some(listener), port),
            Err(e) => {
                tracing::error!("Could not find playground port: {e}");
                (None, 0)
            }
        };

    // Playground sender (needs port + lsp_sender for OpenPlayground)
    let playground_sender = Arc::new(playground_sender::NativePlaygroundSender::new(
        broadcast_tx.clone(),
        lsp_sender.clone(),
        playground_port,
        playground_via_browser,
    ));

    // Start the native event sink if BAML_TRACE_FILE is set.
    let event_sink = std::env::var("BAML_TRACE_FILE")
        .ok()
        .map(|trace_file| bex_events_native::start(trace_file.into()));
    let event_sink_for_flush = event_sink.clone();

    // Create the BexLsp (multi-project LSP)
    let bex = bex_project::new_lsp(
        sys_op_factory,
        lsp_sender,
        playground_sender,
        baml_vfs,
        event_sink,
    );
    let bex: Arc<dyn bex_project::BexLsp> = Arc::new(bex);

    // Start playground HTTP/WS server
    if let Some(listener) = playground_listener {
        let bex_for_playground = bex.clone();
        let btx = broadcast_tx.clone();
        let es = env_state.clone();
        tokio_runtime.spawn(async move {
            if let Err(e) = playground_server::run(listener, bex_for_playground, btx, es).await {
                tracing::error!("Playground server exited: {e}");
            }
        });
    }

    // Spawn the stdout writer thread.
    std::thread::Builder::new()
        .name("lsp-stdout-writer".into())
        .spawn(move || {
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();
            while let Ok(msg) = writer_rx.recv() {
                if msg.write(&mut stdout).is_err() {
                    break;
                }
            }
        })?;

    // Main event loop: read from stdin, dispatch to bex_project.
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();

    // Main event loop — forward all messages to bex_project.
    // The `initialize` handshake is handled by `bex_project` via `handle_request`.
    loop {
        let msg = match lsp_server::Message::read(&mut stdin) {
            Ok(Some(msg)) => msg,
            Ok(None) => break,
            Err(e) => {
                tracing::error!("Failed to read LSP message: {e}");
                break;
            }
        };

        match msg {
            lsp_server::Message::Notification(notification) => {
                tracing::debug!("<<< notification: {}", notification.method);
                if notification.method == "exit" {
                    break;
                }
                bex.handle_notification(notification);
            }
            lsp_server::Message::Request(request) => {
                tracing::debug!("<<< request: {} (id={})", request.method, request.id);
                if request.method == "shutdown" {
                    let response = lsp_server::Response {
                        id: request.id,
                        result: Some(serde_json::Value::Null),
                        error: None,
                    };
                    let _ = writer_tx.send(lsp_server::Message::Response(response));
                    continue;
                }
                bex.handle_request(request);
            }
            lsp_server::Message::Response(response) => {
                tracing::debug!("<<< response from client: {:?}", response.id);
            }
        }
    }

    tracing::info!("LSP server shutting down");
    if let Some(sink) = event_sink_for_flush {
        sink.flush();
    }
    Ok(())
}
