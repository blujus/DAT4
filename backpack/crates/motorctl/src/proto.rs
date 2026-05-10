use serde::{Deserialize, Serialize};

/// Commands accepted on the IPC socket. One JSON object per line.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    /// Drive `port` at `speed` in the range [-1.0, 1.0].
    SetSpeed { port: u8, speed: f32 },
    /// Let `port` coast to a stop.
    Stop { port: u8 },
    /// Hold position with active braking on `port`.
    Brake { port: u8 },
    /// Ask the daemon what's connected.
    Status,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "ok", rename_all = "snake_case")]
pub enum Response {
    Ack,
    Status { ports: Vec<PortInfo> },
    Err { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortInfo {
    pub port: u8,
    pub device: Option<String>,
    pub last_speed: f32,
}
