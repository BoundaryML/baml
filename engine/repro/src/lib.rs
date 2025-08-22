use serde_json::json;
use tokio::io::AsyncBufReadExt;

use crate::definitions::{FrontendMessage, PreSendToWasmMessage};

pub mod definitions;
pub mod server;

pub async fn run_server() -> anyhow::Result<()> {

    let (playground_tx, mut playground_rx) = tokio::sync::broadcast::channel(1000);
    let (broadcast_tx, broadcast_rx) = tokio::sync::broadcast::channel(1000);

    let port_picks = playground_server::pick_ports(playground_server::PortConfiguration {
        base_port: 3900,
        max_attempts: 100,
    }).await?;
    let server = server::Playground2Server {
        app_state: playground_server::AppState {
            broadcast_rx,
            playground_tx,
            playground_port: port_picks.playground_port,
            proxy_port: port_picks.proxy_port,
        },
    };

    let playground_task = tokio::spawn( server.run(port_picks.playground_listener));

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
                    PreSendToWasmMessage::Initialized => {
                        tracing::info!("Playground initialized");
                        let _  = broadcast_tx.send(playground_server::LangServerToWasmMessage::PlaygroundMessage(
                            playground_server::FrontendMessage::add_project {
                                root_path: "/Users/sam/baml4/engine/baml-runtime/src/cli/initial_project/baml_src".to_string(),
                                files: std::collections::HashMap::new(),
                            }
                        ));
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        let playground_message = playground_server::LangServerToWasmMessage::PlaygroundMessage(
                            playground_server::FrontendMessage::run_test {
                                function_name: "ExtractResume".to_string(),
                                test_name: "vaibhav_resume".to_string(),
                            }
                        );
                        tracing::info!("Sending playground message: {:?}", playground_message);
                        let _  = broadcast_tx.send(playground_message);
                    }
                    PreSendToWasmMessage::FrontendMessage(msg) => {
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
            let Ok(Some(line)) = lines.next_line().await else {
                break;
            };
            let playground_message = playground_server::LangServerToWasmMessage::PlaygroundMessage(
                playground_server::FrontendMessage::run_test {
                    function_name: "ExtractResume".to_string(),
                    test_name: "vaibhav_resume".to_string(),
                }
            );
            tracing::info!("Sending playground message: {:?}", playground_message);
            let _  = broadcast_tx.send(playground_message);
        }
        
        Ok::<(), anyhow::Error>(())
    });

    let _ = playground_task.await?;

    Ok(())
}
