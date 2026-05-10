//! gRPC bridge for the motorctl Unix-socket protocol.
//!
//! When the amygdala (a Pi Zero on the robot chassis) runs motorctl
//! with `--grpc-listen <addr>`, this module exposes the same
//! operations over gRPC so a remote cortex can drive the brick
//! across WiFi / LAN / Internet.
//!
//! Schema lives in `proto/backpack.proto`. The implementation should
//! translate proto requests to `crate::proto::Request`, hand them to
//! the existing `Daemon`, and translate the response back — the goal
//! is for the gRPC bridge to be observably identical to a Unix-socket
//! client, so anything that works locally works remotely.
//!
//! Status: stub. The `serve_grpc` entry point is wired into
//! `main.rs` but currently returns `not_implemented` so the rest of
//! the daemon compiles without pulling in `tonic`/`prost`.
//!
//! ## Implementer's notes
//!
//! 1. Add to `crates/motorctl/Cargo.toml`:
//!
//!    ```toml
//!    [dependencies]
//!    tonic = "0.12"
//!    prost = "0.13"
//!
//!    [build-dependencies]
//!    tonic-build = "0.12"
//!    ```
//!
//! 2. Create `crates/motorctl/build.rs`:
//!
//!    ```rust
//!    fn main() -> Result<(), Box<dyn std::error::Error>> {
//!        tonic_build::configure()
//!            .build_server(true)
//!            .build_client(false)
//!            .compile_protos(&["../../proto/backpack.proto"], &["../../proto"])?;
//!        Ok(())
//!    }
//!    ```
//!
//! 3. Replace this file with the generated module include + a
//!    `BrickLinkService` struct that implements
//!    `backpack_v1::brick_link_server::BrickLink`, holding an
//!    `Arc<Daemon>` and dispatching to it.
//!
//! 4. TLS: terminate at the gRPC server using `tonic::transport::ServerTlsConfig`.
//!    The amygdala's certificate is stored at `/etc/backpack/cert.pem` /
//!    `/etc/backpack/key.pem` (see `scripts/install_amygdala.sh`).

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{anyhow, Result};

use crate::ipc::Daemon;

pub async fn serve_grpc(_daemon: Arc<Daemon>, _addr: SocketAddr) -> Result<()> {
    Err(anyhow!(
        "gRPC bridge is scaffolded but not yet implemented \
         (see crates/motorctl/src/grpc.rs and proto/backpack.proto)"
    ))
}
