//! BLE link to a SPIKE Prime / Robot Inventor (51515) hub.
//!
//! Discovery is by advertised name (e.g. "LEGO Hub", "Technic Move") with
//! an optional substring filter. Once connected, all commands are written
//! to the hub's LWP3 characteristic with no response so we don't pay a
//! BLE round-trip per command.

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use tokio::time::sleep;
use tracing::{info, warn};

use crate::lwp3;

pub struct Brick {
    peripheral: Peripheral,
    char: btleplug::api::Characteristic,
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
                services: vec![lwp3::HUB_SERVICE],
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

        let char = peripheral
            .characteristics()
            .into_iter()
            .find(|c| c.uuid == lwp3::HUB_CHAR)
            .ok_or_else(|| anyhow!("LWP3 characteristic not found on hub"))?;
        peripheral
            .subscribe(&char)
            .await
            .context("subscribing to hub notifications")?;

        info!("connected to hub");
        Ok(Self { peripheral, char })
    }

    async fn write(&self, msg: &[u8]) -> Result<()> {
        self.peripheral
            .write(&self.char, msg, WriteType::WithoutResponse)
            .await
            .context("writing LWP3 frame")?;
        Ok(())
    }

    pub async fn set_speed(&self, port: u8, speed: f32) -> Result<()> {
        let speed_pct = (speed.clamp(-1.0, 1.0) * 100.0) as i8;
        self.write(&lwp3::start_speed(port, speed_pct, 100, 0)).await
    }

    pub async fn coast(&self, port: u8) -> Result<()> {
        self.write(&lwp3::coast(port)).await
    }

    pub async fn brake(&self, port: u8) -> Result<()> {
        self.write(&lwp3::brake(port)).await
    }

    pub async fn run_for_degrees(&self, port: u8, degrees: i32, speed: f32) -> Result<()> {
        let speed_pct = (speed.clamp(-1.0, 1.0) * 100.0) as i8;
        self.write(&lwp3::start_speed_for_degrees(
            port,
            degrees,
            speed_pct,
            100,
            lwp3::END_HOLD,
            0,
        ))
        .await
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
    n.contains("lego") || n.contains("spike") || n.contains("technic") || n.contains("mindstorms")
}
