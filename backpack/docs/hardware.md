# Hardware

## Bill of materials

| Part                                    | Notes                              |
|-----------------------------------------|------------------------------------|
| Raspberry Pi 5 (4 GB or 8 GB)           | Built-in BLE; the cortex.          |
| LEGO 51515 hub (Robot Inventor / SPIKE) | 6 LPF2 ports, internal battery.    |
| USB-C 5 V / 5 A PSU for the Pi          | Independent of the hub's battery.  |
| MicroSD or NVMe                         | NVMe HAT works if it fits.         |
| LEGO Technic Large / Medium motors      | LPF2 motors (45602 / 88008 etc).   |
| LEGO sensors as needed                  | Colour, distance, force.           |
| Pi Camera Module 3 (or similar)         | Required for `look()` perception.  |

The hub powers itself and the motors from its internal battery; the Pi
is powered separately. They are coupled only by BLE.

## Mounting ("the backpack")

The Pi rides on top of the robot like a backpack. Two design rules:

1. **Keep the hub central.** The 51515 hub is heavy and houses the
   battery; treat it as the centre of mass and build outward.
2. **Pi on a quick-release frame.** A LEGO Technic frame around the Pi
   with two 4M pins lets you pop the Pi off without disassembling the
   robot — useful when you're flashing the SD card or reseating the
   camera ribbon.

## Hub firmware — LEGO vs Pybricks

The 51515 hub can run two different firmwares, and which one you choose
decides what the cortex can do.

| Firmware          | Motor commands | "Muscle memory" upload | Bluetooth messaging |
|-------------------|---------------:|-----------------------:|--------------------:|
| Stock LEGO        | ✅ (LWP3)      | ❌                     | ❌                  |
| Pybricks          | ✅ (Pybricks)  | ✅                     | ✅                  |

**Stock LEGO firmware** (what the hub ships with) only accepts LWP3
motor commands. The cortex can drive motors and read sensors, but it
cannot push new control loops onto the hub.

**[Pybricks](https://pybricks.com/)** is open-source community firmware
for the same hub. It runs MicroPython on-hub, accepts BLE program
uploads ("Code v2"), and exposes a bidirectional Bluetooth messaging
channel. This is what the cortex needs to push *muscle memory* — small
closed-loop skills that run at 100–500 Hz on the hub itself.

Flashing Pybricks is reversible: you can flash back to stock LEGO
firmware with the Robot Inventor / SPIKE app any time. Use the
[Pybricks Code](https://code.pybricks.com/) web IDE (Chrome / Edge for
Web Bluetooth) to install it. Pick "SPIKE Prime hub" or "MINDSTORMS
Robot Inventor hub" and follow the on-screen flow.

`motorctl` auto-detects which firmware is running and refuses skill
operations cleanly when the hub is on stock LEGO firmware.

## Pairing the hub

The hub advertises over BLE as soon as you press the power button. There
is no PIN. `motorctl` discovers it by name, defaulting to anything
containing "lego", "spike", "technic", "mindstorms", or "pybricks";
pass `--name SPIKE` (or similar) on the command line if you have
several hubs in range.

If you've previously paired the hub with a phone or laptop, that
device will fight the Pi for the connection. Power the hub off and
back on with only the Pi listening, and the Pi will win.

## Power

- Don't try to power the Pi off the hub or vice-versa. They live on
  separate power planes.
- The hub reports battery voltage in LWP3 `Hub Properties` notifications
  (or via Pybricks); `motorctl` will eventually surface this on the
  IPC `status` response so the cortex can throttle missions when the
  battery sags.
