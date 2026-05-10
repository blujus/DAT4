"""MuJoCo wrapper around the robot model.

`Twin` is a thin facade: load the MJCF, hold the `mjModel` and `mjData`,
step physics, expose motor commands keyed by the same port numbers the
real `motorctl` uses (port 0 = left, port 1 = right by convention).
"""

from __future__ import annotations

import argparse
import math
import time
from dataclasses import dataclass
from pathlib import Path

import mujoco
import numpy as np

_MODEL_PATH = Path(__file__).resolve().parent.parent / "assets" / "robot.xml"


@dataclass
class MotorState:
    position_deg: float
    speed_dps: float


class Twin:
    """Single instance of the simulated robot."""

    PORT_TO_ACTUATOR = {0: "left_motor", 1: "right_motor"}
    PORT_TO_JOINT = {0: "left_wheel", 1: "right_wheel"}
    MAX_WHEEL_DPS = 1000.0  # deg/sec at speed=1.0

    def __init__(self, model_path: Path = _MODEL_PATH) -> None:
        self.model = mujoco.MjModel.from_xml_path(str(model_path))
        self.data = mujoco.MjData(self.model)
        self._actuator_ids = {
            port: mujoco.mj_name2id(self.model, mujoco.mjtObj.mjOBJ_ACTUATOR, name)
            for port, name in self.PORT_TO_ACTUATOR.items()
        }
        self._joint_ids = {
            port: mujoco.mj_name2id(self.model, mujoco.mjtObj.mjOBJ_JOINT, name)
            for port, name in self.PORT_TO_JOINT.items()
        }
        self._target_speeds: dict[int, float] = {p: 0.0 for p in self.PORT_TO_ACTUATOR}

    # --- physics ----------------------------------------------------------

    def reset(self, seed: int | None = None) -> None:
        if seed is not None:
            np.random.seed(seed)
        mujoco.mj_resetData(self.model, self.data)
        self._target_speeds = {p: 0.0 for p in self._target_speeds}

    def step(self, dt: float = 0.01) -> None:
        steps = max(1, int(round(dt / self.model.opt.timestep)))
        for _ in range(steps):
            mujoco.mj_step(self.model, self.data)

    # --- motorctl-compatible API -----------------------------------------

    def set_speed(self, port: int, speed: float) -> None:
        """Drive `port` at `speed` in [-1.0, 1.0]. Persists across steps."""
        speed = max(-1.0, min(1.0, speed))
        self._target_speeds[port] = speed
        rad_per_s = speed * self.MAX_WHEEL_DPS * math.pi / 180.0
        self.data.ctrl[self._actuator_ids[port]] = rad_per_s

    def stop(self, port: int) -> None:
        self.set_speed(port, 0.0)

    def run_for_degrees(self, port: int, degrees: int, speed: float) -> None:
        """Spin the wheel for ~degrees at `speed`, then hold.

        Simple integration: drives at the requested speed for the time
        the brick would take, then stops. The Gym env / sim daemon will
        usually use `set_speed` directly instead.
        """
        speed = max(-1.0, min(1.0, speed))
        if abs(speed) < 1e-3 or degrees == 0:
            self.stop(port)
            return
        self.set_speed(port, speed)
        eta = abs(degrees) / (self.MAX_WHEEL_DPS * abs(speed))
        self.step(eta)
        self.stop(port)

    def motor_state(self, port: int) -> MotorState:
        joint_id = self._joint_ids[port]
        qpos_addr = self.model.jnt_qposadr[joint_id]
        qvel_addr = self.model.jnt_dofadr[joint_id]
        return MotorState(
            position_deg=math.degrees(float(self.data.qpos[qpos_addr])),
            speed_dps=math.degrees(float(self.data.qvel[qvel_addr])),
        )

    def chassis_pose(self) -> tuple[float, float, float]:
        """Return (x, y, yaw_rad) of the chassis body."""
        body_id = mujoco.mj_name2id(self.model, mujoco.mjtObj.mjOBJ_BODY, "chassis")
        x, y, _ = self.data.xpos[body_id]
        # quaternion -> yaw
        w, qx, qy, qz = self.data.xquat[body_id]
        yaw = math.atan2(2.0 * (w * qz + qx * qy), 1.0 - 2.0 * (qy * qy + qz * qz))
        return float(x), float(y), float(yaw)


def _view() -> None:
    """Open the MuJoCo passive viewer for ad-hoc inspection."""
    import mujoco.viewer

    twin = Twin()
    with mujoco.viewer.launch_passive(twin.model, twin.data) as v:
        last = time.time()
        while v.is_running():
            now = time.time()
            twin.step(now - last)
            last = now
            v.sync()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--view", action="store_true", help="open the MuJoCo viewer")
    args = parser.parse_args()
    if args.view:
        _view()
    else:
        twin = Twin()
        twin.set_speed(0, 0.5)
        twin.set_speed(1, 0.5)
        twin.step(2.0)
        print("chassis pose after 2s:", twin.chassis_pose())
