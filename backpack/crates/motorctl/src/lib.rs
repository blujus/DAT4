//! Brick-link daemon: connects the Pi cortex to a LEGO SPIKE Prime or
//! Mindstorms Robot Inventor (51515) hub over Bluetooth LE, and exposes
//! that link as a Unix-socket IPC server.
//!
//! The hub owns the inner control loop (PID, encoders, power). On stock
//! LEGO firmware we drive it with LWP3 Port Output commands. On Pybricks
//! firmware we additionally support uploading "muscle memory" skills
//! (small Python programs) that run on the hub itself, and bidirectional
//! Bluetooth messaging between those skills and the Pi cortex.

pub mod brick;
pub mod ipc;
pub mod lwp3;
pub mod proto;
pub mod pybricks;
