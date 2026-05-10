# Backpack

A Raspberry Pi-powered cortex for LEGO Mindstorms / Technic robots.

The LEGO SPIKE Prime / Robot Inventor (51515) hub keeps doing what it's
good at — powering the robot, running the inner motor-control loop,
talking to LPF2 sensors on its six ports. The Pi sits on top as an
**agentic cortex**: instructions like *"follow me"*, *"move in a circle"*,
or *"pick that thing up"* go in as plain English, and Claude (Opus 4.7,
adaptive thinking) plans the action, looks through the camera when it
needs to, and drives the brick to make it happen.

The brick is, in effect, a tool the agent has access to.

```
  user instruction
         │
         ▼
  +----------------------+
  |  Claude (cortex)     |
  |  Opus 4.7 + tool use |
  +----------+-----------+
     | drive / turn / arc / stop / look / status
     ▼
  +----------------------+         +-----------------------+
  |  Skills (Python)     |         |  Perception (Python)  |
  |  diff-drive maths    |         |  Pi camera + Haiku    |
  +----------+-----------+         +-----------------------+
             | JSON over Unix socket
             ▼
  +----------------------+
  |  motorctl (Rust)     |
  |  BLE / LWP3 link     |
  +----------+-----------+
             | Bluetooth LE
             ▼
  +----------------------+
  |  LEGO 51515 hub      |
  |  PID, encoders, 6 LPF2 ports |
  +----------+-----------+
             | LPF2
             ▼
     LEGO motors & sensors
```

## Why this split

| Layer            | Where it runs    | Job                                              |
|------------------|------------------|--------------------------------------------------|
| Cortex           | Pi (Python)      | Plan, perceive, decide. The agent loop.          |
| Skills           | Pi (Python)      | drive / turn / arc, encoder maths, calibration   |
| Perception       | Pi (Python)      | Pi camera + Claude Haiku 4.5 image captioning    |
| Brick link       | Pi (Rust)        | BLE/LWP3 to the hub, IPC to Python               |
| Motor control    | LEGO hub         | PID, stall detect, encoders, sensor I/O          |
| Power            | LEGO hub battery | Independent of the Pi                             |

Keeping the BLE link in Rust means the cortex can be restarted, hung,
or profiled without dropping the connection to the hub. Keeping the
cortex in Python means iteration is fast and the Anthropic SDK is
first-class.

## Hardware

- Raspberry Pi 5 (4 GB or 8 GB) with built-in BLE
- LEGO 51515 hub (SPIKE Prime / Mindstorms Robot Inventor)
- LEGO Technic motors and sensors
- Pi Camera Module 3 (or any libcamera-compatible camera) for `look()`

See [`docs/hardware.md`](docs/hardware.md).

## Quickstart (on the Pi)

```sh
git clone https://github.com/blujus/DAT4.git
cd DAT4/backpack
./scripts/install_pi.sh    # apt + rustup + build motorctl + systemd unit + venv
# put ANTHROPIC_API_KEY in backpack/.env
# turn the hub on, then:
sudo systemctl start motorctl
source python/.venv/bin/activate
set -a; source .env; set +a
python -m backpack 'drive forward 30 cm and stop'
```

More interesting prompts:

```sh
python -m backpack 'move in a circle of radius 40 cm'
python -m backpack 'look around and tell me what you see'
python -m backpack 'follow me — keep about 50 cm behind, stop if I stop'
```

For off-robot development (no LEGO hardware in front of you):

```sh
export BACKPACK_FAKE_CAMERA=1   # perception returns a stub sighting
# you'll still need motorctl pointed at a hub to actually drive motors
```

## Layout

```
backpack/
  Cargo.toml                  # Rust workspace
  rust-toolchain.toml
  crates/
    motorctl/                 # Rust daemon: BLE/LWP3 link + IPC
      src/lwp3.rs             #   LWP3 protocol encoding
      src/brick.rs            #   BLE connection layer
      src/ipc.rs              #   Unix-socket server
      src/proto.rs            #   wire types
      src/main.rs
  python/
    pyproject.toml
    backpack/
      ipc.py                  # client for motorctl
      skills.py               # diff-drive primitives
      perception.py           # camera + vision
      agent.py                # Claude tool-use cortex
      __main__.py             # `python -m backpack '...'`
      orchestrator.py         # no-LLM hardware smoke test
  scripts/
    install_pi.sh
  docs/
    architecture.md
    hardware.md
```

## Status

Early scaffold. The Rust daemon connects to the hub over BLE, encodes
LWP3 Port Output commands, and exposes them on the IPC socket. The
Python cortex wraps Claude with `drive`, `turn`, `arc`, `stop`, `look`,
and `get_status` tools. Next: completion events from the brick (so
`drive(30 cm)` doesn't rely on a sleep estimate), sensor streaming,
and a small mission DSL for things the agent shouldn't have to
re-derive every turn.
