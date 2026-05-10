//! LEGO Wireless Protocol 3 — message encoding for the SPIKE Prime /
//! Robot Inventor / Technic hubs.
//!
//! Reference: https://lego.github.io/lego-ble-wireless-protocol-docs/
//!
//! Message frame:
//!   [length, hub_id, message_type, ...payload]
//! `length` is single-byte if total length < 0x7F, otherwise two bytes
//! with the high bit of the first set. We only emit short frames here.

use uuid::Uuid;

pub const HUB_SERVICE: Uuid = Uuid::from_u128(0x00001623_1212_EFDE_1623_785FEABCD123);
pub const HUB_CHAR: Uuid = Uuid::from_u128(0x00001624_1212_EFDE_1623_785FEABCD123);

pub const HUB_ID: u8 = 0x00;

// Message types we currently care about.
pub const MSG_HUB_ATTACHED_IO: u8 = 0x04;
pub const MSG_PORT_OUTPUT: u8 = 0x81;
pub const MSG_PORT_OUTPUT_FB: u8 = 0x82;

// Port Output Command sub-commands.
pub const SUB_START_POWER: u8 = 0x01;
pub const SUB_START_SPEED: u8 = 0x07;
pub const SUB_START_SPEED_FOR_DEGREES: u8 = 0x0B;
pub const SUB_GOTO_ABS_POSITION: u8 = 0x0D;

/// Startup: execute immediately. Completion: no action.
pub const STARTUP_IMMEDIATE: u8 = 0x11;

/// End-state values for speed-with-degrees commands.
pub const END_FLOAT: u8 = 0;
pub const END_HOLD: u8 = 126;
pub const END_BRAKE: u8 = 127;

/// StartPower(port, power_pct). Special values: 0 = float, 127 = brake.
pub fn start_power(port: u8, power: i8) -> Vec<u8> {
    framed(&[
        HUB_ID,
        MSG_PORT_OUTPUT,
        port,
        STARTUP_IMMEDIATE,
        SUB_START_POWER,
        power as u8,
    ])
}

/// StartSpeed(port, speed_pct, max_power_pct, use_profile).
pub fn start_speed(port: u8, speed: i8, max_power: u8, use_profile: u8) -> Vec<u8> {
    framed(&[
        HUB_ID,
        MSG_PORT_OUTPUT,
        port,
        STARTUP_IMMEDIATE,
        SUB_START_SPEED,
        speed as u8,
        max_power,
        use_profile,
    ])
}

/// StartSpeedForDegrees(port, degrees, speed_pct, max_power, end_state, use_profile).
pub fn start_speed_for_degrees(
    port: u8,
    degrees: i32,
    speed: i8,
    max_power: u8,
    end_state: u8,
    use_profile: u8,
) -> Vec<u8> {
    let mut body = Vec::with_capacity(13);
    body.push(HUB_ID);
    body.push(MSG_PORT_OUTPUT);
    body.push(port);
    body.push(STARTUP_IMMEDIATE);
    body.push(SUB_START_SPEED_FOR_DEGREES);
    body.extend_from_slice(&degrees.to_le_bytes());
    body.push(speed as u8);
    body.push(max_power);
    body.push(end_state);
    body.push(use_profile);
    framed(&body)
}

pub fn coast(port: u8) -> Vec<u8> {
    start_power(port, 0)
}

pub fn brake(port: u8) -> Vec<u8> {
    start_power(port, 127)
}

fn framed(body: &[u8]) -> Vec<u8> {
    let payload_len = body.len();
    if payload_len < 0x7F {
        let mut out = Vec::with_capacity(payload_len + 1);
        out.push((payload_len + 1) as u8);
        out.extend_from_slice(body);
        out
    } else {
        let total = payload_len + 2;
        let mut out = Vec::with_capacity(total);
        out.push(0x80 | (total & 0x7F) as u8);
        out.push((total >> 7) as u8);
        out.extend_from_slice(body);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_power_50() {
        // length 7, hub 0, msg 0x81, port 0, startup 0x11, sub 0x01, power 50
        assert_eq!(start_power(0, 50), vec![0x07, 0x00, 0x81, 0x00, 0x11, 0x01, 50]);
    }

    #[test]
    fn start_speed_for_degrees_one_rev() {
        let m = start_speed_for_degrees(0, 360, 50, 100, END_HOLD, 0);
        // length should be 14 (0x0E)
        assert_eq!(m[0], 0x0E);
        assert_eq!(&m[1..6], &[0x00, 0x81, 0x00, 0x11, 0x0B]);
        // 360 little-endian
        assert_eq!(&m[6..10], &360i32.to_le_bytes());
        assert_eq!(m[10], 50);
        assert_eq!(m[11], 100);
        assert_eq!(m[12], END_HOLD);
        assert_eq!(m[13], 0);
    }
}
