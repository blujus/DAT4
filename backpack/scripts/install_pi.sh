#!/usr/bin/env bash
# Install dependencies and build motorctl on a fresh Raspberry Pi OS image.
# Tested target: Raspberry Pi 5, Raspberry Pi OS Bookworm (64-bit).
#
# Architecture: the LEGO 51515 hub keeps motor control + power. The Pi
# talks to it over BLE. So we install BlueZ + dbus headers (for btleplug)
# rather than touching UART. The Python cortex needs ANTHROPIC_API_KEY.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"

echo "==> apt deps"
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config \
  bluez libdbus-1-dev \
  python3 python3-pip python3-venv \
  python3-picamera2 \
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
User=root

[Install]
WantedBy=multi-user.target
UNIT
sudo systemctl daemon-reload
sudo systemctl enable motorctl

echo "==> python venv (with system picamera2 visible)"
python3 -m venv --system-site-packages "$ROOT/python/.venv"
# shellcheck disable=SC1091
source "$ROOT/python/.venv/bin/activate"
pip install -e "$ROOT/python"

if [ ! -f "$ROOT/.env" ]; then
  cat > "$ROOT/.env" <<'ENV'
# Set your Anthropic API key for the cortex.
# ANTHROPIC_API_KEY=sk-ant-...
#
# For off-robot dev (no Pi camera connected) uncomment:
# BACKPACK_FAKE_CAMERA=1
ENV
  echo "created $ROOT/.env (fill in ANTHROPIC_API_KEY)"
fi

cat <<DONE

Done. Two steps left:
  1. Put your Anthropic API key in $ROOT/.env
  2. Power the LEGO hub on, then:
       sudo systemctl start motorctl
       journalctl -fu motorctl       # watch the BLE connection happen
       source $ROOT/python/.venv/bin/activate
       set -a; source $ROOT/.env; set +a
       python -m backpack 'drive forward 20 cm'
DONE
