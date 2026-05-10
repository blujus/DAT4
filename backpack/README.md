# Backpack

A Raspberry Pi-powered cortex for LEGO Mindstorms / Technic robots.

The LEGO SPIKE Prime / Robot Inventor (51515) hub keeps doing what it's
good at — powering the robot and running the inner motor-control loop.
The Pi sits on top as an **agentic cortex**: instructions like *"follow
me"*, *"move in a circle"*, or *"pick that thing up"* go in as plain
English, and Claude (Opus 4.7, adaptive thinking) plans the action,
looks through the camera when it needs to, and drives the brick to
make it happen.

The brick is a tool the agent has access to. **And the agent can
upload new tools to the brick** — small Python control programs
("muscle memory") that run on the hub itself for skills too fast or
too jittery to live on the Pi: balance reflexes, tight diff-drive PID,
gaits. Those skills can be hand-written, or learned in the digital
twin and distilled back onto the hub.

```
  user goal
     │
     ▼
  +----------------------+
  |  Claude (cortex)     |
  |  Opus 4.7 + tools    |
  +----+--+--+-----------+
       |  |  | load_skill / skill_message
       |  |  ▼
       |  | +----------------------+        BLE        +-----------------+
       |  | |  motorctl (Rust)     |  ---------------▶  |  LEGO 51515 hub |
       |  | |  LWP3 + Pybricks     |                   |  (Pybricks fw)  |
       |  | +----------------------+                   |  + muscle skill |
       |  | drive / turn / arc / status                +--------+--------+
       |  v                                                     |
       |  Skills (Python)                                     LPF2
       v                                                        v
     look()                                                  motors
     Pi camera + Haiku 4.5
```

## Three time scales

This project is essentially a split-brain robot:

| Loop                     | Rate       | Where it lives          |
|--------------------------|------------|-------------------------|
| Cognitive cortex         | 0.1–1 Hz   | Pi (Python, Claude)     |
| Diff-drive primitives    | 10–100 Hz  | Pi (Python)             |
| Muscle memory            | 50–500 Hz  | LEGO hub (Pybricks)     |
| Inner motor PID          | ~1 kHz     | LEGO hub firmware       |

Moving a loop down the stack trades flexibility for tightness. The
cortex decides *what*, the brick decides *how* in milliseconds.

## Hardware

- Raspberry Pi 5 (4 GB or 8 GB) with built-in BLE
- LEGO 51515 hub running [Pybricks](https://pybricks.com/) firmware
  *(stock LEGO firmware also works for command-only mode — you lose
  muscle-memory upload but everything else is fine)*
- LEGO Technic motors and sensors
- Pi Camera Module 3 (or similar) for `look()`

See [`docs/hardware.md`](docs/hardware.md) for the firmware tradeoff
and [`docs/architecture.md`](docs/architecture.md) for the wire
protocols.

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
python -m backpack 'load the diff-drive PID skill, then drive forward at 0.4 m/s for 5 seconds'
```

## Training in the digital twin

```sh
cd backpack/twin
pip install -e '.[train]'

# Train a brick-level skill in MuJoCo
python -m twin.train_skill --skill diff_drive_pid --steps 500_000

# Distill the trained policy into a Pybricks-runnable skill
python -m twin.distill --skill diff_drive_pid \
  --policy runs/diff_drive_pid/policy.zip \
  --out ../skills_lib/diff_drive_pid_learned.py

# Or run the cortex itself against simulation
python -m twin.sim_daemon --socket /tmp/motorctl-sim.sock &
MOTORCTL_SOCKET=/tmp/motorctl-sim.sock python -m backpack 'drive in a circle'
```

See [`twin/README.md`](twin/README.md) for the full picture.

## Layout

```
backpack/
  Cargo.toml                  # Rust workspace
  rust-toolchain.toml
  crates/
    motorctl/                 # Rust daemon: BLE link + IPC
      src/lwp3.rs             #   LWP3 (LEGO firmware)
      src/pybricks.rs         #   Pybricks BLE protocol (skill upload, msgs)
      src/brick.rs            #   dual-firmware BLE connection layer
      src/ipc.rs               #   Unix-socket server
      src/proto.rs            #   wire types
      src/main.rs
  python/
    pyproject.toml
    backpack/
      ipc.py                  # client for motorctl
      skills.py               # diff-drive primitives (Pi-side)
      perception.py           # camera + vision
      agent.py                # Claude tool-use cortex
      __main__.py             # `python -m backpack '...'`
      orchestrator.py         # no-LLM hardware smoke test
  skills_lib/                 # "muscle memory" skills uploaded to the hub
    template.py
    diff_drive_pid.py
    balance.py
  twin/                       # MuJoCo digital twin + RL training
    pyproject.toml
    twin/world.py             # MuJoCo wrapper
    twin/env.py               # Gymnasium env
    twin/sim_daemon.py        # motorctl-compatible IPC against sim
    twin/train_skill.py       # PPO scaffold (stable-baselines3)
    twin/distill.py           # policy -> Pybricks skill
    assets/robot.xml          # MuJoCo MJCF
  scripts/
    install_pi.sh
  docs/
    architecture.md
    hardware.md
```

## Status

Scaffold-complete in five layers. What works end-to-end:

- BLE link to the hub (LWP3 motor commands; Pybricks detection at
  connect time)
- Cortex ↔ motorctl IPC for direct motor control + muscle-memory
  commands
- Cortex agentic loop with drive / turn / arc / look / load_skill
  tools
- MuJoCo digital twin + Gymnasium env + sim_daemon that the cortex
  can run against unchanged
- PPO training scaffold via stable-baselines3

What's stubbed and clearly marked with TODOs:

- The Pybricks Code v2 BLE wire protocol
  (`crates/motorctl/src/pybricks.rs`)
- The RL→MicroPython distillation pass
  (`twin/twin/distill.py`)

Either one is enough work to deserve its own follow-up.
