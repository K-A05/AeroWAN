use crate::{daemon::config::Config, utils::identity::load_or_create_iroh_key};
use anyhow::Result;
use iroh::endpoint::Connection;
use iroh::{Endpoint, EndpointId};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct IrohTransport;

pub struct IrohNode {
    pub endpoint: Endpoint,
    pub connections: Arc<Mutex<Vec<Connection>>>,
    #[allow(dead_code)]
    accept_task: tokio::task::JoinHandle<()>,
}

pub const ALPN: &[u8] = b"aerowan/0"; // define the ALPN for the application

impl IrohNode {
    fn spawn_accept_loop(
        endpoint: Endpoint,
        connections: Arc<Mutex<Vec<Connection>>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                match endpoint.accept().await {
                    Some(incoming) => match incoming.await {
                        Ok(connection) => {
                            log::info!("New inbound connection from: {}", connection.remote_id());
                            connections.lock().await.push(connection);
                        }
                        Err(e) => {
                            log::warn!("Failed to accept connection: {}", e);
                        }
                    },
                    None => {
                        log::info!("Iroh endpoint closed, stopping accept loop");
                        break;
                    }
                }
            }
        })
    }
}

impl IrohNode {
    pub async fn connect(
        &self,
        node_id: EndpointId,
    ) -> Result<Connection, Box<dyn std::error::Error>> {
        let connection = self.endpoint.connect(node_id, ALPN).await?;

        log::info!("connected to peer {}", connection.remote_id());
        self.connections.lock().await.push(connection.clone());
        Ok(connection)
    }
}

// impl IrohNode {
//     // create a ticket to send a file
//     pub async fn send(&self, path: &Path) -> anyhow::Result<String> {
//         let ticket: String =
//     }
//     pub async fn recv(&self, ticket: &str, dest: &Path) -> anyhow::Result<()> {}
// }

impl IrohTransport {
    pub async fn init(
        config: &Config,
        config_dir: &std::path::Path,
    ) -> anyhow::Result<Option<IrohNode>> {
        if !config.iroh.enabled {
            log::info!("Iroh disabled in config — skipping");
            return Ok(None);
        }

        let secret_key =
            load_or_create_iroh_key(config_dir).map_err(|e| anyhow::anyhow!("{}", e))?;
        let endpoint = if config.iroh.bind_port != 0 {
            let addr: std::net::SocketAddr =
                format!("0.0.0.0:{}", config.iroh.bind_port).parse()?;
            Endpoint::builder()
                .secret_key(secret_key)
                .bind_addr(addr)?
                .bind()
                .await?
        } else {
            Endpoint::builder().secret_key(secret_key).bind().await?
        };

        endpoint.set_alpns(vec![ALPN.to_vec()]);
        log::info!("Iroh endpoint started — NodeID: {}", endpoint.id());

        let connections = Arc::new(Mutex::new(Vec::new()));
        let accept_task = IrohNode::spawn_accept_loop(endpoint.clone(), connections.clone());

        Ok(Some(IrohNode {
            endpoint,
            connections,
            accept_task,
        }))
    }
}
