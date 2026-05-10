# Architecture

```
     user goal: "follow me" / "learn to walk like a crab"
              |
              v
  +-----------+--------------------------------------+
  |  Cortex (Python, Claude Opus 4.7)                |
  |  - tool runner: drive, turn, arc, stop,          |
  |                 look, get_status,                |
  |                 load_skill, skill_message        |
  |  - adaptive thinking, effort=high                |
  +-----+---------+-----------+----------------------+
        |         |           |
        |         |           | upload skill / send msg
        |         |           v
        |         |    +------+--------------------+
        |         |    |  motorctl (Rust)          |
        |         |    |  - LWP3 motor commands    |
        |         |    |  - Pybricks code-load     |  (BLE)  +------------------+
        |         |    |  - Pybricks BLE messaging |  ----▶  |  LEGO 51515 hub  |
        |         |    +------+--------------------+         |  (Pybricks fw)   |
        |         |           ^                              |  + muscle-memory |
        |         |           |                              |    skill on-hub  |
        |         v           |                              +--------+---------+
        |    +----+---------------------+                              |
        |    |  Perception (Python)     |                              | LPF2
        |    |  Pi camera + Haiku 4.5   |                              v
        |    +--------------------------+                          motors / sensors
        v
  +-----+--------------+
  |  Skills (Python)   |   diff-drive primitives layered on motorctl,
  |  diff-drive maths  |   used by tools above when no on-hub skill
  +--------------------+   needs to take over.


  ----- offline / training loop -----------------------------------------

            twin/  (MuJoCo + Gymnasium)
              |
              +--- sim_daemon (motorctl-compatible IPC)  -- cortex evals
              |
              \--- BackpackEnv (gym.Env)  -- PPO --▶ policy
                                                       |
                                              twin/distill.py
                                                       |
                                                       v
                                          skills_lib/<name>.py
                                                       |
                                                       v
                                            cortex.load_skill(...)
```

## Three time scales

The split is fundamentally about which loop runs at which rate:

| Loop                     | Rate       | Where it lives          |
|--------------------------|------------|-------------------------|
| Cognitive cortex         | 0.1–1 Hz   | Pi (Python, Claude)     |
| Skills (diff-drive math) | 10–100 Hz  | Pi (Python)             |
| Muscle memory            | 50–500 Hz  | LEGO hub (Pybricks)     |
| Inner motor PID          | ~1 kHz     | LEGO hub firmware       |

Moving a loop *down* the stack (cortex → skills → muscle memory) trades
flexibility for tightness. Things that benefit from being closer to the
motors — balance, gait, fine speed control — should ride on muscle
memory; things that benefit from being closer to the world model —
planning, vision, mission goals — belong in the cortex.

## Wire protocol (cortex ↔ motorctl)

Newline-delimited JSON over a Unix domain socket. Requests are tagged
with `cmd`; responses with `ok`. Schema lives in
`crates/motorctl/src/proto.rs`.

Direct motor control:
```
-> {"cmd":"run_for_degrees","port":0,"degrees":360,"speed":0.5}
<- {"ok":"ack"}
```

Muscle-memory upload:
```
-> {"cmd":"load_skill","name":"diff_drive_pid","code":"..."}
<- {"ok":"ack"}
-> {"cmd":"skill_message","payload":"target 0.4 0.0"}
<- {"ok":"ack"}
```

Status reports firmware family and the running skill so the cortex
knows what it's working with:
```
-> {"cmd":"status"}
<- {"ok":"status","ports":[...],"firmware":"pybricks","current_skill":"diff_drive_pid"}
```

## Wire protocol (motorctl ↔ hub)

- **LEGO firmware:** LEGO Wireless Protocol 3 (LWP3) over BLE. Single
  service `00001623-...`, single read/write/notify characteristic
  `00001624-...`. Command-only — you cannot push code to the hub.
- **Pybricks firmware:** distinct service `c5f50001-...` with control
  + capabilities characteristics. Supports BLE program upload
  ("Code v2") and bidirectional Bluetooth messaging. The motorctl
  daemon detects which firmware the hub is running at connect time
  and routes commands accordingly.

## Roadmap

1. Finish the Pybricks Code v2 BLE wire protocol in
   `crates/motorctl/src/pybricks.rs` (currently stubbed). Either port
   the relevant bits of `pybricksdev` (Python) or wrap the
   `pybricksdev` CLI behind `tokio::process::Command` for a quicker
   first cut.
2. Surface motor-completion + sensor events on the IPC channel so
   `Skills` can wait on "move done" and the cortex can subscribe to
   colour / distance / force readings without polling.
3. Replace `twin/assets/robot.xml` with a model that matches your
   actual LEGO build, including motor torque/speed curves, friction,
   and IMU noise. Sim-to-real fidelity will live or die by this.
4. Implement real distillation in `twin/twin/distill.py` (lookup
   table or fitted PID, then tinyMLP for the harder skills).
5. Cortex memory across sessions via the Anthropic memory tool, so
   what the robot learned ("the kitchen is right", "the red brick is
   30 cm tall", "diff_drive_pid v3 wobbles on hardwood") persists.
