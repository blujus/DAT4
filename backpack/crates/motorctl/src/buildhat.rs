use std::io::{BufRead, BufReader, Write};
use std::time::Duration;

use anyhow::{Context, Result};
use serialport::SerialPort;
use tracing::{debug, warn};

/// Default UART exposed by the Pi 5 / BuildHAT combo.
pub const DEFAULT_PORT: &str = "/dev/serial0";
pub const BAUD: u32 = 115_200;

/// Thin wrapper around the BuildHAT's line-oriented ASCII protocol.
///
/// The HAT firmware exposes commands like:
///   `port 0 ; plimit 0.7 ; set 0.5`     -- drive port 0 at 50% power
///   `port 0 ; coast`                    -- let port 0 coast
///   `port 0 ; pwm ; set 0`              -- active brake (pwm at 0)
///   `list`                              -- enumerate connected devices
pub struct BuildHat {
    tx: Box<dyn SerialPort>,
    rx: BufReader<Box<dyn SerialPort>>,
}

impl BuildHat {
    pub fn open(path: &str) -> Result<Self> {
        let tx = serialport::new(path, BAUD)
            .timeout(Duration::from_millis(200))
            .open()
            .with_context(|| format!("opening BuildHAT serial port {path}"))?;
        let rx_port = tx.try_clone().context("cloning serial port for reader")?;
        Ok(Self {
            tx,
            rx: BufReader::new(rx_port),
        })
    }

    /// Send a single line, terminated with `\r`. The HAT echoes the line
    /// back; we don't try to parse a structured reply here.
    pub fn send(&mut self, line: &str) -> Result<()> {
        debug!(target: "buildhat", %line, "tx");
        self.tx.write_all(line.as_bytes())?;
        self.tx.write_all(b"\r")?;
        self.tx.flush()?;
        Ok(())
    }

    /// Drive `port` at `speed` in [-1.0, 1.0].
    pub fn set_speed(&mut self, port: u8, speed: f32) -> Result<()> {
        let speed = speed.clamp(-1.0, 1.0);
        self.send(&format!("port {port} ; plimit 0.7 ; set {speed:.3}"))
    }

    pub fn coast(&mut self, port: u8) -> Result<()> {
        self.send(&format!("port {port} ; coast"))
    }

    pub fn brake(&mut self, port: u8) -> Result<()> {
        self.send(&format!("port {port} ; pwm ; set 0"))
    }

    /// Drain any pending lines from the HAT (firmware banners, async
    /// sensor data, etc). Non-blocking-ish: stops at the first timeout.
    pub fn drain(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        loop {
            let mut line = String::new();
            match self.rx.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => out.push(line.trim_end().to_string()),
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => break,
                Err(e) => {
                    warn!(target: "buildhat", error = %e, "rx error");
                    break;
                }
            }
        }
        out
    }
}
