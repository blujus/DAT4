use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use motorctl::buildhat::{BuildHat, DEFAULT_PORT};
use motorctl::ipc::{serve, Daemon};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(version, about = "BuildHAT motor-control daemon")]
struct Args {
    /// Serial device for the BuildHAT.
    #[arg(long, default_value = DEFAULT_PORT)]
    serial: String,

    /// Unix domain socket to listen on.
    #[arg(long, default_value = "/run/motorctl.sock")]
    socket: PathBuf,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let args = Args::parse();
    info!(serial = %args.serial, socket = %args.socket.display(), "starting motorctl");

    let hat = BuildHat::open(&args.serial)?;
    let daemon = Arc::new(Daemon::new(hat));
    serve(daemon, &args.socket)
}
