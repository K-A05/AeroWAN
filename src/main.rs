mod daemon; // declares : daemon, config, signals
mod transport; // contains transport implementations for reticulum and iroh, to create and manage transport endpoints.
mod utils; // implements indentity manangement for Iroh and Reticulum
mod network;
mod types;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let daemon = daemon::Daemon::new().await?; // initialize the daemon.
    daemon.run().await
}
