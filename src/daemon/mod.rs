pub mod api;
pub mod config;
pub mod signals;

use crate::daemon::api::LANServer;
use crate::transport::iroh::{IrohNode, IrohTransport};
use crate::transport::reticulum::ReticulumTransport;
use config::Config;
use reticulum::transport::Transport;
use std::sync::Arc; // making use of arc(Atomic reference counter because it is thread safe, this programme conatins multiple threads in order to be able to handle in/outbound connections)
//------------------------------------------------------------------------------------------------------------------------------

pub struct Daemon {
    transport: Transport,             // reticulum Transport
    iroh_node: Option<Arc<IrohNode>>, // iroh might not be enabled by default, keep an Arc reference for async access across threads.
    #[allow(dead_code)] // config_path will be used in the future for dynamic config reloads.
    api_server: LANServer, // struct to handle async tasks to and from the app.
    config_path: std::path::PathBuf, // the path where configurations for the application will be stored.
}

impl Daemon {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (config, config_path) = Config::load()?;

        // env_logger::Builder::from_env(
        //     env_logger::Env::default().default_filter_or(config.log_filter()),
        // )
        // .init();

        log::info!("AeroWAN daemon starting");
        log::info!("Configuration loaded from: {}", config_path.display());

        let transport = ReticulumTransport::init(&config, &config_path).await?;
        let iroh_node: Option<Arc<IrohNode>> = IrohTransport::init(&config, &config_path)
            .await?
            .map(Arc::new);
        let api_server = LANServer::start(&config, &config_path, iroh_node.clone()).await?;

        Ok(Self {
            transport,
            iroh_node,
            config_path,
            api_server,
        })
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        signals::wait_for_shutdown().await?;
        log::info!("Shutdown signal received, cleaning up");
        drop(self.iroh_node);
        drop(self.transport);
        log::info!("AeroWAN daemon stopped");
        Ok(())
    }
}
