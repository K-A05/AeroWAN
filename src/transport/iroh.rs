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

pub const ALPN: &[u8] = b"aerowan/0"; // the Application Level Protocol Negotiation

impl IrohNode {
    fn spawn_accept_loop(
        //  method to create the accept loop for the chat
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
                                //spawns a dedicated reader for the connection so the incoming messages are processed concurrently without blocking the accept loop.
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
                        log::info!("Iroh endpoint closed, stopping accept loop"); // endpoint.accept() will return NONE when the endpoint has been terminated.
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
            // for each connection there is a dedicated reader.
            loop {
                match conn.accept_uni().await {
                    Ok(mut recv) => {
                        // Every message is prefixed with it's length, as a 4 byte big-endian u32, which indicates how many bytes the contents are.
                        let mut len_buf = [0u8; 4];
                        if recv.read_exact(&mut len_buf).await.is_err() {
                            // read_exact will fail/return an error if the header is incomplete or if the stream was closed, treating it as a disconnect.
                            break;
                        }
                        let len = u32::from_be_bytes(len_buf) as usize;

                        // 64 KiB allocated for the messages should be sufficient, anything above that limit/an empty message will be rejected(defensive programming).
                        if len == 0 || len > 64 * 1024 {
                            log::warn!("Chat message from {} has invalid length {}", peer_id, len);
                            continue;
                        }

                        // Read body
                        let mut body = vec![0u8; len]; // allocate the exact number of bytes as the length of the message.
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
                            // push the message into the shared inbox /chat/messges.
                            from: peer_id.clone(), // the endpoint will drain this every time the TUI polls.
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
        let connection = self.endpoint.connect(node_id, ALPN).await?; // connect to the peer node over the existing node ID, using the peer node addressa with the ALPN.
        let peer_id = connection.remote_id().to_string();
        log::info!("Connected to peer {}", peer_id);

        Self::spawn_chat_reader(connection.clone(), peer_id, self.chat_inbox.clone()); // outbound connections also need a chat reader, witohut which only the side receiving the connection would be able to receive the messages.

        self.connections.lock().await.push(connection.clone());
        Ok(connection)
    }

    pub async fn send_message(
        &self,
        peer_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conns = self.connections.lock().await;

        let conn = conns // find the active connection that matches the target peer id.
            .iter()
            .find(|c| c.remote_id().to_string() == peer_id)
            .ok_or_else(|| format!("no connection to peer {}", peer_id))?;

        let mut send = conn.open_uni().await?; // open a new unidrectional stream for the message, each new message gets its own stream.
        let bytes = text.as_bytes();
        let len = (bytes.len() as u32).to_be_bytes();

        send.write_all(&len).await?; // write the 4 byte prefix followed by the body of the message.
        send.write_all(bytes).await?;
        send.finish()?;

        Ok(())
    }

    pub async fn drain_inbox(&self) -> Vec<ChatMessage> {
        self.chat_inbox.lock().await.drain(..).collect() // removes all pending messages from the inbox.
    }
}

impl IrohTransport {
    pub async fn init(
        // Bring up the Iroh node for transportation
        config: &Config,
        config_dir: &std::path::Path,
    ) -> anyhow::Result<Option<IrohNode>> {
        if !config.iroh.enabled {
            // check to see if iroh is enabled.
            log::info!("Iroh disabled in config — skipping");
            return Ok(None);
        }

        let secret_key =
            load_or_create_iroh_key(config_dir).map_err(|e| anyhow::anyhow!("{}", e))?; // load or create an Ed25519 key, this key persists and is stable across restarts, meaning that the node ID will stay the same.

        let endpoint = if config.iroh.bind_port != 0 {
            // build out the endpoint with all the supplied arguments, lack of an argument will cause the endpoint to use default values.
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

        endpoint.set_alpns(vec![ALPN.to_vec()]); // set the ALPN, any application not using the same ALPN will not be able to connect to the application.
        log::info!("Iroh endpoint started — NodeID: {}", endpoint.id());

        let connections = Arc::new(Mutex::new(Vec::new())); // a shared list of connection, that is read by and written to by the accept_loop and connect(),
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
