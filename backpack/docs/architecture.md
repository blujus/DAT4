# Architecture

```
     user: "follow me" / "move in a circle" / "pick that thing up"
              |
              v
  +-----------+--------------------------------------+
  |  Cortex (Python, Claude Opus 4.7)                |
  |  - tool runner: drive, turn, arc, stop,          |
  |                 look, get_status                  |
  |  - adaptive thinking, effort=high                |
  +-----+----------------------------+---------------+
        | motion tools               | perception tool
        v                            v
  +-----+--------------+      +------+-------------------+
  |  Skills (Python)   |      |  Perception (Python)     |
  |  diff-drive maths  |      |  picamera2 + Haiku 4.5   |
  +-----+--------------+      +--------------------------+
        | JSON over Unix socket
        v
  +-----+--------------+
  |  motorctl (Rust)   |
  |  BLE LWP3 link     |
  +-----+--------------+
        | Bluetooth LE GATT char
        v
  +-----+--------------+
  |  LEGO 51515 hub    |
  |  PID, encoders,    |
  |  6 LPF2 ports      |
  +-----+--------------+
        | LPF2
        v
     motors & sensors
```

## The cortex's job

The agent doesn't see ports, encoder degrees, or LWP3 frames. It sees:

- **drive(distance_cm, speed)** — straight-line motion
- **turn(degrees, speed)** — rotate in place; positive = clockwise
- **arc(radius_cm, angle_deg, speed)** — curved path
- **stop()** — halt all motion
- **look(prompt)** — take a frame from the Pi camera and get a textual
  description (Claude Haiku 4.5 captions the JPEG)
- **get_status()** — last commanded speed on each port

This is a small, deliberately stable surface. It is what "the brick" looks
like *to a planning agent*: the body is concrete, but the brick handles
everything below the level of *"go forward 30 cm"*.

A goal like *"follow me"* becomes:

1. `look()` — where is the user?
2. small `arc()` to keep them centred
3. `drive()` a short distance
4. back to step 1, until the goal stops being interesting

Claude decides the loop, the threshold, when to stop, when to ask. We
don't hand-code that.

## Why three processes

A single Python process talking BLE directly is the obvious thing to do,
and for a slow demo it works. But:

- BLE writes from `bleak` block on the asyncio loop. A GC pause or a
  slow vision frame translates into a stutter at the motors.
- The hub doesn't queue beyond its small input buffer. Missing the
  rhythm of `StartSpeed` updates makes the robot drift.
- Hot-reloading the cortex (the whole point of using Python) drops the
  BLE link with it, which forces a reconnect dance.

Moving the BLE link into a tiny Rust daemon means the cortex can be
restarted, hung, profiled or replaced without dropping the hub
connection, and the link itself runs without GC.

Moving vision into a separate per-call process boundary (a `look()`
tool that internally talks to Claude vision) means the cortex doesn't
block its own reasoning loop on camera I/O.

## Wire protocol (cortex ↔ motorctl)

Newline-delimited JSON over a Unix domain socket. Requests are tagged
with `cmd`; responses with `ok`.

```
-> {"cmd":"run_for_degrees","port":0,"degrees":360,"speed":0.5}
<- {"ok":"ack"}

-> {"cmd":"status"}
<- {"ok":"status","ports":[{"port":0,"device":null,"last_speed":0.5}, ...]}
```

Canonical schema lives in `crates/motorctl/src/proto.rs`.

## Wire protocol (motorctl ↔ hub)

LEGO Wireless Protocol 3 (LWP3) over BLE. The hub exposes a single
GATT service (`00001623-...`) with one read/write/notify characteristic
(`00001624-...`).

The daemon currently emits Port Output Commands:

- `StartPower` (0x01) — used for coast (power=0) and brake (power=127)
- `StartSpeed` (0x07) — used for `set_speed`
- `StartSpeedForDegrees` (0x0B) — used for `run_for_degrees`

The encoder for these lives in `crates/motorctl/src/lwp3.rs`, with
unit tests asserting the byte-for-byte wire format.

## Roadmap

1. Decode `Hub Attached I/O` notifications so `status` reports which
   motor / sensor is on each port (and the cortex knows what's
   physically connected).
2. Surface motor-completion events on the IPC channel so `Skills` can
   wait on "move done" rather than a sleep estimate.
3. Sensor streaming — expose colour / distance / force as live values
   the cortex can subscribe to.
4. Memory across sessions: persist what the robot learned ("the kitchen
   is to the right", "the red brick is 30 cm tall") via the Anthropic
   memory tool.
5. Multi-modal `look()` — return the image to the cortex directly so it
   can reason on pixels, not on a caption.
