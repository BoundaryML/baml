use repro::run_server;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from("repro=debug,info"))
        .init();

    run_server().await?;
    Ok(())
}
