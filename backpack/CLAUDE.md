# Backpack — CLAUDE.md

An agentic LEGO Mindstorms robot. Cortex (Claude Opus 4.7) on a Pi
or anywhere on the network. Brick on the LEGO 51515 hub. MuJoCo
digital twin for RL training and virtual testing.

This file is the operating manual for working in `backpack/`. Read it
before touching code.

## Architecture (read first)

Four layers, each owning a different time scale:

| Layer         | Runs on                | Rate       | Job                                          |
|---------------|------------------------|------------|----------------------------------------------|
| Cortex        | Pi / laptop / cloud    | 0.1–1 Hz   | Claude Opus 4.7 tool-use loop. Plans, calls. |
| Skills        | Pi (Python)            | 10–100 Hz  | Diff-drive primitives. Perception.           |
| Muscle memory | LEGO hub (Pybricks)    | 50–500 Hz  | Reflexes uploaded as small Python programs.  |
| Motor PID     | LEGO hub firmware      | ~1 kHz     | Built-in. We don't touch it.                 |

Plus the **amygdala** — a Pi Zero 2 W on the chassis acting as a
BLE↔WiFi gateway when the cortex runs off-robot. It hosts `motorctl`
and a `reflex` daemon for safety (cortex-watchdog, tilt, bumper).

## Tree

```
backpack/
  proto/backpack.proto                # gRPC schema (BrickLink + Reflex)
  Cargo.toml                          # Rust workspace
  rust-toolchain.toml
  crates/
    motorctl/                         # BLE ↔ IPC daemon (Unix socket + gRPC)
      src/proto.rs                    #   Unix-socket wire types
      src/lwp3.rs                     #   LEGO Wireless Protocol encoder
      src/pybricks.rs                 #   Pybricks BLE upload (STUB)
      src/brick.rs                    #   dual-firmware BLE link
      src/ipc.rs                      #   Unix-socket server (Daemon)
      src/grpc.rs                     #   tonic gRPC bridge
      src/main.rs
      build.rs                        #   tonic-build for backpack.proto
    reflex/                           # safety daemon (STUB)
  python/
    pyproject.toml
    backpack/
      ipc.py                          #   MotorCtl client (Unix socket)
      grpc_client.py                  #   gRPC client (STUB)
      skills.py                       #   diff-drive math
      perception.py                   #   Pi camera + Haiku captioning
      agent.py                        #   Claude Opus 4.7 tool-use cortex
      __main__.py                     #   `python -m backpack '<instruction>'`
      orchestrator.py                 #   no-LLM hardware smoke test
  skills_lib/                         # Pybricks programs uploaded to the hub
    template.py                       #   minimal skeleton
    diff_drive_pid.py                 #   hand-written diff-drive PID
    balance.py                        #   self-balance reflex (placeholder)
    README.md
  twin/
    pyproject.toml
    twin/world.py                     #   MuJoCo wrapper (Twin class)
    twin/env.py                       #   Gymnasium env (BackpackEnv)
    twin/sim_daemon.py                #   motorctl-compatible IPC against sim
    twin/demo.py                      #   scripted square; viewer or MP4 record
    twin/train_skill.py               #   PPO scaffold (stable-baselines3)
    twin/distill.py                   #   policy → Pybricks PID skill
    assets/robot.xml                  #   MuJoCo MJCF (generic skid-steer)
    tests/test_smoke.py               #   Skills + sim_daemon end-to-end
    tests/test_distill.py             #   PID gain recovery
    TESTING.md
  scripts/
    install_pi.sh                     #   cortex Pi (Pi 5)
    install_amygdala.sh               #   amygdala Pi Zero 2 W
  docs/
    architecture.md
    hardware.md
    amygdala.md
```

## Build & test commands

**Rust** (motorctl + reflex). Apt deps: `protobuf-compiler`, `libdbus-1-dev`, `pkg-config`.

```sh
cd backpack
cargo build -p motorctl
cargo test  -p motorctl                       # 1 unit + 1 grpc_smoke integration test
cargo build --release -p motorctl -p reflex   # for deploy
```

**Python cortex.** Needs `ANTHROPIC_API_KEY`.

```sh
pip install -e backpack/python
python -m backpack 'drive forward 30 cm'
```

For off-robot dev without a Pi camera attached: `export BACKPACK_FAKE_CAMERA=1`.

**Python twin.**

```sh
pip install -e 'backpack/twin'                # mujoco + gymnasium
pip install -e 'backpack/twin[train]'         # + sb3 + torch (RL training)
pip install -e 'backpack/twin[video]'         # + imageio-ffmpeg (demo --record)
pytest backpack/twin/tests/                   # 4 distill + 2 sim_daemon tests
```

**Watch the bot** (see `twin/TESTING.md` for the full breakdown):

```sh
python -m twin.demo                           # interactive MuJoCo viewer
MUJOCO_GL=osmesa python -m twin.demo --record demo.mp4   # headless MP4
```

**End-to-end virtual run** (cortex driving the simulation):

```sh
# terminal 1
python -m twin.sim_daemon --socket /tmp/motorctl-sim.sock
# terminal 2
export ANTHROPIC_API_KEY=sk-ant-...
MOTORCTL_SOCKET=/tmp/motorctl-sim.sock python -m backpack 'drive in a circle of radius 40 cm'
```

## Wire protocols

- **Cortex ↔ motorctl (local):** newline-JSON over Unix socket. Canonical
  schema lives in `crates/motorctl/src/proto.rs`. The Python client is
  `python/backpack/ipc.py`. The same protocol speaks to the real Rust
  daemon and to `twin/twin/sim_daemon.py` — the cortex doesn't know which.
- **Cortex ↔ motorctl (remote, amygdala mode):** gRPC. Schema:
  `proto/backpack.proto`. Server: `crates/motorctl/src/grpc.rs`.
  Client: `python/backpack/grpc_client.py` (STUB).
- **motorctl ↔ hub (LEGO firmware):** LWP3 over BLE GATT char
  `00001624-...`. Encoder: `crates/motorctl/src/lwp3.rs`.
- **motorctl ↔ hub (Pybricks firmware):** Pybricks Code v2 over BLE GATT
  char `c5f50002-...`. `crates/motorctl/src/pybricks.rs` is currently a
  STUB; loading skills returns `not yet implemented`.

## Conventions

- **Default model: `claude-opus-4-7`.** Adaptive thinking, `output_config={"effort": "high"}`.
  Don't silently downgrade. The user decides cost/quality tradeoffs.
- **Vision tool uses `claude-haiku-4-5`** for image captioning. Fast, cheap,
  good enough for a perception layer.
- **Stub markers are explicit and greppable:** `STUB`, `TODO:`, or a function
  that returns `Err("not yet implemented")`. Don't paper over a stub with a
  mock that looks real — future readers must be able to find what's
  unfinished.
- **Don't break the IPC contract.** The same JSON commands run against
  motorctl, sim_daemon, and (once the gRPC client lands) the network.
  Adding new commands is fine; changing existing ones cascades.
- **Skills don't import anthropic.** Only `agent.py` and `perception.py` do.
  Skills are plain Python that work with any orchestrator (or none).
- **No ANTHROPIC_API_KEY in tests.** Smoke tests validate Skills +
  sim_daemon, not Cortex + sim_daemon. Cortex-in-loop is manual for now.
- **Don't add backwards-compat shims for stubs.** When a stub becomes real,
  tighten the interface; don't keep two code paths.

## Known stubs (also flagged at the top of each file)

1. `crates/motorctl/src/pybricks.rs` — Pybricks Code v2 BLE wire protocol.
   **Gates real skill upload to hardware.** Two implementation paths:
   port the relevant bits of `pybricksdev` (Python), or wrap the
   `pybricksdev` CLI behind `tokio::process::Command` for a quicker first cut.
2. `crates/reflex/src/main.rs` — watchdog / tilt / bumper triggers.
   **Gates safe physical deployment.**
3. `python/backpack/grpc_client.py` — needs grpc-tools-generated stubs and
   a class mirroring `MotorCtl`. Then update `open_motorctl()` in `ipc.py`
   to dispatch on URL scheme. **Gates remote cortex.**
4. Streaming RPCs in `crates/motorctl/src/grpc.rs`
   (`StreamSkillEvents`, `StreamSensorEvents`) — daemon needs an event bus
   first.
5. `twin/twin/distill.py` — only `diff_drive_pid` distillation works
   (R²=1.0 on synthetic recovery). Balance distillation is a different
   model class (state-feedback / LQR); general tinyMLP exporter is
   future work. Both raise `NotImplementedError` today.
6. `twin/assets/robot.xml` — generic skid-steer model. Sim-to-real fidelity
   needs calibration against the user's actual LEGO build (wheel mass /
   friction, motor torque/speed curves, IMU noise).

## Branch protocol

- **Active branch:** `claude/lego-robot-raspberry-pi-2BJ6f`. All backpack
  work lands here.
- **Don't merge into `master`** without asking. This is a data-science
  course repo; the owner may not want experimental robot scaffolding
  mixed in. Treat backpack as a long-lived feature branch.

## Hardware reference

- **Cortex host:** Raspberry Pi 5 (4 GB or 8 GB) with built-in BLE, plus
  Pi Camera Module 3 for `look()`. Or any Linux box on the same network
  in amygdala mode.
- **Amygdala (optional):** Raspberry Pi Zero 2 W on the chassis. Pure
  gateway — too small to host the cortex itself.
- **Brick:** LEGO 51515 hub (SPIKE Prime / Mindstorms Robot Inventor).
  Pybricks firmware required for muscle-memory upload; stock LEGO
  firmware works fine for command-only mode.

Full power / mounting / pairing notes in `docs/hardware.md`.
Amygdala-specific deployment in `docs/amygdala.md`.
Virtual testing in `twin/TESTING.md`.
