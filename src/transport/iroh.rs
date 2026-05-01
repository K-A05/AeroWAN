// use crate::{daemon::config::Config, utils::identity::load_or_create_iroh_key};
// use anyhow::Result;
// use iroh::endpoint::Connection;
// use iroh::{Endpoint, EndpointId};
// use std::collections::VecDeque;
// use std::sync::Arc;
// use tokio::sync::Mutex;

// pub struct IrohTransport;

// #[derive(Debug, Clone, serde::Serialize)]
// pub struct ChatMessage {
//     pub from: String, // node_id of sender
//     pub text: String,
//     pub timestamp: u64, // unix seconds
// }

// pub struct IrohNode {
//     pub endpoint: Endpoint,
//     pub connections: Arc<Mutex<Vec<Connection>>>,
//     pub chat_inbox: Arc<Mutex<VecDeque<ChatMessage>>>,
//     #[allow(dead_code)]
//     accept_task: tokio::task::JoinHandle<()>,
// }

// pub const ALPN: &[u8] = b"aerowan/0"; // define the ALPN for the application

// impl IrohNode {
//     fn spawn_accept_loop(
//         endpoint: Endpoint,
//         connections: Arc<Mutex<Vec<Connection>>>,
//     ) -> tokio::task::JoinHandle<()> {
//         tokio::spawn(async move {
//             loop {
//                 match endpoint.accept().await {
//                     Some(incoming) => match incoming.await {
//                         Ok(connection) => {
//                             log::info!("New inbound connection from: {}", connection.remote_id());
//                             connections.lock().await.push(connection);
//                         }
//                         Err(e) => {
//                             log::warn!("Failed to accept connection: {}", e);
//                         }
//                     },
//                     None => {
//                         log::info!("Iroh endpoint closed, stopping accept loop");
//                         break;
//                     }
//                 }
//             }
//         })
//     }
// }

// impl IrohNode {
//     pub async fn connect(
//         &self,
//         node_id: EndpointId,
//     ) -> Result<Connection, Box<dyn std::error::Error>> {
//         let connection = self.endpoint.connect(node_id, ALPN).await?;

//         log::info!("connected to peer {}", connection.remote_id());
//         self.connections.lock().await.push(connection.clone());
//         Ok(connection)
//     }
// }

// impl IrohTransport {
//     pub async fn init(
//         config: &Config,
//         config_dir: &std::path::Path,
//     ) -> anyhow::Result<Option<IrohNode>> {
//         if !config.iroh.enabled {
//             log::info!("Iroh disabled in config — skipping");
//             return Ok(None);
//         }

//         let secret_key =
//             load_or_create_iroh_key(config_dir).map_err(|e| anyhow::anyhow!("{}", e))?;
//         let endpoint = if config.iroh.bind_port != 0 {
//             let addr: std::net::SocketAddr =
//                 format!("0.0.0.0:{}", config.iroh.bind_port).parse()?;
//             Endpoint::builder()
//                 .secret_key(secret_key)
//                 .bind_addr(addr)?
//                 .bind()
//                 .await?
//         } else {
//             Endpoint::builder().secret_key(secret_key).bind().await?
//         };

//         endpoint.set_alpns(vec![ALPN.to_vec()]);
//         log::info!("Iroh endpoint started — NodeID: {}", endpoint.id());

//         let connections = Arc::new(Mutex::new(Vec::new()));
//         let accept_task = IrohNode::spawn_accept_loop(endpoint.clone(), connections.clone());

//         Ok(Some(IrohNode {
//             endpoint,
//             connections,
//             accept_task,
//         }))
//     }
// }
use crate::{daemon::config::Config, utils::identity::load_or_create_iroh_key};
use anyhow::Result;
use iroh::endpoint::Connection;
use iroh::{Endpoint, EndpointId};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct IrohTransport;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatMessage {
    pub from: String,
    pub text: String,
    pub timestamp: u64,
}

pub struct IrohNode {
    pub endpoint: Endpoint,
    pub connections: Arc<Mutex<Vec<Connection>>>,
    pub chat_inbox: Arc<Mutex<VecDeque<ChatMessage>>>,
    #[allow(dead_code)]
    accept_task: tokio::task::JoinHandle<()>,
}

pub const ALPN: &[u8] = b"aerowan/0";

impl IrohNode {
    fn spawn_accept_loop(
        endpoint: Endpoint,
        connections: Arc<Mutex<Vec<Connection>>>,
        chat_inbox: Arc<Mutex<VecDeque<ChatMessage>>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                match endpoint.accept().await {
                    Some(incoming) => match incoming.await {
                        Ok(connection) => {
                            let peer_id = connection.remote_id().to_string();
                            log::info!("New inbound connection from: {}", peer_id);

                            Self::spawn_chat_reader(
                                connection.clone(),
                                peer_id,
                                chat_inbox.clone(),
                            );

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

    fn spawn_chat_reader(
        conn: Connection,
        peer_id: String,
        chat_inbox: Arc<Mutex<VecDeque<ChatMessage>>>,
    ) {
        tokio::spawn(async move {
            loop {
                match conn.accept_uni().await {
                    Ok(mut recv) => {
                        // Read 4-byte length prefix
                        let mut len_buf = [0u8; 4];
                        if recv.read_exact(&mut len_buf).await.is_err() {
                            break;
                        }
                        let len = u32::from_be_bytes(len_buf) as usize;

                        // Guard against malformed/huge messages
                        if len == 0 || len > 64 * 1024 {
                            log::warn!("Chat message from {} has invalid length {}", peer_id, len);
                            continue;
                        }

                        // Read message body
                        let mut body = vec![0u8; len];
                        if recv.read_exact(&mut body).await.is_err() {
                            break;
                        }

                        let text = match String::from_utf8(body) {
                            Ok(t) => t,
                            Err(_) => {
                                log::warn!("Non-UTF8 message from {}", peer_id);
                                continue;
                            }
                        };

                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();

                        chat_inbox.lock().await.push_back(ChatMessage {
                            from: peer_id.clone(),
                            text,
                            timestamp,
                        });
                    }
                    Err(e) => {
                        log::info!("Chat reader for {} closed: {}", peer_id, e);
                        break;
                    }
                }
            }
        });
    }

    pub async fn connect(
        &self,
        node_id: EndpointId,
    ) -> Result<Connection, Box<dyn std::error::Error>> {
        let connection = self.endpoint.connect(node_id, ALPN).await?;
        let peer_id = connection.remote_id().to_string();
        log::info!("Connected to peer {}", peer_id);

        Self::spawn_chat_reader(connection.clone(), peer_id, self.chat_inbox.clone());

        self.connections.lock().await.push(connection.clone());
        Ok(connection)
    }

    pub async fn send_message(
        &self,
        peer_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conns = self.connections.lock().await;

        let conn = conns
            .iter()
            .find(|c| c.remote_id().to_string() == peer_id)
            .ok_or_else(|| format!("no connection to peer {}", peer_id))?;

        let mut send = conn.open_uni().await?;
        let bytes = text.as_bytes();
        let len = (bytes.len() as u32).to_be_bytes();

        send.write_all(&len).await?;
        send.write_all(bytes).await?;
        send.finish()?;

        Ok(())
    }

    pub async fn drain_inbox(&self) -> Vec<ChatMessage> {
        self.chat_inbox.lock().await.drain(..).collect()
    }
}

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
        let chat_inbox = Arc::new(Mutex::new(VecDeque::new()));

        let accept_task =
            IrohNode::spawn_accept_loop(endpoint.clone(), connections.clone(), chat_inbox.clone());

        Ok(Some(IrohNode {
            endpoint,
            connections,
            chat_inbox,
            accept_task,
        }))
    }
}
