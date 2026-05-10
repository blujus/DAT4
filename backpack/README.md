# Backpack

A Raspberry Pi-powered "backpack" brain for LEGO Mindstorms / Technic
robots. The Pi (a Raspberry Pi 5 with the official Build HAT) replaces the
LEGO hub as the controller, while the LEGO motors, sensors and bricks become
the physical body.

The goal is to materialise software into the physical world through LEGO,
but without being bottlenecked by the LEGO hub's firmware. A small Rust
daemon (`motorctl`) owns the real-time loop and the BuildHAT serial link;
a Python orchestrator on top handles vision, planning and mission logic.

## Why this split

| Layer       | Language | Job                                              |
|-------------|----------|--------------------------------------------------|
| `motorctl`  | Rust     | BuildHAT UART, PID, deterministic 1 kHz loop     |
| Orchestrator| Python   | Vision, planning, scripted behaviours, REPL/CLI  |

Rust handles anything where jitter or latency would make the robot wobble,
stall or miss an encoder tick. Python handles anything where iteration
speed matters more than microseconds.

The two talk over a Unix domain socket with newline-delimited JSON. That
keeps the wire format trivial to debug (`socat - UNIX-CONNECT:/run/motorctl.sock`)
while staying fast enough on a Pi 5 (sub-millisecond round-trip locally).

## Hardware

- Raspberry Pi 5 (4 GB or 8 GB)
- Raspberry Pi Build HAT (4 LPF2 ports, drives PoweredUp / SPIKE / Technic
  motors and sensors)
- LEGO Technic motors (Medium / Large angular motors recommended)
- LEGO sensors (colour, distance, force) as needed
- USB-C 5 V / 5 A PSU for the Pi, 8 V barrel jack on the BuildHAT for
  motor power

See [`docs/hardware.md`](docs/hardware.md) for wiring and the LEGO build
philosophy.

## Quickstart (on the Pi)

```sh
git clone https://github.com/blujus/DAT4.git
cd DAT4/backpack
./scripts/install_pi.sh    # installs deps, enables UART, builds motorctl
sudo systemctl start motorctl
python -m backpack.orchestrator
```

## Layout

```
backpack/
  Cargo.toml                  # Rust workspace
  rust-toolchain.toml
  crates/
    motorctl/                 # Rust daemon: BuildHAT + IPC
  python/
    pyproject.toml
    backpack/                 # Python orchestrator package
  scripts/
    install_pi.sh
  docs/
    architecture.md
    hardware.md
```

## Status

Early scaffold. The BuildHAT serial protocol stub round-trips `set` /
`coast` / `list` commands; the IPC server accepts `set_speed`, `stop` and
`status`. Closed-loop control, sensor streaming and mission DSL are
next.
