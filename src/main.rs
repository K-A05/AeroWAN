mod daemon;
mod transport;
mod network;
mod utils;
mod types;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let daemon = daemon::Daemon::new().await?;
    daemon.run().await
}
