use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter};
use swarmhive_entity::{mail_provider, user};
use swarmhive_server::auth::bootstrap::BOOTSTRAP_OWNER_EMAIL_ENV;
use swarmhive_server::crypto::{SECRET_KEY_ENV, SecretKey};
use swarmhive_server::mail::{
    MailerHandle, console::ConsoleMailer, seed as mail_seed, smtp::SmtpMailer,
};
use swarmhive_server::{build_router, config, db, services::seed, state::AppState};
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = config::load().context("failed to load configuration")?;
    init_tracing(&cfg.telemetry.log_level);

    // Fail-fast: any feature that stores external secrets (mail provider
    // password, future OAuth client_secret, ...) needs the same symmetric
    // key. Resolved from `SWARMHIVE_SECRET_KEY` env first, then from
    // `[secret] key` in `config/local.toml` (gitignored). Missing in both
    // → operator must generate one with `openssl rand -base64 32`.
    let secret_key = SecretKey::from_env_or_config(cfg.secret.key.as_deref()).with_context(
        || {
            format!(
                "{SECRET_KEY_ENV} is required. Either `export {SECRET_KEY_ENV}=$(openssl rand -base64 32)`,\n\
                 or add the following to config/local.toml (gitignored):\n\n\
                 [secret]\n\
                 key = \"<openssl rand -base64 32>\"\n"
            )
        },
    )?;

    let conn = db::connect(&cfg.database)
        .await
        .context("failed to connect to database")?;

    if cfg.database.auto_sync {
        info!("auto_sync enabled — running schema-sync");
        db::sync_schema(&conn).await.context("schema-sync failed")?;
    }

    seed::run(&conn).await.context("seed failed")?;

    // Mail bootstrap: seed default templates always; mailpit provider only
    // in dev when the table is empty. Selecting the active provider happens
    // after AppState is constructed (the mailer slot needs the shared
    // template engine that lives on state).
    mail_seed::seed_default_templates(&conn)
        .await
        .context("mail template seed failed")?;
    if cfg.mail.seed_mailpit_in_dev {
        mail_seed::seed_mailpit_provider(&conn)
            .await
            .context("mailpit provider seed failed")?;
    }

    maybe_print_bootstrap_banner(&conn).await?;

    let state = AppState::new(conn.clone(), cfg.clone(), secret_key.clone());
    wire_active_mailer(&state, &conn, &secret_key).await;
    // Wire the active object-storage backend (if any). Missing/failed build
    // leaves the slot empty — upload endpoints return 409 until configured.
    swarmhive_server::storage::refresh(&state).await;
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

/// Inspect `mail_provider` for an `active=true` row and swap the AppState
/// mailer slot accordingly. On any error (DB hiccup, decrypt failure,
/// invalid SMTP host) we leave the default `ConsoleMailer` in place so the
/// server still boots and the Admin SPA can fix the config.
async fn wire_active_mailer(state: &AppState, db: &DatabaseConnection, secret_key: &SecretKey) {
    let active = match mail_provider::Entity::find()
        .filter(mail_provider::Column::Active.eq(true))
        .one(db)
        .await
    {
        Ok(row) => row,
        Err(err) => {
            warn!(
                ?err,
                "mail_provider lookup failed; staying on ConsoleMailer"
            );
            return;
        }
    };
    let Some(row) = active else {
        info!("no active mail provider; ConsoleMailer fallback in effect");
        return;
    };

    let provider_id = row.id;
    match SmtpMailer::from_provider(db.clone(), state.mail_templates.clone(), row, secret_key) {
        Ok(smtp) => {
            *state.mailer.write().expect("mailer slot poisoned") =
                MailerHandle::new(Arc::new(smtp));
            info!(%provider_id, "smtp mailer wired");
        }
        Err(err) => {
            warn!(%provider_id, ?err, "failed to build SmtpMailer; staying on ConsoleMailer");
            // Defensive re-install of console to make sure the slot is
            // sound (cheap, no-op if already there).
            let console = ConsoleMailer::new(db.clone(), state.mail_templates.clone());
            *state.mailer.write().expect("mailer slot poisoned") =
                MailerHandle::new(Arc::new(console));
        }
    }
}

/// Print a one-time banner pointing the operator at `/setup` whenever the
/// deployment still needs its first Owner. Mentions the env-pinned email if
/// configured so it's discoverable in logs.
async fn maybe_print_bootstrap_banner(db: &DatabaseConnection) -> anyhow::Result<()> {
    let user_count = user::Entity::find().count(db).await?;
    if user_count > 0 {
        return Ok(());
    }
    let locked = std::env::var(BOOTSTRAP_OWNER_EMAIL_ENV)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    println!();
    println!("════════════════════════════════════════════════════════════════");
    println!("  SwarmHive first-run setup");
    println!();
    println!("  Open the Admin SPA in a browser and complete /setup to");
    println!("  create the initial Owner account (email + password).");
    if let Some(email) = locked {
        println!();
        println!("  {BOOTSTRAP_OWNER_EMAIL_ENV} is set; only this email may claim");
        println!("  the Owner role:");
        println!();
        println!("      {email}");
    } else {
        println!();
        println!("  Tip: set {BOOTSTRAP_OWNER_EMAIL_ENV} before exposing this");
        println!("  server to the network to prevent another visitor from");
        println!("  claiming the Owner role first.");
    }
    println!("════════════════════════════════════════════════════════════════");
    println!();
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
