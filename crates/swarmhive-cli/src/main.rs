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
    about = "SwarmHive CLI — local + CI/CD release entrypoint",
    propagate_version = true
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
    /// Promote a release to another channel (e.g. beta -> stable).
    Promote,
    /// Roll back a channel to its previous release.
    Rollback,
    /// Print version information.
    Version,
    /// Authenticate against a SwarmHive server and store a PAT locally.
    Login {
        /// Server URL (defaults to `http://localhost:3030`).
        server: Option<String>,
        /// Email to log in as. If omitted, prompts on stdin.
        #[arg(long)]
        email: Option<String>,
    },
    /// Revoke the locally stored PAT on the server and remove the local file.
    Logout,
    /// Inspect apps.
    Apps {
        #[command(subcommand)]
        command: AppsCommand,
    },
    /// Inspect releases.
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
}

#[derive(Debug, Subcommand)]
enum ReleasesCommand {
    /// List releases of an app.
    List {
        #[arg(long)]
        app: String,
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
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cli = Cli::parse();
    match cli.command {
        Command::Init => todo!("init: scaffold swarmhive.toml"),
        Command::Verify { command } => match command {
            VerifyCommand::Tauri(args) => commands::verify::tauri(args).await?,
            VerifyCommand::Android(args) => commands::verify::android(args).await?,
        },
        Command::Publish { command } => match command {
            PublishCommand::Tauri(args) => commands::publish::tauri(args).await?,
            PublishCommand::Android(args) => commands::publish::android(args).await?,
        },
        Command::Storage { command } => commands::storage::run(command).await?,
        Command::Promote => todo!("promote: channel promotion"),
        Command::Rollback => todo!("rollback: revert channel"),
        Command::Version => {
            println!("swarmhive-cli {}", env!("CARGO_PKG_VERSION"));
        }
        Command::Login { server, email } => {
            commands::login::run(server, email).await?;
        }
        Command::Logout => {
            commands::logout::run().await?;
        }
        Command::Apps { command } => match command {
            AppsCommand::List => commands::apps::list(cli.output).await?,
        },
        Command::Releases { command } => match command {
            ReleasesCommand::List { app } => commands::releases::list(&app, cli.output).await?,
        },
        Command::Artifacts { command } => match command {
            ArtifactsCommand::List { app, version } => {
                commands::artifacts::list(&app, &version, cli.output).await?
            }
        },
    }
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,swarmhive_cli=debug"));
    fmt().with_env_filter(filter).init();
}
