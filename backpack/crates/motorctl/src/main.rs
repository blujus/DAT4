use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use motorctl::brick::Brick;
use motorctl::ipc::{serve, Daemon};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(version, about = "LEGO hub link daemon (LWP3 over BLE)")]
struct Args {
    /// Substring matched against the hub's BLE advertised name
    /// (e.g. "LEGO", "Technic", "SPIKE"). Default: any LEGO-like hub.
    #[arg(long)]
    name: Option<String>,

    /// Unix domain socket to listen on.
    #[arg(long, default_value = "/run/motorctl.sock")]
    socket: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    info!(socket = %args.socket.display(), name = ?args.name, "starting motorctl");

    let brick = Brick::connect(args.name.as_deref()).await?;
    let daemon = Arc::new(Daemon::new(brick));
    serve(daemon, &args.socket).await
}
