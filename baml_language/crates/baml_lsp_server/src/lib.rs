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

mod deadlock_watchdog;
mod native_lsp_sender;
mod native_vfs;
pub mod playground_env;
pub mod playground_http;
pub mod playground_io;
pub mod playground_runs;
pub mod playground_sender;
pub mod playground_server;
pub mod playground_session;
pub mod playground_ws;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

use anyhow::Context as _;
use playground_env::{PlaygroundEnv, PlaygroundEnvState};
use playground_http::{PlaygroundHttp, PlaygroundHttpState};
use playground_io::{PlaygroundIo, PlaygroundIoState};
use playground_session::PlaygroundSessionStore;
use playground_ws::WsOutMessage;
use tokio::net::TcpListener;

pub fn version() -> &'static str {
    baml_version::CANONICAL_VERSION
}

/// Build `SysOps` for a playground-connected project.
///
/// Uses native FS/sys/net but intercepts HTTP (for fetch logs) and env
/// (for webview-resolved env vars).
fn build_playground_sys_ops(
    broadcast_tx: &tokio::sync::broadcast::Sender<WsOutMessage>,
    run_store: &Arc<bex_events::run::InMemoryRunStore>,
    env_state: &Arc<PlaygroundEnvState>,
    io_state: &Arc<PlaygroundIoState>,
) -> sys_ops::SysOps {
    let http_state = Arc::new(PlaygroundHttpState::new(
        broadcast_tx.clone(),
        run_store.clone(),
    ));
    sys_ops::SysOpsBuilder::new()
        .with_fs::<sys_native::NativeSysOps>()
        .with_sys::<sys_native::NativeSysOps>()
        .with_net::<sys_native::NativeSysOps>()
        .with_http_instance(Arc::new(PlaygroundHttp(http_state)))
        .with_env_instance(Arc::new(PlaygroundEnv(env_state.clone())))
        .with_io_instance(Arc::new(PlaygroundIo(io_state.clone())))
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
pub fn run_server(workspace_roots: Vec<PathBuf>) -> anyhow::Result<()> {
    run_server_inner(PlaygroundOpenTarget::LspClient, workspace_roots, None)
}

pub fn run_playground_server(
    workspace_roots: Vec<PathBuf>,
    playground_dir_override: Option<PathBuf>,
) -> anyhow::Result<()> {
    run_server_inner(
        PlaygroundOpenTarget::Browser,
        workspace_roots,
        playground_dir_override,
    )
}

#[derive(Clone, Copy)]
enum PlaygroundOpenTarget {
    LspClient,
    Browser,
}

fn run_server_inner(
    playground_open_target: PlaygroundOpenTarget,
    workspace_roots: Vec<PathBuf>,
    playground_dir_override: Option<PathBuf>,
) -> anyhow::Result<()> {
    let workspace_roots = absolutize_workspace_roots(workspace_roots)?;

    // Set up tracing → stderr so vscode-languageclient captures it
    // in the "BAML Language Server" output channel.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,salsa=warn")),
        )
        .with_ansi(false)
        .init();

    apply_single_workspace_cwd(&workspace_roots)?;

    tracing::info!("baml-lsp v{} starting", version());
    deadlock_watchdog::spawn();

    let tokio_runtime = tokio::runtime::Runtime::new()?;

    // Broadcast channel for playground WS messages (fetch logs, env requests, etc.)
    let (broadcast_tx, _) = tokio::sync::broadcast::channel::<WsOutMessage>(64);
    let run_store = Arc::new(bex_events::run::InMemoryRunStore::default());
    let _profile_observer = bex_events::run::register_profile_observer(Arc::new(
        playground_runs::RunStoreProfileObserver::new(run_store.clone(), broadcast_tx.clone()),
    ));
    let session_store = Arc::new(PlaygroundSessionStore::default());
    let env_state = Arc::new(PlaygroundEnvState::new(
        broadcast_tx.clone(),
        run_store.clone(),
        session_store,
    ));
    let io_state = Arc::new(PlaygroundIoState::new(
        broadcast_tx.clone(),
        run_store.clone(),
    ));

    // Build SysOps with playground interception.
    // The factory creates the same ops for every project.
    let broadcast_tx_for_factory = broadcast_tx.clone();
    let run_store_for_factory = run_store.clone();
    let env_state_for_factory = env_state.clone();
    let io_state_for_factory = io_state.clone();
    #[allow(clippy::type_complexity)]
    let sys_op_factory: Arc<dyn Fn(&vfs::VfsPath) -> Arc<sys_ops::SysOps> + Send + Sync> =
        Arc::new(move |_path: &vfs::VfsPath| {
            Arc::new(build_playground_sys_ops(
                &broadcast_tx_for_factory,
                &run_store_for_factory,
                &env_state_for_factory,
                &io_state_for_factory,
            ))
        });

    // Native VFS
    let vfs: Arc<Box<dyn bex_project::BulkReadFileSystem>> =
        Arc::new(Box::new(native_vfs::NativeVfs::new()));
    let baml_vfs = bex_project::BamlVFS::new(vfs);

    // Stdio sender (LSP client sender)
    let (writer_tx, writer_rx) = crossbeam_channel::unbounded::<lsp_server::Message>();
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
    let playground_sender: Arc<dyn bex_project::PlaygroundSender> =
        Arc::new(playground_sender::NativePlaygroundSender::new(
            broadcast_tx.clone(),
            lsp_sender.clone(),
            playground_port,
            matches!(playground_open_target, PlaygroundOpenTarget::Browser),
        ));

    // Create the BexLsp (multi-project LSP)
    let spawner = bex_project::BackgroundSpawner::with_handle(tokio_runtime.handle().clone());
    let bex = bex_project::new_lsp(
        sys_op_factory,
        lsp_sender,
        playground_sender.clone(),
        baml_vfs,
        spawner,
    );
    let bex: Arc<dyn bex_project::BexLsp> = Arc::new(bex);
    run_store.set_graph_runtime_overlay_span_provider(Arc::new(
        playground_runs::ProjectGraphRuntimeOverlaySpanProvider::new(bex.clone()),
    ));

    let has_explicit_workspace_roots = !workspace_roots.is_empty();
    let explicit_projects = if has_explicit_workspace_roots {
        bex.initialize_workspace_roots(workspace_roots.clone())?
    } else {
        Vec::new()
    };
    if matches!(playground_open_target, PlaygroundOpenTarget::Browser)
        && has_explicit_workspace_roots
    {
        spawn_standalone_workspace_poller(bex.clone(), workspace_roots.clone())?;
    }

    if matches!(playground_open_target, PlaygroundOpenTarget::Browser) && playground_port != 0 {
        if let Some(project) = explicit_projects.first() {
            playground_sender.send_playground_notification(
                bex_project::PlaygroundNotification::OpenPlayground {
                    project: project.clone(),
                    function_name: None,
                    test_name: None,
                    testset_name: None,
                },
            );
        } else if has_explicit_workspace_roots {
            tracing::warn!("No BAML projects discovered for explicit workspace roots");
        }
    }

    // Start playground HTTP/WS server. In editor/LSP mode it runs in the
    // background while stdio drives the process. In browser mode it is the
    // foreground task; otherwise a terminal stdin EOF would shut down the
    // playground immediately.
    if let Some(listener) = playground_listener {
        let bex_for_playground = bex.clone();
        let btx = broadcast_tx.clone();
        let es = env_state.clone();
        let ios = io_state.clone();
        let runs = run_store.clone();
        let playground_dir = playground_dir_override.clone();

        if matches!(playground_open_target, PlaygroundOpenTarget::Browser) {
            return tokio_runtime.block_on(playground_server::run(
                listener,
                bex_for_playground,
                btx,
                es,
                ios,
                runs,
                playground_dir,
            ));
        }

        tokio_runtime.spawn(async move {
            if let Err(e) = playground_server::run(
                listener,
                bex_for_playground,
                btx,
                es,
                ios,
                runs,
                playground_dir,
            )
            .await
            {
                tracing::error!("Playground server exited: {e}");
            }
        });
    } else if matches!(playground_open_target, PlaygroundOpenTarget::Browser) {
        anyhow::bail!("Could not start playground server");
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
    Ok(())
}

fn absolutize_workspace_roots(workspace_roots: Vec<PathBuf>) -> anyhow::Result<Vec<PathBuf>> {
    if workspace_roots.iter().all(|root| root.is_absolute()) {
        return Ok(workspace_roots);
    }

    let cwd = std::env::current_dir().context("Failed to read current directory")?;
    Ok(workspace_roots
        .into_iter()
        .map(|root| {
            if root.is_absolute() {
                root
            } else {
                cwd.join(root)
            }
        })
        .collect())
}

fn apply_single_workspace_cwd(workspace_roots: &[PathBuf]) -> anyhow::Result<()> {
    let [root] = workspace_roots else {
        return Ok(());
    };

    let cwd = workspace_cwd(root);
    std::env::set_current_dir(&cwd)
        .with_context(|| format!("Failed to set current directory to {}", cwd.display()))?;
    tracing::info!(
        "Using {} as standalone LSP current directory",
        cwd.display()
    );
    Ok(())
}

fn workspace_cwd(root: &Path) -> PathBuf {
    if root.is_file() {
        root.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.to_path_buf())
    } else {
        root.to_path_buf()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WorkspaceSignature {
    files: BTreeMap<PathBuf, FileSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileSignature {
    len: u64,
    modified_ns: Option<u128>,
}

fn spawn_standalone_workspace_poller(
    bex: Arc<dyn bex_project::BexLsp>,
    workspace_roots: Vec<PathBuf>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("playground-workspace-poller".to_string())
        .spawn(move || {
            let mut last_signature = workspace_signature(&workspace_roots);
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let next_signature = workspace_signature(&workspace_roots);
                if next_signature == last_signature {
                    continue;
                }
                last_signature = next_signature;
                tracing::info!("Detected standalone workspace file change; refreshing playground");
                if let Err(err) = bex.initialize_workspace_roots(workspace_roots.clone()) {
                    tracing::warn!("Failed to refresh standalone playground workspace: {err}");
                }
            }
        })?;
    Ok(())
}

fn workspace_signature(workspace_roots: &[PathBuf]) -> WorkspaceSignature {
    let mut files = BTreeMap::new();
    for root in workspace_roots {
        collect_workspace_signature(root, &mut files);
    }
    WorkspaceSignature { files }
}

fn collect_workspace_signature(path: &Path, files: &mut BTreeMap<PathBuf, FileSignature>) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.is_file() {
        if watches_standalone_workspace_file(path) {
            files.insert(path.to_path_buf(), file_signature(&metadata));
        }
        return;
    }
    if !metadata.is_dir() || should_skip_poll_dir(path) {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        collect_workspace_signature(&entry.path(), files);
    }
}

fn watches_standalone_workspace_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "baml.toml")
        || path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension == "baml")
}

fn should_skip_poll_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                ".baml" | ".git" | ".next" | ".turbo" | "dist" | "node_modules" | "target"
            )
        })
}

fn file_signature(metadata: &fs::Metadata) -> FileSignature {
    FileSignature {
        len: metadata.len(),
        modified_ns: metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolutize_workspace_roots_makes_relative_paths_absolute() {
        let cwd = std::env::current_dir().expect("cwd should be available");
        let absolute = cwd.join("already-absolute");

        let roots =
            absolutize_workspace_roots(vec![PathBuf::from("relative-workspace"), absolute.clone()])
                .expect("workspace roots should absolutize");

        assert_eq!(roots, vec![cwd.join("relative-workspace"), absolute]);
    }

    #[test]
    fn workspace_cwd_uses_file_parent_and_keeps_directories() {
        let dir = std::env::temp_dir().join(format!(
            "baml-lsp-workspace-cwd-test-{}",
            std::process::id()
        ));
        let file = dir.join("project.baml");

        std::fs::create_dir_all(&dir).expect("temp dir should be created");
        std::fs::write(&file, "function Test() -> int { 1 }\n")
            .expect("temp file should be created");

        assert_eq!(workspace_cwd(&dir), dir);
        assert_eq!(workspace_cwd(&file), file.parent().unwrap());

        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn standalone_workspace_poller_watches_sources_and_skips_generated_dirs() {
        assert!(watches_standalone_workspace_file(Path::new("baml.toml")));
        assert!(watches_standalone_workspace_file(Path::new(
            "baml_src/main.baml"
        )));
        assert!(!watches_standalone_workspace_file(Path::new(
            ".baml/profiles/run.bamlprof"
        )));
        assert!(should_skip_poll_dir(Path::new(".baml")));
        assert!(should_skip_poll_dir(Path::new("target")));
        assert!(!should_skip_poll_dir(Path::new("baml_src")));
    }
}
