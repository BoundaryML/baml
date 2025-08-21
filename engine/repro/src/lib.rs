use serde_json::json;
use tokio::io::AsyncBufReadExt;

use crate::definitions::{FrontendMessage, PreSendToWasmMessage};

pub mod definitions;
pub mod ping_handler;
pub mod port_picker;
pub mod server;
mod websocket_rpc_handler;
mod websocket_ws_handler;

pub async fn run_server() -> anyhow::Result<()> {

    let (playground_tx, mut playground_rx) = tokio::sync::broadcast::channel(1000);
    let (broadcast_tx, broadcast_rx) = tokio::sync::broadcast::channel(1000);

    let port_picks = port_picker::pick().await?;
    let server = server::Playground2Server {
        app_state: server::AppState {
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
                        let _  = broadcast_tx.send(server::LangServerToWasmMessage::PlaygroundMessage(serde_json::from_value(json!(
                            {"command":"add_project","content":{"root_path":"/Users/sam/baml4/engine/baml-runtime/src/cli/initial_project/baml_src","files":{"/Users/sam/baml4/engine/baml-runtime/src/cli/initial_project/baml_src/resume.baml":"// Defining a data model.\nclass Resume {\n  name string\n  email string\n  experience string[]\n  skills string[]\n}\n// Create a function to extract the resume from a string.\nfunction ZippityDoo(resume: string) -> Resume {\n  // Specify a client as provider/model-name\n  // you can use custom LLM params with a custom client name from clients.baml like \"client CustomHaiku\"\n  client \"openai/gpt-4o\" // Set OPENAI_API_KEY to use this client.\n  prompt #\"\n    Extract lorem ipsum this content:\n    {{ resume }}\n\n    {{ ctx.output_format }}\n  \"#\n}\n\n// Create a function to extract the resume from a string.\nfunction ExtractResume(resume: string) -> Resume {\n  // Specify a client as provider/model-name\n  // you can use custom LLM params with a custom client name from clients.baml like \"client CustomHaiku\"\n  client CustomGPT4oMini // Set OPENAI_API_KEY to use this client.\n  prompt #\"\n    Extract rerererere this content:\n    {{ resume }}\n\n    {{ ctx.output_format }}\n  \"#\n}\n\n\n\n// Test the function with a sample resume. Open the VSCode playground to run this.\ntest vaibhav_resume {\n  functions [ExtractResume, ZippityDoo]\n  args {\n    resume #\"\n      Vaibhav Gupta\n      vbv@boundaryml.com\n\n      Experience:\n      - Founder at BoundaryML\n      - CV Engineer at Google\n      - CV Engineer at Microsoft\n\n      Skills:\n      - Rust\n      - C++\n    \"#\n  }\n}\n","/Users/sam/baml4/engine/baml-runtime/src/cli/initial_project/baml_src/clients.baml":"// Learn more about clients at https://docs.boundaryml.com/docs/snippets/clients/overview\n\nclient<llm> CustomGPT4o {\n  provider openai\n  options {\n    model \"gpt-4o\"\n    api_key env.OPENAI_API_KEY\n  }\n}\n\nclient<llm> CustomGPT4oMini {\n  provider openai\n  retry_policy Exponential\n  options {\n    model \"gpt-4o-mini\"\n    api_key env.OPENAI_API_KEY\n  }\n}\n\nclient<llm> CustomSonnet {\n  provider anthropic\n  options {\n    model \"claude-3-5-sonnet-20241022\"\n    api_key env.ANTHROPIC_API_KEY\n  }\n}\n\n\nclient<llm> CustomHaiku {\n  provider anthropic\n  retry_policy Constant\n  options {\n    model \"claude-3-haiku-20240307\"\n    api_key env.ANTHROPIC_API_KEY\n  }\n}\n\n// https://docs.boundaryml.com/docs/snippets/clients/round-robin\nclient<llm> CustomFast {\n  provider round-robin\n  options {\n    // This will alternate between the two clients\n    strategy [CustomGPT4oMini, CustomHaiku]\n  }\n}\n\n// https://docs.boundaryml.com/docs/snippets/clients/fallback\nclient<llm> OpenaiFallback {\n  provider fallback\n  options {\n    // This will try the clients in order until one succeeds\n    strategy [CustomGPT4oMini, CustomGPT4oMini]\n  }\n}\n\n// https://docs.boundaryml.com/docs/snippets/clients/retry\nretry_policy Constant {\n  max_retries 3\n  // Strategy is optional\n  strategy {\n    type constant_delay\n    delay_ms 200\n  }\n}\n\nretry_policy Exponential {\n  max_retries 2\n  // Strategy is optional\n  strategy {\n    type exponential_backoff\n    delay_ms 300\n    multiplier 1.5\n    max_delay_ms 10000\n  }\n}"}}}
                        ))?));
            // let playground_message = (server::LangServerToWasmMessage::PlaygroundMessage(serde_json::from_value(json!(
            //     {"command":"run_test","content":{"function_name":"ExtractResume","test_name":"vaibhav_resume"}}
            // ))?));
            // tracing::info!("Sending playground message: {:?}", playground_message);
            // let _  = broadcast_tx.send(playground_message);
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
            let playground_message = (server::LangServerToWasmMessage::PlaygroundMessage(serde_json::from_value(json!(
                {"command":"run_test","content":{"function_name":"ExtractResume","test_name":"vaibhav_resume"}}
            ))?));
            tracing::info!("Sending playground message: {:?}", playground_message);
            let _  = broadcast_tx.send(playground_message);
        }
        
        Ok::<(), anyhow::Error>(())
    });

    let _ = playground_task.await?;

    Ok(())
}
