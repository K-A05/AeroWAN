use crate::{daemon::config::Config, utils::identity::load_or_create_iroh_key};
use iroh::Endpoint;

pub struct IrohTransport;

impl IrohTransport {
    pub async fn init(config: &Config, config_dir: &std::path::Path) -> Result<Option<Endpoint>, Box<dyn std::error::Error>> {
        if !config.iroh.enabled {
            log::info!("Iroh disabled in config — skipping");
            return Ok(None);
        }
        let secret_key = load_or_create_iroh_key(config_dir)?;
        let endpoint = if config.iroh.bind_port != 0 {
        let addr: std::net::SocketAddr = 
            format!("0.0.0.0:{}", config.iroh.bind_port).parse()?;
        Endpoint::builder()
            .secret_key(secret_key)
            .bind_addr(addr)?
            .bind()
            .await?
        }  else {
        Endpoint::bind().await?
        };

        log::info!("Iroh endpoint started — NodeID: {}", endpoint.id());
        Ok(Some(endpoint))
    }
}