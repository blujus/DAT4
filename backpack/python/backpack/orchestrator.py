"""Hardware smoke test — no LLM, just exercises the IPC and motors.

Run this first to confirm motorctl is up, the hub is paired, and motors
spin in the right direction. Once that works, switch to the cortex:

    python -m backpack 'drive forward 30 cm and stop'
"""

from __future__ import annotations

import time

from .ipc import open_motorctl


def main() -> None:
    with open_motorctl() as m:
        print("status:", m.status())
        m.run_for_degrees(port=0, degrees=360, speed=0.5)
        time.sleep(2.0)
        m.run_for_degrees(port=0, degrees=360, speed=-0.5)


if __name__ == "__main__":
    main()
