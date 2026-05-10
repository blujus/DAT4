//! Pybricks BLE protocol — muscle-memory skill upload + Bluetooth messaging.
//!
//! Pybricks is community firmware for SPIKE Prime / Robot Inventor / City /
//! Technic / Move hubs. Unlike stock LEGO firmware (which only takes LWP3
//! motor commands), Pybricks accepts arbitrary MicroPython programs over
//! BLE, runs them on the hub itself, and exposes a bidirectional Bluetooth
//! messaging channel back to the Pi.
//!
//! References:
//! - <https://docs.pybricks.com/projects/pybricksdev/en/latest/api/ble.html>
//! - <https://github.com/pybricks/technical-info/blob/master/pybricks-ble-profile.md>
//!
//! GATT layout:
//!   Service:        c5f50001-8280-46da-89f4-6d8051e4aeef
//!     Control char: c5f50002-8280-46da-89f4-6d8051e4aeef  (write/notify)
//!     Hub capab.:   c5f50003-8280-46da-89f4-6d8051e4aeef  (read)
//!
//! "Code v2" upload flow (sketched):
//!  1. Pi reads the hub-capabilities char to learn the max chunk size.
//!  2. Pi compiles user Python to .mpy bytecode (`mpy-cross`).
//!  3. Pi writes "Start Program Download" command to the control char.
//!  4. Pi streams .mpy bytes in chunks of (max_chunk - header) bytes,
//!     waiting for an ACK after each chunk.
//!  5. Pi writes "Start User Program" to begin execution.
//!  6. Hub starts streaming `WriteStdout` events on the control char;
//!     bidirectional messages flow on the same channel.
//!
//! TODO: implement this. The current module exposes the public API so the
//! IPC layer compiles, but every method returns `not_implemented`. To
//! complete it, port the relevant pieces of `pybricksdev/ble` from
//! Python or wrap the official `pybricksdev` CLI behind `tokio::process`.

use anyhow::{anyhow, Result};
use uuid::Uuid;

pub const SERVICE: Uuid = Uuid::from_u128(0xc5f5_0001_8280_46da_89f4_6d80_51e4_aeef);
pub const CONTROL_CHAR: Uuid = Uuid::from_u128(0xc5f5_0002_8280_46da_89f4_6d80_51e4_aeef);
pub const CAPABILITIES_CHAR: Uuid =
    Uuid::from_u128(0xc5f5_0003_8280_46da_89f4_6d80_51e4_aeef);

/// Control-char command IDs (see Pybricks technical-info repo for the full set).
pub mod cmd {
    pub const STOP_USER_PROGRAM: u8 = 0x00;
    pub const START_USER_PROGRAM: u8 = 0x01;
    pub const START_REPL: u8 = 0x02;
    pub const WRITE_USER_PROGRAM_META: u8 = 0x03;
    pub const WRITE_USER_RAM: u8 = 0x04;
    pub const REBOOT_TO_UPDATE_MODE: u8 = 0x05;
    pub const WRITE_STDIN: u8 = 0x06;
    pub const WRITE_APP_DATA: u8 = 0x07;
}

pub struct PybricksLink {
    // Holds the connected Peripheral + characteristics. Filled in by
    // `brick::Brick` once we know we're talking to a Pybricks hub.
    // For now this is an opaque marker so the API surface compiles.
    pub(crate) _placeholder: (),
}

impl PybricksLink {
    pub fn new() -> Self {
        Self { _placeholder: () }
    }

    /// Compile `code` to .mpy bytecode and stream it to the hub, then
    /// start it as the user program.
    pub async fn load_skill(&self, _name: &str, _code: &str) -> Result<()> {
        Err(anyhow!(
            "pybricks code-v2 upload is not yet implemented (see crates/motorctl/src/pybricks.rs)"
        ))
    }

    /// Stop the running user program.
    pub async fn unload_skill(&self) -> Result<()> {
        Err(anyhow!(
            "pybricks stop_user_program is not yet implemented"
        ))
    }

    /// Push a payload to the running skill via Bluetooth messaging
    /// (`hub.ble.observe`/`broadcast` on the Pybricks side).
    pub async fn send_message(&self, _payload: &str) -> Result<()> {
        Err(anyhow!(
            "pybricks ble messaging is not yet implemented"
        ))
    }
}

impl Default for PybricksLink {
    fn default() -> Self {
        Self::new()
    }
}
