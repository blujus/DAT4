"""Tiny example cortex: spin port A one revolution forward, then back.

Run with `python -m backpack.orchestrator` once `motorctl` is up and
paired with the hub.
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
