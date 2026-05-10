# The amygdala

A Pi Zero 2 W on the robot chassis acting as the **BLE-to-WiFi gateway**
between the LEGO hub and the cortex. Named for the brain region: it
sits between conscious thought (the cortex) and muscle (the brick),
owning fast safety-critical reflexes that don't have time to wait for
the cortex to weigh in.

## Why

Without the amygdala, the cortex has to ride on the robot — BLE range
is a few metres, bandwidth caps around 1–2 Mbps, and the link is
brittle. With the amygdala:

- **The cortex can run anywhere.** Laptop, beefier Pi, cloud GPU.
  Whatever has CPU + an Anthropic API key. The amygdala terminates
  BLE locally and exposes the same API over gRPC/WiFi at LAN latency.
- **There's a place for safety reflexes.** The reflex daemon owns
  motor stops on cortex-watchdog timeout, tilt-over, and bumper
  triggers. The cortex can lag, crash, or lose internet — the robot
  still stops itself within ~50 ms.
- **WiFi >> BLE for telemetry.** Camera frames, sensor streams, and
  agent traces fit comfortably on WiFi; they don't on BLE.

## Hardware

- **Pi Zero 2 W** — quad-core A53 @ 1 GHz, 512 MB RAM, WiFi + BLE.
  Plenty for motorctl + reflex; not enough for the cortex itself.
- **Wired-up to the LEGO build** as a small backpack on the chassis.
  The Zero powers off USB; the LEGO hub powers itself.
- Optional: **camera on the Zero's CSI port** instead of cortex-side.
  Lower latency for `look()`, but the Zero has to do the streaming.

## Topology

```
  +---------------------------+
  |  Cortex (anywhere)        |
  |  Python + Claude          |
  |  - speaks gRPC over TLS   |
  |  - holds heartbeat stream |
  +-------+-----------+-------+
          |           |
   gRPC (BrickLink)   gRPC (Reflex)
          |           |
          v           v
  +---------------------------+
  |   amygdala (Pi Zero 2 W)  |
  |                           |
  |   motorctl  (Rust)        |
  |    │  Unix socket /run/.. |
  |    │  gRPC :50051         |
  |    └─ BLE ──┐             |
  |              │             |
  |   reflex    │ (safety)    |
  |    │  gRPC :50052         |
  |    │  watchdog 500 ms     |
  |    └───────┼             |
  +-------------|-------------+
                | BLE
                v
  +---------------------------+
  |   LEGO 51515 hub          |
  |   (Pybricks fw + skill)   |
  +---------------------------+
```

## Protocol

Schema: [`proto/backpack.proto`](../proto/backpack.proto). Two services:

- **BrickLink** — mirrors the existing Unix-socket motorctl protocol
  (`SetSpeed`, `RunForDegrees`, `Stop`, `Brake`, `GetStatus`,
  `LoadSkill`, `UnloadSkill`, `SkillMessage`, `StreamSkillEvents`,
  `StreamSensorEvents`). Anything that works on the Unix socket
  works over gRPC.
- **Reflex** — a `Heartbeat` bidirectional stream the cortex *must*
  keep alive. The first ping carries the watchdog deadline; each pong
  reports `{armed, last_trigger, deadline_ms, last_seen_ms}`. If the
  cortex doesn't ping in time, the reflex brakes all motors and
  reports `last_trigger="watchdog_timeout"` on reconnect.

The cortex's `MotorCtl` client is transport-agnostic by design —
`open_motorctl("unix://...")` and `open_motorctl("grpc://...")`
should be drop-in equivalents.

## Security

This is a robot on your home network with motors that can hurt people
or break things. Treat it accordingly:

- gRPC over **TLS only**. The install script generates a self-signed
  cert; replace with a real one for anything beyond a lab.
- The cortex pins the amygdala's certificate (planned). No
  bring-your-own-cert from the Internet.
- The reflex is the last line of defence — even if the cortex link
  is hijacked, the watchdog stops the robot when heartbeats stop.
- Do **not** expose the gRPC ports to the public Internet without
  putting them behind a VPN / Tailscale. The protocol assumes the
  caller is trusted.

## Deployment

On the Zero:

```sh
git clone https://github.com/blujus/DAT4.git
cd DAT4/backpack
./scripts/install_amygdala.sh
sudo systemctl start motorctl reflex
```

On the cortex host:

```sh
export MOTORCTL_URL="grpc://amygdala.local:50051"
export REFLEX_URL="grpc://amygdala.local:50052"
python -m backpack 'find the red brick on the floor and stop next to it'
```

## Status

The shape is committed (proto, crate skeletons, install script,
systemd units). Real implementation is the next step:

1. `crates/motorctl/src/grpc.rs` — wire `tonic` to `Daemon`.
2. `crates/reflex/src/main.rs` — watchdog + sensor triggers.
3. `python/backpack/grpc_client.py` — mirror the `MotorCtl` surface.
4. Heartbeat client glue in `python/backpack/agent.py` — keep the
   Reflex stream alive while the cortex runs.
