# Muscle memory: skills that run on the brick

This directory holds small Python programs that the Pi cortex can push
to the LEGO hub at runtime. Each file is a self-contained "skill" — a
local control loop the brick runs in real time, without paying the BLE
round-trip cost of every motor update.

Think of these the way a person thinks of muscle memory: high-frequency,
closed-loop, sub-second-reaction stuff that the conscious cortex would
be terrible at. Examples: balance reflexes, fine diff-drive PID, gait
cycles, force-feedback grippers.

## Hub firmware: Pybricks

**Skills require [Pybricks](https://pybricks.com/) firmware on the
hub.** Stock LEGO firmware can be *commanded* (via LWP3) but cannot be
*reprogrammed* over BLE. Pybricks is open-source firmware for the same
hub that:

- speaks BLE program-load ("Code v2")
- runs MicroPython on-hub with a documented `pybricks` API
- exposes a Bluetooth messaging channel back to the Pi for live
  cortex/skill communication
- is fully reversible — flash back to stock LEGO firmware any time

Flash with the [Pybricks Code](https://code.pybricks.com/) web IDE
(needs Chrome / Edge for Web Bluetooth). Pick "SPIKE Prime hub" or
"MINDSTORMS Robot Inventor hub" and follow the on-screen instructions.

The `motorctl` daemon auto-detects which firmware the hub is running
and will refuse `load_skill` calls cleanly if it sees stock LEGO
firmware.

## How a skill is structured

A skill is a Python file targeting Pybricks. It typically:

1. Imports motors / sensors via `pybricks.pupdevices` and the hub's
   `pybricks.hubs.PrimeHub`.
2. Sets up its own control loop — PID, state machine, whatever.
3. Listens for messages from the Pi cortex via
   `hub.ble.observe(channel)` (or the equivalent on your Pybricks
   version). Messages are short strings the cortex sends with
   `motor.skill_message("...")`.
4. Runs forever (or until told to stop) in a `while True:` loop.

See [`template.py`](template.py) for a minimal skeleton, and
[`diff_drive_pid.py`](diff_drive_pid.py) / [`balance.py`](balance.py)
for more substantial examples.

## How a skill gets loaded

From the Pi:

```python
with open_motorctl() as m:
    m.load_skill("diff_drive_pid", Path("skills_lib/diff_drive_pid.py"))
    m.skill_message("target 0.4 0.0")   # 0.4 m/s forward, 0 rad/s yaw
    ...
    m.unload_skill()
```

From the cortex (Claude calls these tools itself):

```
load_skill("diff_drive_pid")
skill_message("target 0.4 0.0")
```

## Where new skills come from

Three paths, listed by amount of human effort required:

1. **Hand-written** — the same way you'd hand-write any small Python
   controller. Good for things you can specify directly: a PID, a
   gait, an emergency-stop reflex.
2. **RL-trained in the digital twin** — train a policy in `twin/` (see
   the `twin/README.md` for the gym/MuJoCo env), then distill the
   policy to a small lookup table or PID + scheduling layer that fits
   on the hub. Helper: `twin/twin/distill.py`.
3. **Cortex-authored** — the cortex can write a new skill from scratch
   using its tools, save it to `skills_lib/`, then `load_skill(...)`
   it. This isn't wired up by default (it's a footgun) but the IPC
   surface supports it.

## Status

The IPC surface (`load_skill` / `unload_skill` / `skill_message`) is
fully plumbed end to end, but the BLE Code v2 wire protocol in
`crates/motorctl/src/pybricks.rs` is currently a stub returning
"not implemented". To finish the path, port the upload state machine
from [`pybricksdev`](https://github.com/pybricks/pybricksdev) (Python),
or wrap the `pybricksdev` CLI behind `tokio::process::Command` for a
quicker first cut.
