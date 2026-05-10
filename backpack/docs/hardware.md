# Hardware

## Bill of materials

| Part                                | Notes                                  |
|-------------------------------------|----------------------------------------|
| Raspberry Pi 5 (4 GB or 8 GB)       | The brain.                             |
| Raspberry Pi Build HAT              | LPF2 connector + STM32 co-processor.   |
| 8 V / 48 W barrel-jack PSU          | Powers the BuildHAT motor rail.        |
| USB-C 5 V / 5 A PSU for the Pi      | Don't try to power the Pi off the HAT. |
| MicroSD or NVMe                     | NVMe HAT is great if you have room.    |
| LEGO Technic Large / Medium motors  | LPF2 motors only (45602 / 88008 etc).  |
| LEGO sensors as needed              | Colour, distance, force.               |

## Mounting ("the backpack")

The Pi + BuildHAT stack sits on the robot like a backpack. Two design
goals:

1. **LPF2 cables stay short.** The HAT's four ports should face outward
   so cables can drop straight to the motors. Long cable runs around the
   chassis make wiring fragile.
2. **Serviceable.** The Pi should come off without disassembling the
   robot. A LEGO Technic frame around the HAT with two 4M pins works
   well; the Pi slides in and out.

A reference frame is in `docs/` (TBD: add the .io / .ldr file).

## UART notes (Pi 5 specific)

- The BuildHAT lives on `/dev/serial0`, which on the Pi 5 is the primary
  PL011 UART exposed on the GPIO header.
- Disable the serial console (`raspi-config` -> Interface Options ->
  Serial Port -> login shell `No`, hardware `Yes`). The install script
  does this for you.
- On first power-on, the BuildHAT's STM32 firmware loads from the Pi.
  Expect a ~3 s pause before commands are accepted.

## Power

- Don't backfeed the Pi from the HAT and don't backfeed the HAT from the
  Pi. Use both supplies.
- The HAT measures input voltage (`vin` command); `motorctl` will
  eventually surface this on the IPC `status` response so the
  orchestrator can throttle when the battery sags.
