"""High-level orchestration layer for the backpack robot.

The heavy lifting lives in the Rust `motorctl` daemon; this package is a
thin client plus the mission-level glue (vision, planning, REPL).
"""

from .ipc import MotorCtl

__all__ = ["MotorCtl"]
