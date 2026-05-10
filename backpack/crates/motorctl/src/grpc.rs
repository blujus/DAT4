//! gRPC bridge for the motorctl Unix-socket protocol.
//!
//! When the amygdala (a Pi Zero on the robot chassis) runs motorctl
//! with `--grpc-listen <addr>`, this module exposes the same
//! operations over gRPC so a remote cortex can drive the brick
//! across WiFi / LAN.
//!
//! Schema lives in `proto/backpack.proto`. Each RPC translates the
//! protobuf request to a `crate::proto::Request`, hands it to the
//! shared `Daemon`, and translates the response back, so the gRPC
//! bridge is observably identical to a Unix-socket client.
//!
//! The streaming RPCs (`StreamSkillEvents`, `StreamSensorEvents`)
//! return `Status::unimplemented` for now: the underlying daemon
//! doesn't yet expose an event bus. That's a follow-up.
//!
//! TLS is intentionally out of scope for the first cut — the
//! amygdala runs on a private LAN. When TLS lands it'll be wired in
//! via `tonic::transport::ServerTlsConfig` here.

use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use tonic::{Request as TonicRequest, Response as TonicResponse, Status};

use crate::ipc::Daemon;
use crate::proto as ipc_proto;

pub mod backpack_v1 {
    tonic::include_proto!("backpack.v1");
}

use backpack_v1::brick_link_server::{BrickLink, BrickLinkServer};
use backpack_v1::{
    Ack, Firmware as PbFirmware, LoadSkillRequest, PortInfo as PbPortInfo, PortRequest,
    RunForDegreesRequest, SensorEvent, SetSpeedRequest, SkillEvent, SkillMessageRequest,
    StatusRequest, StatusResponse, StreamSensorEventsRequest, StreamSkillEventsRequest,
    UnloadSkillRequest,
};

/// gRPC adapter around the shared `Daemon`.
pub struct BrickLinkService {
    daemon: Arc<Daemon>,
}

impl BrickLinkService {
    pub fn new(daemon: Arc<Daemon>) -> Self {
        Self { daemon }
    }
}

/// Translate a daemon `Response` into the proto `StatusResponse`.
/// Pulled out so it can be unit-tested without spinning a server.
fn status_response_to_proto(resp: ipc_proto::Response) -> Result<StatusResponse, Status> {
    match resp {
        ipc_proto::Response::Status {
            ports,
            firmware,
            current_skill,
        } => Ok(StatusResponse {
            firmware: firmware_to_proto(firmware) as i32,
            current_skill,
            ports: ports
                .into_iter()
                .map(|p| PbPortInfo {
                    port: p.port as u32,
                    device: p.device,
                    last_speed: p.last_speed,
                })
                .collect(),
        }),
        ipc_proto::Response::Err { message } => Err(Status::internal(message)),
        other => Err(Status::internal(format!(
            "unexpected daemon response for Status: {other:?}"
        ))),
    }
}

fn firmware_to_proto(fw: ipc_proto::Firmware) -> PbFirmware {
    match fw {
        ipc_proto::Firmware::Lego => PbFirmware::Lego,
        ipc_proto::Firmware::Pybricks => PbFirmware::Pybricks,
    }
}

fn ack_or_err(resp: ipc_proto::Response) -> Result<Ack, Status> {
    match resp {
        ipc_proto::Response::Ack => Ok(Ack {}),
        ipc_proto::Response::Err { message } => Err(Status::internal(message)),
        other => Err(Status::internal(format!(
            "unexpected daemon response: {other:?}"
        ))),
    }
}

fn port_u8(port: u32) -> Result<u8, Status> {
    u8::try_from(port).map_err(|_| Status::invalid_argument(format!("port {port} out of range")))
}

#[tonic::async_trait]
impl BrickLink for BrickLinkService {
    async fn set_speed(
        &self,
        request: TonicRequest<SetSpeedRequest>,
    ) -> Result<TonicResponse<Ack>, Status> {
        let r = request.into_inner();
        let req = ipc_proto::Request::SetSpeed {
            port: port_u8(r.port)?,
            speed: r.speed,
        };
        ack_or_err(self.daemon.handle(req).await).map(TonicResponse::new)
    }

    async fn run_for_degrees(
        &self,
        request: TonicRequest<RunForDegreesRequest>,
    ) -> Result<TonicResponse<Ack>, Status> {
        let r = request.into_inner();
        let req = ipc_proto::Request::RunForDegrees {
            port: port_u8(r.port)?,
            degrees: r.degrees,
            speed: r.speed,
        };
        ack_or_err(self.daemon.handle(req).await).map(TonicResponse::new)
    }

    async fn stop(
        &self,
        request: TonicRequest<PortRequest>,
    ) -> Result<TonicResponse<Ack>, Status> {
        let r = request.into_inner();
        let req = ipc_proto::Request::Stop {
            port: port_u8(r.port)?,
        };
        ack_or_err(self.daemon.handle(req).await).map(TonicResponse::new)
    }

    async fn brake(
        &self,
        request: TonicRequest<PortRequest>,
    ) -> Result<TonicResponse<Ack>, Status> {
        let r = request.into_inner();
        let req = ipc_proto::Request::Brake {
            port: port_u8(r.port)?,
        };
        ack_or_err(self.daemon.handle(req).await).map(TonicResponse::new)
    }

    async fn get_status(
        &self,
        _request: TonicRequest<StatusRequest>,
    ) -> Result<TonicResponse<StatusResponse>, Status> {
        let resp = self.daemon.handle(ipc_proto::Request::Status).await;
        status_response_to_proto(resp).map(TonicResponse::new)
    }

    async fn load_skill(
        &self,
        request: TonicRequest<LoadSkillRequest>,
    ) -> Result<TonicResponse<Ack>, Status> {
        let r = request.into_inner();
        let req = ipc_proto::Request::LoadSkill {
            name: r.name,
            code: r.code,
        };
        ack_or_err(self.daemon.handle(req).await).map(TonicResponse::new)
    }

    async fn unload_skill(
        &self,
        _request: TonicRequest<UnloadSkillRequest>,
    ) -> Result<TonicResponse<Ack>, Status> {
        let req = ipc_proto::Request::UnloadSkill;
        ack_or_err(self.daemon.handle(req).await).map(TonicResponse::new)
    }

    async fn skill_message(
        &self,
        request: TonicRequest<SkillMessageRequest>,
    ) -> Result<TonicResponse<Ack>, Status> {
        let r = request.into_inner();
        let req = ipc_proto::Request::SkillMessage { payload: r.payload };
        ack_or_err(self.daemon.handle(req).await).map(TonicResponse::new)
    }

    type StreamSkillEventsStream =
        Pin<Box<dyn futures::Stream<Item = Result<SkillEvent, Status>> + Send + 'static>>;

    async fn stream_skill_events(
        &self,
        _request: TonicRequest<StreamSkillEventsRequest>,
    ) -> Result<TonicResponse<Self::StreamSkillEventsStream>, Status> {
        Err(Status::unimplemented(
            "event streaming not yet wired up in the daemon",
        ))
    }

    type StreamSensorEventsStream =
        Pin<Box<dyn futures::Stream<Item = Result<SensorEvent, Status>> + Send + 'static>>;

    async fn stream_sensor_events(
        &self,
        _request: TonicRequest<StreamSensorEventsRequest>,
    ) -> Result<TonicResponse<Self::StreamSensorEventsStream>, Status> {
        Err(Status::unimplemented(
            "event streaming not yet wired up in the daemon",
        ))
    }
}

pub async fn serve_grpc(daemon: Arc<Daemon>, addr: SocketAddr) -> Result<()> {
    let svc = BrickLinkService::new(daemon);
    tonic::transport::Server::builder()
        .add_service(BrickLinkServer::new(svc))
        .serve(addr)
        .await
        .map_err(|e| anyhow!(e))
}
