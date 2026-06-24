use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

use crate::commands::client::OutputFormat;

mod auth;
mod commands;
mod config;
mod credentials;

#[derive(Debug, Parser)]
#[command(
    name = "swarmhive",
    version,
    about = "SwarmHive CLI — local + CI/CD release entrypoint"
    // 不 propagate_version：顶层已有 `--version` + `swarmhive version` 子命令；传播到
    // 子命令会与 releases / artifacts variant 的 `version`(release 版本号)字段撞名,
    // 触发 clap "Argument names must be unique" panic。
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// Output format for list commands.
    #[arg(long, value_enum, default_value_t = OutputFormat::Table, global = true)]
    output: OutputFormat,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize swarmhive.toml in the current directory (interactive, or
    /// flag-driven with --yes / non-TTY for AI / CI).
    Init(commands::init::InitArgs),
    /// Validate a release without uploading (dry-run).
    Verify {
        #[command(subcommand)]
        command: VerifyCommand,
    },
    /// Upload artifacts to a draft release (use --finalize to publish in one step).
    Publish {
        #[command(subcommand)]
        command: PublishCommand,
    },
    /// Configure storage backends.
    Storage {
        #[command(subcommand)]
        command: commands::storage::StorageCommand,
    },
    /// Manage mail providers / templates / logs.
    Mail {
        #[command(subcommand)]
        command: commands::mail::MailCommand,
    },
    /// Manage notifications (webhook endpoints / subscriptions / deliveries).
    Notifications {
        #[command(subcommand)]
        command: commands::notifications::NotificationsCommand,
    },
    /// Print version information.
    Version,
    /// Authenticate against a SwarmHive server via the browser (device flow)
    /// and store a PAT locally.
    Login {
        /// Server URL (defaults to `http://localhost:3030`).
        server: Option<String>,
    },
    /// Revoke the locally stored PAT on the server and remove the local file.
    Logout,
    /// Manage apps.
    Apps {
        #[command(subcommand)]
        command: AppsCommand,
    },
    /// Manage channels (list / create / set-default / promote / rollback).
    Channels {
        #[command(subcommand)]
        command: ChannelsCommand,
    },
    /// Manage releases.
    Releases {
        #[command(subcommand)]
        command: ReleasesCommand,
    },
    /// Inspect artifacts.
    Artifacts {
        #[command(subcommand)]
        command: ArtifactsCommand,
    },
    /// Manage API tokens / PATs (mint scoped tokens for CI).
    Tokens {
        #[command(subcommand)]
        command: TokensCommand,
    },
    /// Query telemetry (overview / summary / adoption / funnel / distribution).
    Telemetry {
        #[command(subcommand)]
        command: TelemetryCommand,
    },
}

#[derive(Debug, Subcommand)]
enum TelemetryCommand {
    /// Global at-a-glance overview across all apps.
    Overview {
        #[arg(long, default_value_t = 30)]
        days: u32,
    },
    /// Per-app metric cards (today's active devices, downloads, latest version).
    Summary {
        #[arg(long)]
        app: String,
        #[arg(long, default_value_t = 30)]
        days: u32,
    },
    /// Per-app version adoption (daily unique devices; version=(total) is the daily total).
    Adoption {
        #[arg(long)]
        app: String,
        #[arg(long, default_value_t = 30)]
        days: u32,
    },
    /// Per-app update funnel (occurrence-based, not device-deduplicated).
    Funnel {
        #[arg(long)]
        app: String,
        #[arg(long, default_value_t = 30)]
        days: u32,
    },
    /// Per-app update-check distribution by a dimension.
    Distribution {
        #[arg(long)]
        app: String,
        #[arg(long, default_value_t = 30)]
        days: u32,
        /// Dimension: platform | arch | version | channel.
        #[arg(long, default_value = "platform")]
        dim: String,
    },
}

#[derive(Debug, Subcommand)]
enum VerifyCommand {
    /// Verify a Tauri desktop release.
    Tauri(commands::verify::TauriArgs),
    /// Verify a React Native Android release.
    Android(commands::verify::AndroidArgs),
}

#[derive(Debug, Subcommand)]
enum PublishCommand {
    /// Publish a Tauri desktop release.
    Tauri(commands::publish::TauriArgs),
    /// Publish a React Native Android release.
    Android(commands::publish::AndroidArgs),
}

#[derive(Debug, Subcommand)]
enum AppsCommand {
    /// List apps on the server.
    List,
    /// Show one app's detail.
    Get {
        #[arg(long)]
        app: String,
    },
    /// Create an app.
    Create {
        #[arg(long)]
        slug: String,
        #[arg(long)]
        display_name: String,
        /// Comma-separated platforms, e.g. tauri-desktop,react-native-android
        #[arg(long, value_delimiter = ',', required = true)]
        platforms: Vec<String>,
    },
    /// Update an app's mutable fields (slug is immutable).
    Update {
        #[arg(long)]
        app: String,
        #[arg(long)]
        display_name: Option<String>,
        /// Replace the platform set, comma-separated.
        #[arg(long, value_delimiter = ',')]
        platforms: Option<Vec<String>>,
    },
    /// Delete an app (requires --yes; fails if it still has releases).
    Delete {
        #[arg(long)]
        app: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum TokensCommand {
    /// List your tokens (PAT + API).
    List,
    /// Create a token. PAT inherits your permissions; API needs --permissions or --preset.
    Create {
        #[arg(long)]
        name: String,
        /// Token kind: pat | api.
        #[arg(long, default_value = "api")]
        kind: String,
        /// Comma-separated permissions for an API token, e.g. release:publish,artifact:upload.
        #[arg(long, value_delimiter = ',')]
        permissions: Option<Vec<String>>,
        /// Permission preset for an API token. `ci-publish` expands to the full publish
        /// permission set (incl. release:update). Mutually exclusive with --permissions.
        #[arg(long)]
        preset: Option<String>,
    },
    /// Revoke a token by id (requires --yes).
    Delete {
        #[arg(long)]
        id: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ChannelsCommand {
    /// List an app's channels.
    List {
        #[arg(long)]
        app: String,
    },
    /// Create a channel.
    Create {
        #[arg(long)]
        app: String,
        #[arg(long)]
        name: String,
    },
    /// Mark a channel as the app's default.
    SetDefault {
        #[arg(long)]
        app: String,
        #[arg(long)]
        name: String,
    },
    /// Point a channel at a published release version.
    Promote {
        #[arg(long)]
        app: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        version: String,
    },
    /// Roll a channel back to a previous release (defaults to the previous one).
    Rollback {
        #[arg(long)]
        app: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        to_version: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ReleasesCommand {
    /// List releases of an app.
    List {
        #[arg(long)]
        app: String,
    },
    /// Show one release's detail.
    Get {
        #[arg(long)]
        app: String,
        #[arg(long)]
        version: String,
    },
    /// Create a draft release (no upload).
    Create {
        #[arg(long)]
        app: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        android_version_code: Option<i64>,
        /// RN Android force-update floor (versionCode); clients below it are forced to update.
        #[arg(long)]
        android_min_version_code: Option<i64>,
        /// Read release notes from a file.
        #[arg(long)]
        notes_file: Option<std::path::PathBuf>,
    },
    /// Update a release's mutable fields (incl. rollout and force-update policy).
    Update {
        #[arg(long)]
        app: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        android_version_code: Option<i64>,
        /// Gray rollout percent 1-100; omit to leave unchanged, 100 to disable gray.
        #[arg(long, value_parser = clap::value_parser!(i16).range(1..=100))]
        rollout_percent: Option<i16>,
        /// Tauri force-update floor (semver); omit to leave unchanged, 0.0.0 to remove.
        #[arg(long)]
        min_version: Option<String>,
        /// RN Android force-update floor (versionCode); omit to leave unchanged.
        #[arg(long)]
        android_min_version_code: Option<i64>,
        #[arg(long)]
        notes_file: Option<std::path::PathBuf>,
    },
    /// Publish an existing draft release.
    Publish {
        #[arg(long)]
        app: String,
        #[arg(long)]
        version: String,
    },
    /// Finalize (publish) a draft release after its artifacts are uploaded. Idempotent.
    Finalize {
        #[arg(long)]
        app: String,
        #[arg(long)]
        version: String,
    },
    /// Yank a published release (requires --yes).
    Yank {
        #[arg(long)]
        app: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ArtifactsCommand {
    /// List artifacts of a release.
    List {
        #[arg(long)]
        app: String,
        #[arg(long)]
        version: String,
    },
}

/// 故障分类:决定退出码 + GitHub Actions annotation 级别。
/// - 永久(`exit 2`):权限 / 配置 / 校验 / 冲突——重试无意义,CI 应立即失败。
/// - 可重试(`exit 1`):服务端暂时不可用(5xx/408/429)、网络超时——CI 可重试。
#[derive(Clone, Copy)]
enum ErrorClass {
    Permanent,
    Retryable,
}

impl ErrorClass {
    fn exit_code(self) -> i32 {
        match self {
            ErrorClass::Permanent => 2,
            ErrorClass::Retryable => 1,
        }
    }
}

/// 把顶层错误分类成永久 / 可重试。`ApiProblem` 按状态码(其 `retryable()`);reqwest
/// 网络层错误(超时 / 连接 / 发送失败)视作可重试;其余(本地配置 / 校验 / 未知)视作
/// 永久,不盲目重试。
fn classify_error(err: &anyhow::Error) -> ErrorClass {
    if let Some(p) = err.downcast_ref::<commands::client::ApiProblem>() {
        return if p.retryable() {
            ErrorClass::Retryable
        } else {
            ErrorClass::Permanent
        };
    }
    if err.chain().any(|cause| {
        cause
            .downcast_ref::<reqwest::Error>()
            .is_some_and(|e| e.is_timeout() || e.is_connect() || e.is_request())
    }) {
        return ErrorClass::Retryable;
    }
    ErrorClass::Permanent
}

#[tokio::main]
async fn main() {
    init_tracing();
    let cli = Cli::parse();
    let output = cli.output;
    if let Err(err) = dispatch(cli.command, output).await {
        let class = classify_error(&err);
        render_error(&err, output, class);
        std::process::exit(class.exit_code());
    }
}

async fn dispatch(command: Command, output: OutputFormat) -> anyhow::Result<()> {
    match command {
        Command::Init(args) => commands::init::run(args, output)?,
        Command::Verify { command } => match command {
            VerifyCommand::Tauri(args) => commands::verify::tauri(args, output).await?,
            VerifyCommand::Android(args) => commands::verify::android(args, output).await?,
        },
        Command::Publish { command } => match command {
            PublishCommand::Tauri(args) => commands::publish::tauri(args, output).await?,
            PublishCommand::Android(args) => commands::publish::android(args, output).await?,
        },
        Command::Storage { command } => commands::storage::run(command, output).await?,
        Command::Mail { command } => commands::mail::run(command, output).await?,
        Command::Notifications { command } => commands::notifications::run(command, output).await?,
        Command::Version => {
            println!("swarmhive-cli {}", env!("CARGO_PKG_VERSION"));
        }
        Command::Login { server } => {
            commands::login::run(server).await?;
        }
        Command::Logout => {
            commands::logout::run().await?;
        }
        Command::Apps { command } => match command {
            AppsCommand::List => commands::apps::list(output).await?,
            AppsCommand::Get { app } => commands::apps::get(&app, output).await?,
            AppsCommand::Create {
                slug,
                display_name,
                platforms,
            } => commands::apps::create(slug, display_name, platforms, output).await?,
            AppsCommand::Update {
                app,
                display_name,
                platforms,
            } => commands::apps::update(&app, display_name, platforms, output).await?,
            AppsCommand::Delete { app, yes } => commands::apps::delete(&app, yes, output).await?,
        },
        Command::Tokens { command } => match command {
            TokensCommand::List => commands::tokens::list(output).await?,
            TokensCommand::Create {
                name,
                kind,
                permissions,
                preset,
            } => commands::tokens::create(name, kind, permissions, preset, output).await?,
            TokensCommand::Delete { id, yes } => commands::tokens::delete(&id, yes, output).await?,
        },
        Command::Telemetry { command } => match command {
            TelemetryCommand::Overview { days } => {
                commands::telemetry::overview(days, output).await?
            }
            TelemetryCommand::Summary { app, days } => {
                commands::telemetry::summary(&app, days, output).await?
            }
            TelemetryCommand::Adoption { app, days } => {
                commands::telemetry::adoption(&app, days, output).await?
            }
            TelemetryCommand::Funnel { app, days } => {
                commands::telemetry::funnel(&app, days, output).await?
            }
            TelemetryCommand::Distribution { app, days, dim } => {
                commands::telemetry::distribution(&app, days, &dim, output).await?
            }
        },
        Command::Channels { command } => match command {
            ChannelsCommand::List { app } => commands::channels::list(&app, output).await?,
            ChannelsCommand::Create { app, name } => {
                commands::channels::create(&app, name, output).await?
            }
            ChannelsCommand::SetDefault { app, name } => {
                commands::channels::set_default(&app, &name, output).await?
            }
            ChannelsCommand::Promote { app, name, version } => {
                commands::channels::promote(&app, &name, version, output).await?
            }
            ChannelsCommand::Rollback {
                app,
                name,
                to_version,
            } => commands::channels::rollback(&app, &name, to_version, output).await?,
        },
        Command::Releases { command } => match command {
            ReleasesCommand::List { app } => commands::releases::list(&app, output).await?,
            ReleasesCommand::Get { app, version } => {
                commands::releases::get(&app, &version, output).await?
            }
            ReleasesCommand::Create {
                app,
                version,
                android_version_code,
                android_min_version_code,
                notes_file,
            } => {
                commands::releases::create(
                    &app,
                    version,
                    android_version_code,
                    android_min_version_code,
                    notes_file,
                    output,
                )
                .await?
            }
            ReleasesCommand::Update {
                app,
                version,
                android_version_code,
                rollout_percent,
                min_version,
                android_min_version_code,
                notes_file,
            } => {
                commands::releases::update(
                    &app,
                    &version,
                    android_version_code,
                    rollout_percent,
                    min_version,
                    android_min_version_code,
                    notes_file,
                    output,
                )
                .await?
            }
            ReleasesCommand::Publish { app, version } => {
                commands::releases::publish(&app, &version, output).await?
            }
            ReleasesCommand::Finalize { app, version } => {
                commands::releases::finalize(&app, &version, output).await?
            }
            ReleasesCommand::Yank { app, version, yes } => {
                commands::releases::yank(&app, &version, yes, output).await?
            }
        },
        Command::Artifacts { command } => match command {
            ArtifactsCommand::List { app, version } => {
                commands::artifacts::list(&app, &version, output).await?
            }
        },
    }
    Ok(())
}

/// 按 `--output` 渲染顶层错误:json → problem+json(API 错误)或 `{"error":...,"retryable":...}`
/// (本地错误)到 stderr;table → 人话 + 一行 remediation hint(若有)。在 GitHub Actions 里
/// (`GITHUB_ACTIONS=true`)额外发一行 annotation:永久错误 `::error::`、可重试 `::warning::`,
/// 让 action step 红/绿语义与退出码一致。配合分层退出码给 skill / AI / CI 稳定契约。
fn render_error(err: &anyhow::Error, output: OutputFormat, class: ErrorClass) {
    let problem = err.downcast_ref::<commands::client::ApiProblem>();
    let hint = problem.and_then(|p| p.remediation_hint());
    let retryable = matches!(class, ErrorClass::Retryable);

    // GitHub Actions annotation(单行;annotation 不能含裸换行,会被截断)。
    if std::env::var_os("GITHUB_ACTIONS").is_some_and(|v| v == "true") {
        let level = if retryable { "warning" } else { "error" };
        let mut msg = err.to_string();
        if let Some(hint) = hint {
            msg = format!("{msg} — {hint}");
        }
        println!("::{level}::{}", msg.replace('\n', " "));
    }

    match output {
        OutputFormat::Json => {
            let body = match problem {
                Some(p) => p.problem.clone(),
                None => serde_json::json!({ "error": err.to_string(), "retryable": retryable }),
            };
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&body).unwrap_or_else(|_| body.to_string())
            );
        }
        OutputFormat::Table => {
            eprintln!("error: {err:#}");
            if let Some(hint) = hint {
                eprintln!("hint: {hint}");
            }
        }
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,swarmhive_cli=debug"));
    fmt().with_env_filter(filter).init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use commands::client::ApiProblem;

    fn api_err(status: u16) -> anyhow::Error {
        anyhow::Error::new(ApiProblem {
            status,
            problem: serde_json::json!({ "status": status }),
        })
    }

    #[test]
    fn permission_errors_are_permanent_exit_2() {
        for status in [401u16, 403, 409, 422] {
            assert_eq!(classify_error(&api_err(status)).exit_code(), 2, "{status}");
        }
    }

    #[test]
    fn server_unavailable_is_retryable_exit_1() {
        for status in [408u16, 429, 500, 503] {
            assert_eq!(classify_error(&api_err(status)).exit_code(), 1, "{status}");
        }
    }

    #[test]
    fn local_errors_default_permanent_exit_2() {
        // 非 ApiProblem、非网络错误(本地配置 / 校验)→ 永久,不盲目重试。
        let err = anyhow::anyhow!("missing app slug: pass --app");
        assert_eq!(classify_error(&err).exit_code(), 2);
    }
}
