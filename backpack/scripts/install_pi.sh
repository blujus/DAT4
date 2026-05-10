#!/usr/bin/env bash
# Install dependencies and build motorctl on a fresh Raspberry Pi OS image.
# Tested target: Raspberry Pi 5, Raspberry Pi OS Bookworm (64-bit).
#
# Architecture: the LEGO 51515 hub keeps motor control + power. The Pi
# talks to it over BLE. So we install BlueZ + dbus headers (for btleplug)
# rather than touching UART.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"

echo "==> apt deps"
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config \
  bluez libdbus-1-dev \
  python3 python3-pip python3-venv \
  git curl

if ! command -v cargo >/dev/null 2>&1; then
  echo "==> rustup"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

echo "==> ensure bluetoothd is running"
sudo systemctl enable --now bluetooth

echo "==> build motorctl (release)"
( cd "$ROOT" && cargo build --release -p motorctl )

echo "==> install systemd unit"
sudo install -m 0755 "$ROOT/target/release/motorctl" /usr/local/bin/motorctl
sudo tee /etc/systemd/system/motorctl.service >/dev/null <<'UNIT'
[Unit]
Description=Backpack brick-link daemon (LWP3 over BLE)
After=bluetooth.service
Requires=bluetooth.service

[Service]
# Drop --name to accept any LEGO-like hub; pass e.g. --name SPIKE to pin.
ExecStart=/usr/local/bin/motorctl --socket /run/motorctl.sock
Restart=on-failure
RestartSec=2
# btleplug talks to bluetoothd over DBus; root keeps things simple.
User=root

[Install]
WantedBy=multi-user.target
UNIT
sudo systemctl daemon-reload
sudo systemctl enable motorctl

echo "==> python venv"
python3 -m venv "$ROOT/python/.venv"
# shellcheck disable=SC1091
source "$ROOT/python/.venv/bin/activate"
pip install -e "$ROOT/python"

cat <<DONE

Done. Power the LEGO hub on (it will start advertising over BLE) and:
  sudo systemctl start motorctl
  journalctl -fu motorctl       # watch the connection happen
  source $ROOT/python/.venv/bin/activate
  python -m backpack.orchestrator
DONE
