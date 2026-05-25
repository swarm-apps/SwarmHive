use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

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
    }
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,swarmhive_cli=debug"));
    fmt().with_env_filter(filter).init();
}
