#!/usr/bin/env bash
# Install dependencies and build motorctl on a fresh Raspberry Pi OS image.
# Tested target: Raspberry Pi 5, Raspberry Pi OS Bookworm (64-bit).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"

echo "==> apt deps"
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config libudev-dev \
  python3 python3-pip python3-venv \
  git curl

if ! command -v cargo >/dev/null 2>&1; then
  echo "==> rustup"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

echo "==> enable UART for BuildHAT"
# Pi 5: the BuildHAT lives on /dev/serial0. Disable the serial console and
# enable the UART. Idempotent: only edits if the lines aren't already there.
CONFIG=/boot/firmware/config.txt
sudo raspi-config nonint do_serial 2 || true   # disable login shell on serial
grep -q '^enable_uart=1' "$CONFIG" || echo 'enable_uart=1' | sudo tee -a "$CONFIG"
grep -q '^dtoverlay=disable-bt' "$CONFIG" || echo 'dtoverlay=disable-bt' | sudo tee -a "$CONFIG"

echo "==> build motorctl (release)"
( cd "$ROOT" && cargo build --release -p motorctl )

echo "==> install systemd unit"
sudo install -m 0755 "$ROOT/target/release/motorctl" /usr/local/bin/motorctl
sudo tee /etc/systemd/system/motorctl.service >/dev/null <<'UNIT'
[Unit]
Description=Backpack motor-control daemon
After=network.target

[Service]
ExecStart=/usr/local/bin/motorctl --serial /dev/serial0 --socket /run/motorctl.sock
Restart=on-failure
RestartSec=1

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

Done. Reboot once so the UART changes take effect, then:
  sudo systemctl start motorctl
  source $ROOT/python/.venv/bin/activate
  python -m backpack.orchestrator
DONE
