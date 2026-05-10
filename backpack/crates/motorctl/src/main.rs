use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use motorctl::brick::Brick;
use motorctl::grpc::serve_grpc;
use motorctl::ipc::{serve, Daemon};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(version, about = "LEGO hub link daemon (LWP3 + Pybricks over BLE)")]
struct Args {
    /// Substring matched against the hub's BLE advertised name.
    #[arg(long)]
    name: Option<String>,

    /// Unix domain socket to listen on.
    #[arg(long, default_value = "/run/motorctl.sock")]
    socket: PathBuf,

    /// If set, also expose the API over gRPC on this address (e.g.
    /// `0.0.0.0:50051`). Used in amygdala mode where the cortex runs
    /// off-robot.
    #[arg(long)]
    grpc_listen: Option<SocketAddr>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    info!(
        socket = %args.socket.display(),
        grpc = ?args.grpc_listen,
        name = ?args.name,
        "starting motorctl",
    );

    let brick = Brick::connect(args.name.as_deref()).await?;
    let daemon = Arc::new(Daemon::new(brick));

    if let Some(addr) = args.grpc_listen {
        let d = daemon.clone();
        tokio::spawn(async move {
            if let Err(e) = serve_grpc(d, addr).await {
                warn!(error = %e, "gRPC bridge unavailable");
            }
        });
    }

    serve(daemon, &args.socket).await
}
