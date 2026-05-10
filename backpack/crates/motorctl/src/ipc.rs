use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::brick::Brick;
use crate::proto::{Firmware, PortInfo, Request, Response};

pub struct Daemon {
    brick: Mutex<Brick>,
    last_speeds: Mutex<[f32; 6]>,
    current_skill: Mutex<Option<String>>,
}

impl Daemon {
    pub fn new(brick: Brick) -> Self {
        Self {
            brick: Mutex::new(brick),
            last_speeds: Mutex::new([0.0; 6]),
            current_skill: Mutex::new(None),
        }
    }

    pub async fn handle(&self, req: Request) -> Response {
        match req {
            Request::SetSpeed { port, speed } => {
                let res = self.brick.lock().await.set_speed(port, speed).await;
                match res {
                    Ok(()) => {
                        if let Some(slot) = self.last_speeds.lock().await.get_mut(port as usize) {
                            *slot = speed;
                        }
                        Response::Ack
                    }
                    Err(e) => err(e),
                }
            }
            Request::Stop { port } => match self.brick.lock().await.coast(port).await {
                Ok(()) => Response::Ack,
                Err(e) => err(e),
            },
            Request::Brake { port } => match self.brick.lock().await.brake(port).await {
                Ok(()) => Response::Ack,
                Err(e) => err(e),
            },
            Request::RunForDegrees { port, degrees, speed } => {
                match self
                    .brick
                    .lock()
                    .await
                    .run_for_degrees(port, degrees, speed)
                    .await
                {
                    Ok(()) => Response::Ack,
                    Err(e) => err(e),
                }
            }
            Request::Status => {
                let speeds = *self.last_speeds.lock().await;
                let firmware = self.brick.lock().await.firmware();
                let current_skill = self.current_skill.lock().await.clone();
                Response::Status {
                    ports: (0..6u8)
                        .map(|p| PortInfo {
                            port: p,
                            device: None,
                            last_speed: speeds[p as usize],
                        })
                        .collect(),
                    firmware,
                    current_skill,
                }
            }
            Request::LoadSkill { name, code } => {
                match self.brick.lock().await.load_skill(&name, &code).await {
                    Ok(()) => {
                        *self.current_skill.lock().await = Some(name);
                        Response::Ack
                    }
                    Err(e) => err(e),
                }
            }
            Request::UnloadSkill => match self.brick.lock().await.unload_skill().await {
                Ok(()) => {
                    *self.current_skill.lock().await = None;
                    Response::Ack
                }
                Err(e) => err(e),
            },
            Request::SkillMessage { payload } => {
                match self.brick.lock().await.skill_message(&payload).await {
                    Ok(()) => Response::Ack,
                    Err(e) => err(e),
                }
            }
            Request::SubscribeSkillEvents => {
                // TODO: wire skill events out to the subscribed connection.
                // For now we acknowledge so the python side can no-op.
                Response::Ack
            }
        }
    }
}

fn err(e: anyhow::Error) -> Response {
    Response::Err {
        message: format!("{e:#}"),
    }
}

pub async fn serve(daemon: Arc<Daemon>, socket_path: &Path) -> Result<()> {
    if socket_path.exists() {
        let _ = std::fs::remove_file(socket_path);
    }
    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("binding {}", socket_path.display()))?;
    info!(socket = %socket_path.display(), "motorctl listening");

    loop {
        let (stream, _) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, "accept failed");
                continue;
            }
        };
        let d = daemon.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(d, stream).await {
                warn!(error = %e, "client error");
            }
        });
    }
}

async fn handle_client(daemon: Arc<Daemon>, stream: UnixStream) -> Result<()> {
    let (rx, mut tx) = stream.into_split();
    let mut reader = BufReader::new(rx);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(());
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Request>(trimmed) {
            Ok(req) => daemon.handle(req).await,
            Err(e) => Response::Err {
                message: format!("bad request: {e}"),
            },
        };
        let mut out = serde_json::to_vec(&resp)?;
        out.push(b'\n');
        tx.write_all(&out).await?;
        tx.flush().await?;
    }
}

// Suppress unused-import warning when Firmware is referenced only via the Status response.
const _: fn() = || {
    let _: Firmware;
};
