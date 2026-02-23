use tokio::signal;

pub async fn wait_for_shutdown() -> Result<(), Box<dyn std::error::Error>> {
    log::info!("AeroWAN daemon running — waiting for shutdown signal");
    signal::ctrl_c().await?;
    Ok(())
}
