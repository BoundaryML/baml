use std::collections::HashMap;

use playground_server::{
    pick_ports, AppState, FrontendMessage, LangServerToWasmMessage, PlaygroundServer,
    PortConfiguration, PreLangServerToWasmMessage,
};
use tokio::io::AsyncBufReadExt;
use tracing_subscriber::EnvFilter;

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

pub async fn run_server() -> anyhow::Result<()> {
    let (playground_tx, mut playground_rx) = tokio::sync::broadcast::channel(1000);
    let (broadcast_tx, broadcast_rx) = tokio::sync::broadcast::channel(1000);

    let port_picks = pick_ports(PortConfiguration {
        base_port: 3900,
        max_attempts: 100,
    })
    .await?;

    let server = Playground2Server {
        app_state: AppState {
            broadcast_rx,
            playground_tx,
            playground_port: port_picks.playground_port,
            proxy_port: port_picks.proxy_port,
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
        let broadcast_tx = broadcast_tx.clone();
        tokio::spawn(async move {
            while let Ok(msg) = playground_rx.recv().await {
                tracing::info!("Received message from playground: {:?}", msg);
                match msg {
                    PreLangServerToWasmMessage::WasmIsInitialized => {
                        tracing::info!("Playground initialized");
                        let _  = broadcast_tx.send(LangServerToWasmMessage::PlaygroundMessage(
                            FrontendMessage::add_project {
                                root_path: "/Users/sam/baml4/engine/baml-runtime/src/cli/initial_project/baml_src".to_string(),
                                files: HashMap::new(),
                            }
                        ));
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        let playground_message =
                            LangServerToWasmMessage::PlaygroundMessage(FrontendMessage::run_test {
                                function_name: "ExtractResume".to_string(),
                                test_name: "vaibhav_resume".to_string(),
                            });
                        tracing::info!("Sending playground message: {:?}", playground_message);
                        let _ = broadcast_tx.send(playground_message);
                    }
                    PreLangServerToWasmMessage::FrontendMessage(msg) => {
                        tracing::info!("Received frontend message: {:?}", msg);
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
                LangServerToWasmMessage::PlaygroundMessage(FrontendMessage::run_test {
                    function_name: "ExtractResume".to_string(),
                    test_name: "vaibhav_resume".to_string(),
                });
            tracing::info!("Sending playground message: {:?}", playground_message);
            let _ = broadcast_tx.send(playground_message);
        }

        Ok::<(), anyhow::Error>(())
    });

    let _ = playground_task.await?;

    Ok(())
}
