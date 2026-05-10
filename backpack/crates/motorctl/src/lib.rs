//! Brick-link daemon: connects the Pi cortex to a LEGO SPIKE Prime or
//! Mindstorms Robot Inventor (51515) hub over Bluetooth LE using the
//! LEGO Wireless Protocol (LWP3), and exposes that link as a Unix-socket
//! IPC server.
//!
//! The hub owns the inner control loop (PID, stall detection, encoders)
//! and the power rail. This daemon's job is to deliver high-level
//! commands without jitter so the Python cortex above can take its time
//! doing vision and planning.

pub mod brick;
pub mod ipc;
pub mod lwp3;
pub mod proto;
