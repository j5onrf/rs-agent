use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    rs_agent::bridge::run_bridge().await
}
