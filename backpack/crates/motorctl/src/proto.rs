use serde::{Deserialize, Serialize};

/// Commands accepted on the IPC socket. One JSON object per line.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    // ---- Direct motor control (any firmware) -----------------------------
    /// Drive `port` at `speed` in [-1.0, 1.0]. The brick keeps running
    /// until told otherwise.
    SetSpeed { port: u8, speed: f32 },
    /// Run `port` for `degrees` (encoder degrees), then hold.
    RunForDegrees { port: u8, degrees: i32, speed: f32 },
    /// Let `port` coast to a stop.
    Stop { port: u8 },
    /// Active brake on `port`.
    Brake { port: u8 },
    /// Snapshot of what the daemon thinks each port is doing.
    Status,

    // ---- Muscle memory (Pybricks-only) ------------------------------------
    /// Upload a skill (Pybricks Python program) to the hub and start it.
    /// The currently running skill, if any, is replaced.
    LoadSkill { name: String, code: String },
    /// Stop and unload the currently running skill.
    UnloadSkill,
    /// Send a message to the running skill. Delivered via the Pybricks
    /// Bluetooth messaging API.
    SkillMessage { payload: String },
    /// Subscribe this connection to events emitted by the running skill.
    /// Each event arrives as a `Response::SkillEvent`.
    SubscribeSkillEvents,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "ok", rename_all = "snake_case")]
pub enum Response {
    Ack,
    Status {
        ports: Vec<PortInfo>,
        firmware: Firmware,
        current_skill: Option<String>,
    },
    SkillEvent {
        payload: String,
    },
    Err {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortInfo {
    pub port: u8,
    pub device: Option<String>,
    pub last_speed: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Firmware {
    /// Stock LEGO firmware. LWP3 only, no skill upload.
    Lego,
    /// Pybricks firmware. Supports skill upload + Bluetooth messaging.
    Pybricks,
}
