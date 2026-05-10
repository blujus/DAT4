//! Brick-link daemon: connects the Pi cortex (or, with the gRPC
//! bridge, *any* network-reachable cortex) to a LEGO SPIKE Prime /
//! Robot Inventor (51515) hub over Bluetooth LE.
//!
//! On stock LEGO firmware we drive the hub with LWP3 Port Output
//! commands. On Pybricks firmware we additionally support uploading
//! "muscle memory" skills (small Python programs) that run on the hub
//! itself, and bidirectional Bluetooth messaging between those skills
//! and the cortex.
//!
//! Two listeners share the same `Daemon`:
//!   - Unix domain socket (always on; used by local tooling)
//!   - gRPC over TCP (optional, for remote cortex / amygdala mode)

pub mod brick;
pub mod grpc;
pub mod ipc;
pub mod lwp3;
pub mod proto;
pub mod pybricks;
