#!/usr/bin/env bash
# Provision a Pi Zero 2 W as the amygdala — the on-chassis BLE-to-WiFi
# gateway between the LEGO hub and a remote cortex.
#
# What this installs:
#   - bluez + libdbus-1-dev (so motorctl can talk BLE to the hub)
#   - rustup + cargo
#   - motorctl built in release mode, listening on Unix socket + gRPC
#   - reflex daemon, listening on its own gRPC port
#   - systemd units for both, started on boot
#   - a self-signed TLS cert under /etc/backpack/ (replace for production)
#
# Run on the Zero (NOT on the cortex Pi):
#   curl -sSL <url>/install_amygdala.sh | bash
# or after cloning:
#   ./scripts/install_amygdala.sh
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"

GRPC_PORT_BRICK="${GRPC_PORT_BRICK:-50051}"
GRPC_PORT_REFLEX="${GRPC_PORT_REFLEX:-50052}"

echo "==> apt deps"
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config \
  bluez libdbus-1-dev \
  protobuf-compiler \
  openssl ca-certificates \
  git curl

if ! command -v cargo >/dev/null 2>&1; then
  echo "==> rustup (this can take a while on a Zero 2 W)"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

echo "==> ensure bluetoothd is running"
sudo systemctl enable --now bluetooth

echo "==> generate self-signed TLS cert"
sudo mkdir -p /etc/backpack
if [ ! -f /etc/backpack/cert.pem ]; then
  sudo openssl req -x509 -newkey rsa:2048 -nodes -days 825 \
    -keyout /etc/backpack/key.pem -out /etc/backpack/cert.pem \
    -subj "/CN=$(hostname).local"
  sudo chmod 0600 /etc/backpack/key.pem
fi

echo "==> build motorctl + reflex (release)"
( cd "$ROOT" && cargo build --release -p motorctl -p reflex )

echo "==> install binaries"
sudo install -m 0755 "$ROOT/target/release/motorctl" /usr/local/bin/motorctl
sudo install -m 0755 "$ROOT/target/release/reflex"   /usr/local/bin/reflex

echo "==> systemd units"
sudo tee /etc/systemd/system/motorctl.service >/dev/null <<UNIT
[Unit]
Description=Backpack brick-link daemon (LWP3/Pybricks over BLE; gRPC bridge)
After=bluetooth.service
Requires=bluetooth.service

[Service]
ExecStart=/usr/local/bin/motorctl --socket /run/motorctl.sock --grpc-listen 0.0.0.0:${GRPC_PORT_BRICK}
Restart=on-failure
RestartSec=2
User=root

[Install]
WantedBy=multi-user.target
UNIT

sudo tee /etc/systemd/system/reflex.service >/dev/null <<UNIT
[Unit]
Description=Backpack reflex daemon (safety layer)
After=motorctl.service
Requires=motorctl.service

[Service]
ExecStart=/usr/local/bin/reflex --motorctl-socket /run/motorctl.sock --grpc-listen 0.0.0.0:${GRPC_PORT_REFLEX}
Restart=on-failure
RestartSec=2
User=root

[Install]
WantedBy=multi-user.target
UNIT

sudo systemctl daemon-reload
sudo systemctl enable motorctl reflex

cat <<DONE

Amygdala provisioned.

From the cortex (any host on the same network), point the cortex at:
  brick:    grpc://$(hostname).local:${GRPC_PORT_BRICK}
  reflex:   grpc://$(hostname).local:${GRPC_PORT_REFLEX}

TLS cert is self-signed at /etc/backpack/cert.pem — replace for production,
or pin it on the cortex side.

Next:
  sudo systemctl start motorctl reflex
  journalctl -fu motorctl  reflex
DONE
