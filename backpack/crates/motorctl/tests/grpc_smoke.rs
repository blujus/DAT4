//! End-to-end smoke test for the gRPC bridge.
//!
//! Spins up the gRPC server against a stubbed `Brick` (no BLE),
//! connects a tonic client over a real TCP socket on a random port,
//! and exercises `GetStatus` to assert request → response translation.
//!
//! BLE-touching RPCs (set_speed etc.) aren't covered here — the stub
//! brick errors on them by design; that path is exercised via unit
//! tests in `grpc.rs`'s helper functions.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use motorctl::brick::Brick;
use motorctl::grpc::backpack_v1::brick_link_client::BrickLinkClient;
use motorctl::grpc::backpack_v1::{Firmware as PbFirmware, StatusRequest};
use motorctl::grpc::serve_grpc;
use motorctl::ipc::Daemon;
use motorctl::proto::Firmware;
use tokio::net::TcpListener;

#[tokio::test]
async fn get_status_round_trip() {
    // Bind a TCP socket to grab a free port, then drop the listener so
    // tonic can rebind. There's an inherent TOCTOU window here, but on
    // CI loopback it's reliable enough for a smoke test.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    drop(listener);

    let brick = Brick::stub_for_tests(Firmware::Lego);
    let daemon = Arc::new(Daemon::new(brick));

    let server_handle = tokio::spawn(async move {
        serve_grpc(daemon, addr).await
    });

    // Give the server a beat to start listening.
    let endpoint = format!("http://{addr}");
    let mut client = None;
    for _ in 0..40 {
        match BrickLinkClient::connect(endpoint.clone()).await {
            Ok(c) => {
                client = Some(c);
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(25)).await,
        }
    }
    let mut client = client.expect("gRPC server never became reachable");

    let resp = client
        .get_status(StatusRequest {})
        .await
        .expect("get_status RPC failed")
        .into_inner();

    assert_eq!(resp.firmware, PbFirmware::Lego as i32);
    assert_eq!(resp.ports.len(), 6);
    for (i, port) in resp.ports.iter().enumerate() {
        assert_eq!(port.port, i as u32);
        assert_eq!(port.last_speed, 0.0);
        assert!(port.device.is_none());
    }
    assert!(resp.current_skill.is_none());

    server_handle.abort();
    let _ = server_handle.await;
}
