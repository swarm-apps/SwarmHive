use std::net::SocketAddr;

use anyhow::Context;
use sea_orm::{DatabaseConnection, EntityTrait, PaginatorTrait};
use swarmhive_entity::user;
use swarmhive_server::auth::service as auth_service;
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

    maybe_issue_setup_token(&conn)
        .await
        .context("setup-token issuance failed")?;

    let state = AppState::new(conn, cfg.clone());
    let app = build_router(state);

    let addr: SocketAddr = cfg.server.bind.parse().context("invalid server.bind")?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "swarmhive-server listening");

    // ConnectInfo<SocketAddr> is required by tower-governor's
    // SmartIpKeyExtractor when there's no X-Forwarded-For header.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

/// On every startup, if the `user` table is empty issue a fresh one-shot
/// setup token and print it to stdout (single-source operator log). Existing
/// installations are no-ops.
async fn maybe_issue_setup_token(db: &DatabaseConnection) -> anyhow::Result<()> {
    let user_count = user::Entity::find().count(db).await?;
    if user_count > 0 {
        return Ok(());
    }
    let token = auth_service::issue_setup_token(db).await?;
    print_setup_banner(&token);
    Ok(())
}

fn print_setup_banner(token: &str) {
    println!();
    println!("════════════════════════════════════════════════════════════════");
    println!("  SwarmHive first-run setup");
    println!();
    println!("  Open the Admin SPA and complete /setup with this token:");
    println!();
    println!("      {token}");
    println!();
    println!("  The token is one-shot and expires in 1 hour.");
    println!("════════════════════════════════════════════════════════════════");
    println!();
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
