"""CLI entry point for the cortex.

Usage:
    python -m backpack 'follow me'
    python -m backpack 'drive in a circle of radius 50 cm'
    python -m backpack 'find the red brick on the floor and stop next to it'

Needs:
    ANTHROPIC_API_KEY  in the environment
    motorctl           running and paired with the LEGO hub
"""

from __future__ import annotations

import os
import sys

from .agent import Cortex
from .ipc import open_motorctl


def main() -> None:
    if len(sys.argv) < 2:
        print("usage: python -m backpack '<instruction>'", file=sys.stderr)
        raise SystemExit(2)
    if not os.environ.get("ANTHROPIC_API_KEY"):
        print("error: ANTHROPIC_API_KEY is not set", file=sys.stderr)
        raise SystemExit(2)

    instruction = " ".join(sys.argv[1:])
    socket_path = os.environ.get("MOTORCTL_SOCKET", "/run/motorctl.sock")
    with open_motorctl(socket_path) as motor:
        Cortex(motor).act(instruction)


if __name__ == "__main__":
    main()
