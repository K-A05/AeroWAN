use crate::utils::identity::load_api_key;
use axum::routing::get;
use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::IntoResponse,
};
use iroh::Endpoint;
use std::sync::Arc;

use crate::daemon::Config;
use std::path::Path;

pub struct AppState {
    pub iroh_endpoint: Option<Endpoint>,
}
#[derive(Clone)]
pub struct IamLayer {
    api_key: String,
}

impl IamLayer {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

pub async fn i_am_middleware(
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
    match &state.iroh_endpoint {
        Some(endpoint) => (StatusCode::OK, format!("NodeID: {}", endpoint.id())),
        None => (
            StatusCode::OK,
            "Running in Reticulum-only mode, iroh disabled".to_string(),
        ),
    }
}

pub async fn peers_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match &state.iroh_endpoint {
        Some(_endpoint) => (StatusCode::OK, "[]".to_string()),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "iroh disabled, peer list unavailable".to_string(),
        ),
    }
}

pub struct LANServer {
    handle: tokio::task::JoinHandle<()>,
}

impl LANServer {
    pub async fn start(
        config: &Config,
        config_dir: &Path,
        iroh_endpoint: Option<Endpoint>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let api_key = load_api_key(config_dir)?; // define in identity.rs
        let state = Arc::new(AppState { iroh_endpoint });

        let app = Router::new() // use the axum router to structure the Lan server API
            .route("/status", get(status_handler))
            .route("/peers", get(peers_handler))
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
