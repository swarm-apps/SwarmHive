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
    /// Initialize swarmhive.toml in the current directory.
    Init,
    /// Validate a release without uploading (dry-run).
    Verify {
        #[command(subcommand)]
        command: VerifyCommand,
    },
    /// Upload artifacts and create (and by default publish) a release.
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
        /// Read release notes from a file.
        #[arg(long)]
        notes_file: Option<std::path::PathBuf>,
    },
    /// Update a release's mutable fields.
    Update {
        #[arg(long)]
        app: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        android_version_code: Option<i64>,
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

#[tokio::main]
async fn main() {
    init_tracing();
    let cli = Cli::parse();
    let output = cli.output;
    if let Err(err) = dispatch(cli.command, output).await {
        render_error(&err, output);
        std::process::exit(1);
    }
}

async fn dispatch(command: Command, output: OutputFormat) -> anyhow::Result<()> {
    match command {
        Command::Init => todo!("init: scaffold swarmhive.toml"),
        Command::Verify { command } => match command {
            VerifyCommand::Tauri(args) => commands::verify::tauri(args).await?,
            VerifyCommand::Android(args) => commands::verify::android(args).await?,
        },
        Command::Publish { command } => match command {
            PublishCommand::Tauri(args) => commands::publish::tauri(args).await?,
            PublishCommand::Android(args) => commands::publish::android(args).await?,
        },
        Command::Storage { command } => commands::storage::run(command, output).await?,
        Command::Mail { command } => commands::mail::run(command, output).await?,
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
                notes_file,
            } => {
                commands::releases::create(&app, version, android_version_code, notes_file, output)
                    .await?
            }
            ReleasesCommand::Update {
                app,
                version,
                android_version_code,
                notes_file,
            } => {
                commands::releases::update(&app, &version, android_version_code, notes_file, output)
                    .await?
            }
            ReleasesCommand::Publish { app, version } => {
                commands::releases::publish(&app, &version, output).await?
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

/// 按 `--output` 渲染顶层错误:json → problem+json(API 错误)或 `{"error":...}`(本地
/// 错误)到 stderr;table → 人话。配合 `process::exit(1)` 给 skill / AI 稳定契约。
fn render_error(err: &anyhow::Error, output: OutputFormat) {
    match output {
        OutputFormat::Json => {
            let body = match err.downcast_ref::<commands::client::ApiProblem>() {
                Some(p) => p.problem.clone(),
                None => serde_json::json!({ "error": err.to_string() }),
            };
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&body).unwrap_or_else(|_| body.to_string())
            );
        }
        OutputFormat::Table => eprintln!("error: {err:#}"),
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,swarmhive_cli=debug"));
    fmt().with_env_filter(filter).init();
}
