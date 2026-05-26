use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

mod auth;
mod commands;
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
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize swarmhive.toml in the current directory.
    Init,
    /// Validate a release without uploading (dry-run).
    Verify,
    /// Upload artifacts and create a release.
    Publish,
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cli = Cli::parse();
    match cli.command {
        Command::Init => todo!("init: scaffold swarmhive.toml"),
        Command::Verify => todo!("verify: dry-run validation"),
        Command::Publish => todo!("publish: upload artifacts + create release"),
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
    }
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,swarmhive_cli=debug"));
    fmt().with_env_filter(filter).init();
}
