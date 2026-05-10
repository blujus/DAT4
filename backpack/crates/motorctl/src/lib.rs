//! Real-time motor control daemon for the Raspberry Pi BuildHAT.
//!
//! `buildhat` owns the serial link to the HAT and translates higher-level
//! commands into the BuildHAT's line-oriented ASCII protocol. `ipc` exposes
//! those commands over a Unix domain socket so that a Python orchestrator
//! (or any other process) can drive the robot without having to deal with
//! the serial port directly.

pub mod buildhat;
pub mod ipc;
pub mod proto;
