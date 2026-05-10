use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tracing::{error, info, warn};

use crate::buildhat::BuildHat;
use crate::proto::{PortInfo, Request, Response};

/// Daemon-side state shared across IPC connections.
pub struct Daemon {
    hat: Mutex<BuildHat>,
    last_speeds: Mutex<[f32; 4]>,
}

impl Daemon {
    pub fn new(hat: BuildHat) -> Self {
        Self {
            hat: Mutex::new(hat),
            last_speeds: Mutex::new([0.0; 4]),
        }
    }

    pub fn handle(&self, req: Request) -> Response {
        match req {
            Request::SetSpeed { port, speed } => self.with_hat(|h| h.set_speed(port, speed))
                .map(|_| {
                    if let Some(slot) = self.last_speeds.lock().unwrap().get_mut(port as usize) {
                        *slot = speed;
                    }
                    Response::Ack
                })
                .unwrap_or_else(err),
            Request::Stop { port } => self
                .with_hat(|h| h.coast(port))
                .map(|_| Response::Ack)
                .unwrap_or_else(err),
            Request::Brake { port } => self
                .with_hat(|h| h.brake(port))
                .map(|_| Response::Ack)
                .unwrap_or_else(err),
            Request::Status => {
                let speeds = *self.last_speeds.lock().unwrap();
                Response::Status {
                    ports: (0..4u8)
                        .map(|p| PortInfo {
                            port: p,
                            device: None,
                            last_speed: speeds[p as usize],
                        })
                        .collect(),
                }
            }
        }
    }

    fn with_hat<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut BuildHat) -> Result<T>,
    {
        let mut hat = self.hat.lock().unwrap();
        f(&mut hat)
    }
}

fn err(e: anyhow::Error) -> Response {
    Response::Err {
        message: format!("{e:#}"),
    }
}

pub fn serve(daemon: Arc<Daemon>, socket_path: &Path) -> Result<()> {
    if socket_path.exists() {
        std::fs::remove_file(socket_path).ok();
    }
    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("binding {}", socket_path.display()))?;
    info!(socket = %socket_path.display(), "motorctl listening");

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let d = daemon.clone();
                std::thread::spawn(move || {
                    if let Err(e) = handle_client(d, s) {
                        warn!(error = %e, "client error");
                    }
                });
            }
            Err(e) => error!(error = %e, "accept failed"),
        }
    }
    Ok(())
}

fn handle_client(daemon: Arc<Daemon>, stream: UnixStream) -> Result<()> {
    let reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Request>(&line) {
            Ok(req) => daemon.handle(req),
            Err(e) => Response::Err {
                message: format!("bad request: {e}"),
            },
        };
        let mut out = serde_json::to_vec(&resp)?;
        out.push(b'\n');
        writer.write_all(&out)?;
        writer.flush()?;
    }
    Ok(())
}
