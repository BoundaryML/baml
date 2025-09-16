use std::{collections::HashMap, fs, path::Path};

use playground_server::{
    pick_ports, AppState, FrontendMessage, PlaygroundServer, PortConfiguration,
    WebviewNotification, WebviewRouterMessage,
};
use tokio::io::AsyncBufReadExt;
use tracing_subscriber::EnvFilter;
use walkdir::WalkDir;

// const PROJECT_DIR: &'static str = "/Users/sam/baml4/c2/baml_src";
// const PROJECT_DIR: &'static str = "/Users/sam/baml/engine/playground-server/tests/codelens-bugs";
const PROJECT_DIR: &'static str = "/Users/sam/baml/integ-tests/baml_src";

#[derive(Debug)]
pub struct Playground2Server {
    pub app_state: AppState,
}

impl Playground2Server {
    pub async fn run(
        self,
        listener: tokio::net::TcpListener,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        let server = PlaygroundServer {
            app_state: self.app_state,
        };

        server.run(listener).await
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from("playground_barebones=debug,info"))
        .init();

    run_server().await?;
    Ok(())
}

fn load_project_from_directory(dir_path: &'static str) -> FrontendMessage {
    let mut files = HashMap::new();
    let base_path = Path::new(dir_path);

    for entry in WalkDir::new(dir_path) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();

        // Skip if not a file
        if !path.is_file() {
            continue;
        }

        // Skip if not a .baml file
        if path.extension().and_then(|s| s.to_str()) != Some("baml") {
            continue;
        }

        // Read file content
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Get relative path
        let relative_path = match path.strip_prefix(base_path) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let relative_path_str = match relative_path.to_str() {
            Some(s) => s.to_string(),
            None => continue,
        };

        tracing::debug!("Loading file: {}", relative_path_str);
        files.insert(relative_path_str, content);
    }

    tracing::info!("Loaded {} .baml files from {}", files.len(), dir_path);

    FrontendMessage::add_project {
        root_path: dir_path.to_string(),
        files,
    }
}

pub async fn run_server() -> anyhow::Result<()> {
    let (to_webview_router_tx, mut to_webview_router_rx) = tokio::sync::broadcast::channel(1000);
    let (webview_router_to_websocket_tx, webview_router_to_websocket_rx) =
        tokio::sync::broadcast::channel(1000);

    let port_picks = pick_ports(PortConfiguration {
        base_port: 3900,
        max_attempts: 100,
    })
    .await?;

    let server = Playground2Server {
        app_state: AppState {
            webview_router_to_websocket_rx: webview_router_to_websocket_rx,
            to_webview_router_tx: to_webview_router_tx,
            playground_port: port_picks.playground_port,
            proxy_port: port_picks.proxy_port,
            editor_config: std::sync::Arc::new(std::sync::RwLock::new(
                playground_server::config::EditorConfig::default(),
            )),
            file_access: playground_server::fs::WorkspaceFileAccess::new(vec![
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            ]),
        },
    };

    let playground_task = tokio::spawn(server.run(port_picks.playground_listener));

    tracing::info!(
        "Playground started on: http://localhost:{}",
        port_picks.playground_port
    );
    tracing::info!(
        "Proxy started on: http://localhost:{}",
        port_picks.proxy_port
    );

    {
        let webview_router_to_websocket_tx = webview_router_to_websocket_tx.clone();
        tokio::spawn(async move {
            while let Ok(msg) = to_webview_router_rx.recv().await {
                tracing::info!("Received message from playground: {:?}", msg);
                match msg {
                    WebviewRouterMessage::WasmIsInitialized => {
                        tracing::info!("Playground initialized");
                        let _ = webview_router_to_websocket_tx.send(
                            WebviewNotification::PlaygroundMessage(load_project_from_directory(
                                PROJECT_DIR,
                            )),
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        // let playground_message =
                        //     LangServerToWasmMessage::PlaygroundMessage(FrontendMessage::run_test {
                        //         function_name: "ExtractResume".to_string(),
                        //         test_name: "vaibhav_resume".to_string(),
                        //     });
                        // tracing::info!("Sending playground message: {:?}", playground_message);
                        // let _ = webview_router_to_websocket_tx.send(playground_message);
                        // loop {
                        //     tracing::info!("Sending samtest_update_project {}", chrono::Local::now());
                        //     if let Err(e) =
                        //         webview_router_to_websocket_tx.send(LangServerToWasmMessage::PlaygroundMessage(
                        //             FrontendMessage::samtest_update_project {
                        //                 root_path: PROJECT_DIR.to_string(),
                        //                 files: vec![(
                        //                     "test.baml".to_string(),
                        //                     "// comment\n".to_string(),
                        //                 )]
                        //                 .into_iter()
                        //                 .collect(),
                        //             },
                        //         ))
                        //     {
                        //         tracing::error!("Error sending playground message: {:?}", e);
                        //     };
                        //     tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                        // }
                    }
                    msg => {
                        tracing::info!("Router received: {:?}", msg);
                    }
                }
            }

            Ok::<(), anyhow::Error>(())
        });
    }

    // Start a loop to watch stdin and echo it back
    tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let mut lines = tokio::io::BufReader::new(stdin).lines();

        loop {
            println!("Press enter to send test message");
            let Ok(Some(_line)) = lines.next_line().await else {
                break;
            };
            let playground_message =
                WebviewNotification::PlaygroundMessage(FrontendMessage::run_test {
                    function_name: "TestFnNamedArgsSingleClass".to_string(),
                    test_name: "TestFnNamedArgsSingleClass".to_string(),
                });
            tracing::info!("Sending playground message: {:?}", playground_message);
            let _ = webview_router_to_websocket_tx
                .send(playground_message)
                .inspect_err(|e| {
                    tracing::error!("Error sending playground message: {:?}", e);
                });
            // tracing::info!("Sending samtest_update_project {}", chrono::Local::now());
            // if let Err(e) = webview_router_to_websocket_tx.send(LangServerToWasmMessage::PlaygroundMessage(
            //     FrontendMessage::samtest_update_project {
            //         root_path: PROJECT_DIR.to_string(),
            //         files: vec![("test.baml".to_string(), "// comment\n".to_string())]
            //             .into_iter()
            //             .collect(),
            //     },
            // )) {
            //     tracing::error!("Error sending playground message: {:?}", e);
            // };
        }

        Ok::<(), anyhow::Error>(())
    });

    let _ = playground_task.await?;

    Ok(())
}
