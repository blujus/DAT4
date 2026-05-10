"""High-level cortex for the backpack robot.

The LEGO 51515 hub owns motor control. The Rust `motorctl` daemon owns
the BLE/LWP3 link to the hub. This package owns the cognitive layer:
perception, planning, and turning natural-language goals into the right
sequence of motor and perception calls.
"""

from .agent import Cortex
from .ipc import MotorCtl, open_motorctl
from .skills import DriveConfig, Skills

__all__ = [
    "Cortex",
    "DriveConfig",
    "MotorCtl",
    "Skills",
    "open_motorctl",
]
