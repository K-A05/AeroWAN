use crate::daemon::config::Config;
use iroh::Endpoint;

pub struct IrohTransport;

impl IrohTransport {
    pub async fn init(config: &Config) -> Result<Option<Endpoint>, Box<dyn std::error::Error>> {
        if !config.iroh.enabled {
            log::info!("Iroh disabled in config — skipping");
            return Ok(None);
        }

      let endpoint = if config.iroh.bind_port != 0 {
        let addr: std::net::SocketAddr = 
            format!("0.0.0.0:{}", config.iroh.bind_port).parse()?;
        Endpoint::builder()
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