use crate::daemon::Config;
use crate::transport::iroh::IrohNode;
use crate::utils::identity::load_api_key;
use axum::routing::{get, post};
use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::IntoResponse,
};
use iroh::EndpointId;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;

pub struct AppState {
    pub iroh_node: Option<Arc<IrohNode>>, // reference to the Iroh node struct for use in the API.
}

#[derive(Deserialize)]
pub struct ConnectRequest {
    pub node_id: EndpointId,
}

#[derive(Deserialize)]
pub struct ChatSendRequest {
    pub node_id: String,
    pub message: String,
}

#[derive(Clone)]
pub struct IamLayer {
    // authentication layer for the API
    api_key: String,
}

impl IamLayer {
    pub fn new(api_key: String) -> Self {
        Self { api_key } // API key constructor
    }
}

pub async fn i_am_middleware(
    // checks to see if the incoming request has a valid API  key,
    State(api_key): State<String>,
    req: Request,
    next: Next,
) -> Result<impl IntoResponse, StatusCode> {
    let auth = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match auth {
        Some(key) if key == api_key => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

pub async fn status_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match &state.iroh_node {
        // returns the iroh node id for the application, if enabled, else a plain text message.
        Some(node) => (StatusCode::OK, format!("NodeID: {}", node.endpoint.id())),
        None => (
            StatusCode::OK,
            "Running in Reticulum-only mode, iroh disabled".to_string(),
        ),
    }
}

pub async fn peers_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match &state.iroh_node {
        // iterated over the live connection list and returns a nodeID as a JSON string, this is polled by the TUI to keep the peers panel up to date.
        Some(node) => {
            let connections = node.connections.lock().await;
            let peer_ids: Vec<String> = connections
                .iter()
                .map(|conn| conn.remote_id().to_string())
                .collect();
            match serde_json::to_string(&peer_ids) {
                Ok(json) => (StatusCode::OK, json),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to serialize peers: {}", e),
                ),
            }
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "iroh disabled, peer list unavailable".to_string(),
        ),
    }
}

pub async fn connect_handler(
    // dials a remote peer by their node ID, a successful connection is written to the shared connection list and a chat reader is spawned for the connection.
    State(state): State<Arc<AppState>>,
    axum::Json(body): axum::Json<ConnectRequest>,
) -> impl IntoResponse {
    match &state.iroh_node {
        Some(node) => match node.connect(body.node_id).await {
            Ok(conn) => (
                StatusCode::OK,
                format!("Connected to: {}", conn.remote_id()),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Connection failed: {}", e),
            ),
        },
        None => (StatusCode::SERVICE_UNAVAILABLE, "iroh disabled".to_string()),
    }
}

pub async fn chat_send_handler(
    // creates a unidirectional QUIC stream to the target peer and writed the length pre-fixed message
    State(state): State<Arc<AppState>>,
    axum::Json(body): axum::Json<ChatSendRequest>,
) -> impl IntoResponse {
    match &state.iroh_node {
        Some(node) => match node.send_message(&body.node_id, &body.message).await {
            Ok(()) => (StatusCode::OK, "message sent".to_string()),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to send message: {}", e),
            ),
        },
        None => (StatusCode::SERVICE_UNAVAILABLE, "iroh disabled".to_string()),
    }
}

pub async fn chat_messages_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // drains all the messages from the inbox and returns them as a JSON array.
    match &state.iroh_node {
        Some(node) => {
            let messages = node.drain_inbox().await;
            match serde_json::to_string(&messages) {
                Ok(json) => (StatusCode::OK, json),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to serialize messages: {}", e),
                ),
            }
        }
        None => (StatusCode::SERVICE_UNAVAILABLE, "iroh disabled".to_string()),
    }
}

pub struct LANServer {
    handle: tokio::task::JoinHandle<()>,
}

impl LANServer {
    pub async fn start(
        config: &Config,
        config_dir: &Path,
        iroh_node: Option<Arc<IrohNode>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let api_key = load_api_key(config_dir)?;
        let state = Arc::new(AppState { iroh_node });
        let app = Router::new()
            .route("/status", get(status_handler)) // returns node ID/ mode of operation
            .route("/peers", get(peers_handler)) // returns peers.
            .route("/connect", post(connect_handler)) // connection endpoint to connect with another peer.
            .route("/chat/send", post(chat_send_handler)) // endpoint for sending messages.
            .route("/chat/messages", get(chat_messages_handler)) // handle messages in the queue.
            .layer(axum::middleware::from_fn_with_state(
                api_key.clone(),
                i_am_middleware,
            ))
            .with_state(state);
        let addr = format!("0.0.0.0:{}", config.api.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        log::info!("API server listening on {}", addr);
        Ok(Self { handle })
    }
}
