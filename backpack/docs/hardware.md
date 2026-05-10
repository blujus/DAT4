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

## Pairing the hub

The hub advertises over BLE as soon as you press the power button. There
is no PIN. `motorctl` discovers it by name, defaulting to anything
containing "lego", "spike", "technic" or "mindstorms" — pass
`--name SPIKE` (or similar) on the command line if you have several
hubs in range.

If you've previously paired the hub with a phone or laptop, that
device will fight the Pi for the connection. Power the hub off and
back on with only the Pi listening, and the Pi will win.

## Power

- Don't try to power the Pi off the hub or vice-versa. They live on
  separate power planes.
- The hub reports battery voltage in LWP3 `Hub Properties` notifications;
  `motorctl` will eventually surface this on the IPC `status` response
  so the cortex can throttle missions when the battery sags.

## Firmware

The scaffold targets stock LEGO firmware on the 51515 hub. It does *not*
require Pybricks. If you have flashed Pybricks onto the hub, the LWP3
GATT service is replaced by the Pybricks service — a future addition
to `motorctl` could speak both, but today, stay on the LEGO firmware.
