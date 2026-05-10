"""Gymnasium environment for training brick-level skills.

This is the surface RL agents see. Subclass `BackpackEnv` to define a
specific skill task: override `_reward()`, `_observation()`, and
`_terminated()`. The default task is `drive_forward`: track a target
forward velocity while keeping yaw drift small.
"""

from __future__ import annotations

import argparse
import math
from typing import Any

import gymnasium as gym
import numpy as np
from gymnasium import spaces

from .world import Twin


class BackpackEnv(gym.Env):
    metadata = {"render_modes": ["human"]}

    def __init__(
        self,
        target_v_fwd: float = 0.3,
        episode_seconds: float = 10.0,
        control_hz: float = 50.0,
    ) -> None:
        super().__init__()
        self.twin = Twin()
        self.target_v_fwd = target_v_fwd
        self.episode_seconds = episode_seconds
        self.dt = 1.0 / control_hz
        self._t = 0.0
        self._prev_pose: tuple[float, float, float] = (0.0, 0.0, 0.0)

        # Action: [left_speed, right_speed] in [-1, 1].
        self.action_space = spaces.Box(low=-1.0, high=1.0, shape=(2,), dtype=np.float32)
        # Observation: [v_fwd_error, yaw, yaw_rate, left_dps, right_dps, target_v_fwd].
        high = np.array([5.0, math.pi, 50.0, 2000.0, 2000.0, 5.0], dtype=np.float32)
        self.observation_space = spaces.Box(low=-high, high=high, dtype=np.float32)

    # ------------------------------------------------------------------ gym

    def reset(self, *, seed: int | None = None, options: dict[str, Any] | None = None):
        super().reset(seed=seed)
        self.twin.reset(seed=seed)
        self._t = 0.0
        self._prev_pose = self.twin.chassis_pose()
        return self._observation(), {}

    def step(self, action: np.ndarray):
        action = np.clip(action, -1.0, 1.0)
        self.twin.set_speed(0, float(action[0]))
        self.twin.set_speed(1, float(action[1]))
        self.twin.step(self.dt)
        self._t += self.dt

        obs = self._observation()
        reward = self._reward(obs, action)
        terminated = self._terminated(obs)
        truncated = self._t >= self.episode_seconds
        return obs, reward, terminated, truncated, {}

    # ------------------------------------------------------- task hooks

    def _observation(self) -> np.ndarray:
        x, y, yaw = self.twin.chassis_pose()
        px, py, _ = self._prev_pose
        # body-frame forward velocity
        dx = (x - px) / self.dt if self.dt > 0 else 0.0
        dy = (y - py) / self.dt if self.dt > 0 else 0.0
        v_fwd = math.cos(yaw) * dx + math.sin(yaw) * dy
        v_err = self.target_v_fwd - v_fwd

        # yaw rate via finite differences on stored prev pose
        yaw_prev = self._prev_pose[2]
        yaw_rate = (yaw - yaw_prev) / self.dt if self.dt > 0 else 0.0
        self._prev_pose = (x, y, yaw)

        l = self.twin.motor_state(0).speed_dps
        r = self.twin.motor_state(1).speed_dps
        return np.array([v_err, yaw, yaw_rate, l, r, self.target_v_fwd], dtype=np.float32)

    def _reward(self, obs: np.ndarray, action: np.ndarray) -> float:
        v_err, yaw, yaw_rate, *_ = obs
        # track target velocity, penalise heading drift, mild action cost
        return float(
            -1.0 * v_err * v_err
            - 0.1 * yaw * yaw
            - 0.01 * yaw_rate * yaw_rate
            - 0.001 * float(np.sum(action * action))
        )

    def _terminated(self, obs: np.ndarray) -> bool:
        # Tip-over guard: if yaw rate explodes, the wheels probably slipped
        # or the chassis flipped. Cheap proxy.
        return abs(float(obs[2])) > 30.0


def _smoke() -> None:
    env = BackpackEnv()
    obs, _ = env.reset(seed=0)
    total = 0.0
    for _ in range(200):
        action = env.action_space.sample()
        obs, r, term, trunc, _ = env.step(action)
        total += r
        if term or trunc:
            break
    print(f"smoke ok. cumulative reward = {total:.2f}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--smoke", action="store_true")
    args = parser.parse_args()
    if args.smoke:
        _smoke()
