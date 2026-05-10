"""Tiny example orchestrator: drives motors on ports 0 and 1 forward
for a second, then stops. Run with `python -m backpack.orchestrator`.
"""

from __future__ import annotations

import time

from .ipc import open_motorctl


def main() -> None:
    with open_motorctl() as m:
        print("status:", m.status())
        m.set_speed(0, 0.5)
        m.set_speed(1, 0.5)
        time.sleep(1.0)
        m.stop(0)
        m.stop(1)


if __name__ == "__main__":
    main()
