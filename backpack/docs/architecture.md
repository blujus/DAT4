# Architecture

```
  +-------------------------------+
  |  Python orchestrator          |
  |  - vision / planning / REPL   |
  |  - mission scripts            |
  +---------------+---------------+
                  | JSON-over-Unix-socket
                  v
  +---------------+---------------+
  |  motorctl (Rust daemon)       |
  |  - BuildHAT serial driver     |
  |  - real-time control loop     |
  |  - IPC server                 |
  +---------------+---------------+
                  | UART /dev/serial0 @ 115200
                  v
  +-------------------------------+
  |  Raspberry Pi Build HAT       |
  |  - 4 LPF2 ports               |
  |  - own MCU + firmware         |
  +---------------+---------------+
                  | LPF2 cables
                  v
  +-------------------------------+
  |  LEGO motors & sensors        |
  +-------------------------------+
```

## Why two processes

A single Python process on the Pi is tempting but risky: GC pauses and the
GIL produce jitter that shows up as wobble in the motors and missed encoder
ticks. Splitting the control loop into a Rust daemon means:

- Python can pause for hundreds of milliseconds doing vision and the robot
  still tracks straight.
- The Rust side can later run a 1 kHz PID loop on a pinned thread without
  fighting the Python runtime.
- The IPC boundary doubles as a debugging surface: any tool that can write
  a line to a Unix socket can drive the robot.

## Wire protocol

Newline-delimited JSON. Requests are tagged with a `cmd` field; responses
are tagged with an `ok` field (`"ack"`, `"status"`, or `"err"`).

```
-> {"cmd":"set_speed","port":0,"speed":0.5}
<- {"ok":"ack"}

-> {"cmd":"status"}
<- {"ok":"status","ports":[{"port":0,"device":null,"last_speed":0.5}, ...]}
```

The canonical schema lives in `crates/motorctl/src/proto.rs`.

## Roadmap

1. Sensor streaming on the IPC channel (subscribe to `port N` events).
2. Closed-loop position / velocity control inside `motorctl`.
3. Pluggable behaviour modules in Python (line-follow, obstacle-avoid).
4. ROS 2 bridge for those who want it; the Unix-socket protocol stays the
   ground truth.
