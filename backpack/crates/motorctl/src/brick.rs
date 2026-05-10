//! BLE link to a SPIKE Prime / Robot Inventor (51515) hub.
//!
//! Discovery is by advertised name, with the firmware family detected
//! from which GATT service the hub exposes:
//!
//! - LEGO LWP3 service (`00001623-...`)  -> stock LEGO firmware, command-only
//! - Pybricks service   (`c5f50001-...`) -> Pybricks firmware, supports
//!                                          skill upload + BLE messaging
//!
//! Once connected, motor commands go to the LWP3 characteristic with no
//! response so we don't pay a BLE round-trip per command. Pybricks
//! skill ops route through `pybricks::PybricksLink` (currently stubbed).

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use tokio::time::sleep;
use tracing::{info, warn};

use crate::lwp3;
use crate::proto::Firmware;
use crate::pybricks::{self, PybricksLink};

pub struct Brick {
    peripheral: Option<Peripheral>,
    lwp3_char: Option<btleplug::api::Characteristic>,
    pybricks: Option<PybricksLink>,
    firmware: Firmware,
}

impl Brick {
    /// Build a `Brick` with no underlying BLE connection. Used by tests
    /// that exercise the IPC / gRPC plumbing without needing a real hub.
    /// Any BLE-touching call (`set_speed`, `coast`, `brake`,
    /// `run_for_degrees`, skill ops) will return an error; `firmware()`
    /// returns the value passed in.
    pub fn stub_for_tests(firmware: Firmware) -> Self {
        Self {
            peripheral: None,
            lwp3_char: None,
            pybricks: None,
            firmware,
        }
    }
}

impl Brick {
    pub async fn connect(name_filter: Option<&str>) -> Result<Self> {
        let manager = Manager::new().await.context("creating BLE manager")?;
        let adapter = manager
            .adapters()
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no BLE adapter found (is bluetoothd running?)"))?;

        adapter
            .start_scan(ScanFilter {
                services: vec![lwp3::HUB_SERVICE, pybricks::SERVICE],
            })
            .await
            .context("starting BLE scan")?;
        info!(filter = ?name_filter, "scanning for hub");

        let peripheral = find_hub(&adapter, name_filter).await?;
        peripheral.connect().await.context("connecting to hub")?;
        peripheral
            .discover_services()
            .await
            .context("discovering services")?;

        // Detect firmware by which service is exposed.
        let services = peripheral.services();
        let has_lwp3 = services.iter().any(|s| s.uuid == lwp3::HUB_SERVICE);
        let has_pybricks = services.iter().any(|s| s.uuid == pybricks::SERVICE);

        let (firmware, lwp3_char, pybricks) = match (has_lwp3, has_pybricks) {
            (_, true) => (Firmware::Pybricks, None, Some(PybricksLink::new())),
            (true, false) => {
                let c = peripheral
                    .characteristics()
                    .into_iter()
                    .find(|c| c.uuid == lwp3::HUB_CHAR)
                    .ok_or_else(|| anyhow!("LWP3 characteristic not found on hub"))?;
                peripheral
                    .subscribe(&c)
                    .await
                    .context("subscribing to hub notifications")?;
                (Firmware::Lego, Some(c), None)
            }
            (false, false) => {
                return Err(anyhow!(
                    "connected hub exposes neither LWP3 nor Pybricks service"
                ));
            }
        };

        info!(?firmware, "connected to hub");
        Ok(Self {
            peripheral: Some(peripheral),
            lwp3_char,
            pybricks,
            firmware,
        })
    }

    pub fn firmware(&self) -> Firmware {
        self.firmware
    }

    async fn write_lwp3(&self, msg: &[u8]) -> Result<()> {
        let peripheral = self
            .peripheral
            .as_ref()
            .ok_or_else(|| anyhow!("BLE peripheral not connected (test stub?)"))?;
        let char = self
            .lwp3_char
            .as_ref()
            .ok_or_else(|| anyhow!("LWP3 not available on this firmware"))?;
        peripheral
            .write(char, msg, WriteType::WithoutResponse)
            .await
            .context("writing LWP3 frame")?;
        Ok(())
    }

    pub async fn set_speed(&self, port: u8, speed: f32) -> Result<()> {
        let speed_pct = (speed.clamp(-1.0, 1.0) * 100.0) as i8;
        self.write_lwp3(&lwp3::start_speed(port, speed_pct, 100, 0))
            .await
    }

    pub async fn coast(&self, port: u8) -> Result<()> {
        self.write_lwp3(&lwp3::coast(port)).await
    }

    pub async fn brake(&self, port: u8) -> Result<()> {
        self.write_lwp3(&lwp3::brake(port)).await
    }

    pub async fn run_for_degrees(&self, port: u8, degrees: i32, speed: f32) -> Result<()> {
        let speed_pct = (speed.clamp(-1.0, 1.0) * 100.0) as i8;
        self.write_lwp3(&lwp3::start_speed_for_degrees(
            port,
            degrees,
            speed_pct,
            100,
            lwp3::END_HOLD,
            0,
        ))
        .await
    }

    pub async fn load_skill(&self, name: &str, code: &str) -> Result<()> {
        let pb = self.require_pybricks("load_skill")?;
        pb.load_skill(name, code).await
    }

    pub async fn unload_skill(&self) -> Result<()> {
        let pb = self.require_pybricks("unload_skill")?;
        pb.unload_skill().await
    }

    pub async fn skill_message(&self, payload: &str) -> Result<()> {
        let pb = self.require_pybricks("skill_message")?;
        pb.send_message(payload).await
    }

    fn require_pybricks(&self, op: &str) -> Result<&PybricksLink> {
        self.pybricks.as_ref().ok_or_else(|| {
            anyhow!(
                "{op} requires Pybricks firmware on the hub; got {:?}",
                self.firmware
            )
        })
    }
}

async fn find_hub(adapter: &Adapter, name_filter: Option<&str>) -> Result<Peripheral> {
    for attempt in 0..30 {
        for p in adapter.peripherals().await? {
            let props = p.properties().await?.unwrap_or_default();
            let name = props.local_name.unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let matches = match name_filter {
                Some(f) => name.to_ascii_lowercase().contains(&f.to_ascii_lowercase()),
                None => looks_like_lego_hub(&name),
            };
            if matches {
                info!(name = %name, "found candidate hub");
                return Ok(p);
            }
        }
        if attempt == 0 {
            warn!("no hub yet — make sure the hub is on and advertising");
        }
        sleep(Duration::from_millis(500)).await;
    }
    Err(anyhow!("no hub advertised within timeout"))
}

fn looks_like_lego_hub(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("lego")
        || n.contains("spike")
        || n.contains("technic")
        || n.contains("mindstorms")
        || n.contains("pybricks")
}
