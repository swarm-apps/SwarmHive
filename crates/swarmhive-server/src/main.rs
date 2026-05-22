use std::net::SocketAddr;

use axum::{response::Json, routing::get, Router};
use serde_json::json;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let app = router();

    let addr: SocketAddr = "0.0.0.0:3030".parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "swarmhive-server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn router() -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/api/v1/version", get(version))
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn version() -> Json<serde_json::Value> {
    Json(json!({
        "name": "swarmhive-server",
        "version": env!("CARGO_PKG_VERSION"),
        "core": swarmhive_core::VERSION,
    }))
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,swarmhive_server=debug,swarmhive_core=debug"));
    fmt().with_env_filter(filter).init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("shutdown signal received");
}
