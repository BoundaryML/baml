use repro::run_server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_server().await?;
    Ok(())
}
