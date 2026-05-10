# Backpack

A Raspberry Pi-powered cortex for LEGO Mindstorms / Technic robots.

The LEGO SPIKE Prime / Mindstorms Robot Inventor (51515) hub keeps doing
what it's good at — powering the robot, running the inner motor-control
loop, talking to LPF2 sensors on its six ports. The Pi sits on top as a
**cortex**: vision, planning, language, mission scripting, anything where
iteration speed and library ecosystem matter more than microseconds.

```
  Pi cortex  ──BLE LWP3──▶  LEGO 51515 hub  ──LPF2──▶  motors / sensors
  (Python +                     (low-level PID,                (the body)
   Rust link)                    power, encoders)
```

## Why this split

| Layer        | Where it runs | Job                                          |
|--------------|---------------|----------------------------------------------|
| Cortex       | Pi (Python)   | vision, planning, REPL, mission scripts      |
| Brick link   | Pi (Rust)     | BLE/LWP3 link to the hub, IPC server         |
| Motor control| LEGO hub      | PID, stall detect, encoders, sensor I/O      |
| Power        | LEGO hub      | battery + 9V motor rail                      |

The Rust daemon (`motorctl`) is intentionally thin now: it owns the BLE
characteristic and translates high-level commands into LWP3 frames. The
reason to keep it in Rust rather than calling `bleak` from Python is to
keep the link off the Python event loop — GC pauses or a slow vision
frame shouldn't translate into a stuttering robot.

The two processes talk over a Unix-domain socket with newline-delimited
JSON, so any tool that can write to a socket can drive the robot:

```sh
echo '{"cmd":"run_for_degrees","port":0,"degrees":360,"speed":0.5}' \
  | socat - UNIX-CONNECT:/run/motorctl.sock
```

## Hardware

- Raspberry Pi 5 (4 GB or 8 GB) with built-in BLE
- LEGO SPIKE Prime hub **or** Mindstorms Robot Inventor 51515 (six LPF2
  ports, internal battery, BLE + USB)
- LEGO Technic motors and sensors as needed

See [`docs/hardware.md`](docs/hardware.md) for pairing notes and the
LEGO build philosophy.

## Quickstart (on the Pi)

```sh
git clone https://github.com/blujus/DAT4.git
cd DAT4/backpack
./scripts/install_pi.sh    # apt + rustup + builds motorctl + systemd unit
# turn the hub on, then:
sudo systemctl start motorctl
python -m backpack.orchestrator
```

## Layout

```
backpack/
  Cargo.toml                  # Rust workspace
  rust-toolchain.toml
  crates/
    motorctl/                 # Rust daemon: BLE/LWP3 link + IPC
      src/lwp3.rs             #   protocol encoding
      src/brick.rs            #   BLE connection layer
      src/ipc.rs              #   Unix-socket server
      src/proto.rs            #   wire types
      src/main.rs
  python/
    pyproject.toml
    backpack/                 # Python cortex package
  scripts/
    install_pi.sh
  docs/
    architecture.md
    hardware.md
```

## Status

Early scaffold. The Rust daemon connects to the hub over BLE, encodes
LWP3 Port Output commands (StartSpeed, StartSpeedForDegrees, StartPower
for brake/coast), and exposes them on the IPC socket. Sensor streaming,
hub-attached-IO discovery and a mission DSL are next.
