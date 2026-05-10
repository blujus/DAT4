//! Reflex daemon — the safety layer between the cortex and the brick.
//!
//! Runs alongside `motorctl` on the amygdala (Pi Zero on the chassis).
//! It connects to motorctl as a *client* (Unix socket today; gRPC
//! later) and can issue motor stops at any time, independent of
//! whatever the cortex is doing.
//!
//! The reflex owns three jobs:
//!
//!   - **Watchdog**. The cortex keeps a heartbeat stream open. If a
//!     ping doesn't arrive within `--watchdog-ms`, the reflex brakes
//!     all motors and surfaces `last_trigger="watchdog_timeout"`.
//!   - **Tilt guard**. IMU tilt past a threshold → coast all motors,
//!     `last_trigger="tilt"`.
//!   - **Bumper / proximity**. A configurable port + threshold (force,
//!     distance, colour) triggers an immediate stop.
//!
//! Triggering is one-way for now: the reflex can stop the robot but
//! does not arm new motions. Recovery is the cortex's job. Once a
//! trigger fires, the reflex stays "latched" until the cortex calls
//! `Reflex.Reset` (TODO).
//!
//! Status: scaffold. Watchdog and trigger logic are TODOs (see
//! comments). The `--grpc-listen` flag and the heartbeat server are
//! the obvious first piece to wire up.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(version, about = "Backpack reflex daemon (safety layer)")]
struct Args {
    /// Unix socket where motorctl is listening.
    #[arg(long, default_value = "/run/motorctl.sock")]
    motorctl_socket: PathBuf,

    /// gRPC listen address for the cortex heartbeat (e.g. 0.0.0.0:50052).
    #[arg(long)]
    grpc_listen: Option<SocketAddr>,

    /// Default watchdog deadline. The cortex can override on the first
    /// heartbeat ping.
    #[arg(long, default_value_t = 500)]
    watchdog_ms: u64,

    /// Tilt threshold in degrees from vertical. Above this, coast all motors.
    #[arg(long, default_value_t = 45.0)]
    tilt_deg: f32,

    /// Hub port to use as the bumper / proximity trigger (set -1 to disable).
    #[arg(long, default_value_t = -1)]
    bumper_port: i32,

    /// Bumper trigger threshold (units depend on the device on `bumper_port`).
    #[arg(long, default_value_t = 0.5)]
    bumper_threshold: f32,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    info!(?args, "reflex starting");

    // TODO:
    //   1. Connect to motorctl Unix socket as a long-lived client.
    //   2. Spawn a tokio task running the gRPC Reflex heartbeat server
    //      (see proto/backpack.proto :: Reflex). Track last-seen ms.
    //   3. Watchdog loop: if last_seen_ms > deadline, send Brake on
    //      ports 0..6 via the motorctl client, latch state to
    //      "watchdog_timeout".
    //   4. Sensor loop: subscribe_sensor_events; on tilt > tilt_deg or
    //      bumper crossing, send Stop and latch.
    //
    // For now the binary exits cleanly so it can be wired into the
    // systemd unit and the rest of the deploy without holding things up.
    Err(anyhow!(
        "reflex daemon scaffold; implementation pending (see crates/reflex/src/main.rs)"
    ))
}
