# Architecture

```
  +---------------------------------------+
  |  Python cortex                        |
  |  - vision / planning / mission DSL    |
  |  - REPL, scripts, notebooks           |
  +-------------------+-------------------+
                      | JSON over Unix socket
                      v
  +-------------------+-------------------+
  |  motorctl (Rust, on the Pi)           |
  |  - BLE LWP3 link to the hub           |
  |  - command queue + state mirror       |
  |  - IPC server                         |
  +-------------------+-------------------+
                      | BLE, LWP3 GATT char
                      v
  +---------------------------------------+
  |  LEGO 51515 hub                       |
  |  - inner motor control loop (PID)     |
  |  - encoders, stall detection          |
  |  - power, battery, 6 LPF2 ports       |
  +-------------------+-------------------+
                      | LPF2 cables
                      v
  +---------------------------------------+
  |  LEGO motors & sensors                |
  +---------------------------------------+
```

## Why three layers, not one

A single Python process talking BLE directly is the obvious thing to do,
and for a slow demo it works. But:

- BLE writes from `bleak` block on the asyncio loop. Any GC pause or a
  slow vision frame translates into a stutter at the motors.
- The hub doesn't queue beyond its small input buffer. If we miss the
  rhythm of `StartSpeed` updates, the robot drifts.
- Hot-reloading Python (the whole point of using it for the cortex)
  drops the BLE link with it.

Moving the BLE link into a tiny Rust daemon means the cortex can be
restarted, hung, profiled or replaced without dropping the hub
connection, and the link itself runs without GC.

## Wire protocol (cortex ↔ motorctl)

Newline-delimited JSON over a Unix domain socket. Requests are tagged
with `cmd`; responses with `ok`.

```
-> {"cmd":"set_speed","port":0,"speed":0.5}
<- {"ok":"ack"}

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
   motor / sensor is on each port.
2. Sensor streaming on the IPC channel (subscribe to port-value
   notifications and forward them as events).
3. Mission DSL in Python: small state machines composed of motor +
   sensor primitives.
4. Vision pipeline (Pi camera) feeding the cortex.
