use std::net::SocketAddr;

use anyhow::Context;
use swarmhive_server::{build_router, config, db, services::seed, state::AppState};
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = config::load().context("failed to load configuration")?;
    init_tracing(&cfg.telemetry.log_level);

    let conn = db::connect(&cfg.database)
        .await
        .context("failed to connect to database")?;

    if cfg.database.auto_sync {
        info!("auto_sync enabled — running schema-sync");
        db::sync_schema(&conn).await.context("schema-sync failed")?;
    }

    seed::run(&conn).await.context("seed failed")?;

    let state = AppState::new(conn, cfg.clone());
    let app = build_router(state);

    let addr: SocketAddr = cfg.server.bind.parse().context("invalid server.bind")?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "swarmhive-server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn init_tracing(filter: &str) {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter));
    fmt().with_env_filter(env_filter).init();
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
