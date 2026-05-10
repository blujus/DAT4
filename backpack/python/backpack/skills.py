"""Differential-drive motion primitives.

These compose the brick's `run_for_degrees` calls (delivered via
motorctl) into the moves the cortex actually wants to reason about:
drive forward N centimetres, turn N degrees in place, arc with a given
radius. Encoder math assumes a standard Technic wheel and a measurable
wheelbase — tune `DriveConfig` for your robot.
"""

from __future__ import annotations

import math
import time
from dataclasses import dataclass

from .ipc import MotorCtl


@dataclass
class DriveConfig:
    """Per-robot calibration. Override the fields that don't match your build."""

    left_port: int = 0          # LEGO port A
    right_port: int = 1         # LEGO port B
    wheel_diameter_mm: float = 56.0
    wheelbase_mm: float = 120.0
    invert_left: bool = False
    invert_right: bool = True   # mirrored motor mounting is the common case


def _mm_per_degree(cfg: DriveConfig) -> float:
    return math.pi * cfg.wheel_diameter_mm / 360.0


class Skills:
    """Wraps a `MotorCtl` client with differential-drive primitives."""

    def __init__(self, motor: MotorCtl, cfg: DriveConfig | None = None) -> None:
        self.motor = motor
        self.cfg = cfg or DriveConfig()

    def drive(self, distance_cm: float, speed: float = 0.4) -> None:
        """Straight-line motion. Negative distance reverses."""
        sign = 1 if distance_cm >= 0 else -1
        deg = abs(distance_cm * 10.0) / _mm_per_degree(self.cfg)
        self._run_pair(int(deg), sign * speed, sign * speed)

    def turn(self, degrees: float, speed: float = 0.3) -> None:
        """Pivot in place. Positive = clockwise (right turn)."""
        arc_mm = abs(degrees) * math.pi / 180.0 * self.cfg.wheelbase_mm / 2.0
        deg = arc_mm / _mm_per_degree(self.cfg)
        sign = 1 if degrees >= 0 else -1
        self._run_pair(int(deg), sign * speed, -sign * speed)

    def arc(self, radius_cm: float, angle_deg: float, speed: float = 0.3) -> None:
        """Curved path. Positive radius curves right, negative curves left.
        Positive angle drives forward around the arc; negative drives back.
        """
        if abs(radius_cm) < 0.01:
            return self.turn(angle_deg, speed)
        r_mm = abs(radius_cm) * 10.0
        sweep = math.radians(abs(angle_deg))
        outer_mm = (r_mm + self.cfg.wheelbase_mm / 2.0) * sweep
        inner_mm = max(0.0, r_mm - self.cfg.wheelbase_mm / 2.0) * sweep
        inner_speed = speed * (inner_mm / outer_mm) if outer_mm > 0 else 0.0
        if radius_cm > 0:
            left, right = speed, inner_speed
        else:
            left, right = inner_speed, speed
        if angle_deg < 0:
            left, right = -left, -right
        outer_deg = outer_mm / _mm_per_degree(self.cfg)
        self._run_pair(int(outer_deg), left, right)

    def stop(self) -> None:
        self.motor.stop(self.cfg.left_port)
        self.motor.stop(self.cfg.right_port)

    def _run_pair(self, encoder_deg: int, left_speed: float, right_speed: float) -> None:
        ls = -left_speed if self.cfg.invert_left else left_speed
        rs = -right_speed if self.cfg.invert_right else right_speed
        self.motor.run_for_degrees(self.cfg.left_port, encoder_deg, ls)
        self.motor.run_for_degrees(self.cfg.right_port, encoder_deg, rs)
        # Estimate completion based on a nominal 1000 deg/sec at speed 1.0.
        # The brick is doing the actual control loop — we just need to wait
        # roughly the right amount before issuing the next move. Replace
        # with motorctl completion events once we wire those through.
        avg_speed = max((abs(ls) + abs(rs)) / 2.0, 0.05)
        eta = encoder_deg / (1000.0 * avg_speed) + 0.2
        time.sleep(min(eta, 30.0))
