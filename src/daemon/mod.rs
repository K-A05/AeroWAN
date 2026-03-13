pub mod api;
pub mod config;
pub mod signals;

use crate::daemon::api::LANServer;
use crate::transport::iroh::IrohTransport;
use crate::transport::reticulum::ReticulumTransport;
use config::Config;
use iroh::Endpoint;
use reticulum::transport::Transport;

pub struct Daemon {
    transport: Transport,
    iroh_endpoint: Option<Endpoint>,
    api_server: LANServer,
    #[allow(dead_code)] // config_path will be used in the future for dynamic config reloads.
    config_path: std::path::PathBuf,
}

impl Daemon {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (config, config_path) = Config::load()?;

        env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or(config.log_filter()),
        )
        .init();

        log::info!("AeroWAN daemon starting");
        log::info!("Configuration loaded from: {}", config_path.display());

        let transport = ReticulumTransport::init(&config, &config_path).await?;
        let iroh_endpoint = IrohTransport::init(&config, &config_path).await?;
        let api_server = LANServer::start(&config, &config_path, iroh_endpoint.clone()).await?;

        Ok(Self {
            transport,
            iroh_endpoint,
            config_path,
            api_server,
        })
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        signals::wait_for_shutdown().await?;
        log::info!("Shutdown signal received, cleaning up");
        drop(self.iroh_endpoint);
        drop(self.transport);
        log::info!("AeroWAN daemon stopped");
        Ok(())
    }
}
