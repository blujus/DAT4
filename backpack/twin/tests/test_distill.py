"""Unit tests for `twin.distill`.

These run without SB3, torch, MuJoCo, or Gymnasium — they exercise the
pure fitting and rendering helpers using synthetic data, which is the
whole reason those helpers were factored out.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest

from twin import distill


# Synthetic policy: a deterministic linear function of the observation.
#   obs = [v_err, yaw, yaw_rate, l_dps, r_dps, target]
#   action[0] = -2.0 * v_err + 0.05 * yaw     (left wheel)
#   action[1] = -2.0 * v_err - 0.05 * yaw     (right wheel)
KP_TRUE = -2.0
KYAW_LEFT_TRUE = 0.05
KYAW_RIGHT_TRUE = -0.05


def _synthetic_policy(obs: np.ndarray) -> np.ndarray:
    a_l = KP_TRUE * obs[..., 0] + KYAW_LEFT_TRUE * obs[..., 1]
    a_r = KP_TRUE * obs[..., 0] + KYAW_RIGHT_TRUE * obs[..., 1]
    return np.stack([a_l, a_r], axis=-1)


def _make_synthetic_trajectory(n: int, dt: float, seed: int = 0):
    rng = np.random.default_rng(seed)
    # Random observations spanning the obs space.
    obs = np.zeros((n, 6), dtype=np.float64)
    obs[:, 0] = rng.uniform(-1.0, 1.0, size=n)        # v_err
    obs[:, 1] = rng.uniform(-0.5, 0.5, size=n)        # yaw
    obs[:, 2] = rng.uniform(-1.0, 1.0, size=n)        # yaw_rate
    obs[:, 3] = rng.uniform(-500, 500, size=n)        # left_dps
    obs[:, 4] = rng.uniform(-500, 500, size=n)        # right_dps
    obs[:, 5] = 0.3                                   # target_v_fwd
    actions = _synthetic_policy(obs)
    return obs, actions


def test_fit_pid_recovers_kp_on_synthetic_policy():
    """The fitted KP coefficient on v_err must match the true value."""
    obs, actions = _make_synthetic_trajectory(n=2000, dt=0.01, seed=42)
    gains = distill.fit_pid_gains(obs, actions, dt=0.01)

    # The synthetic policy has zero KI/KD/KDYAW/bias, so KP_L and KP_R
    # should both come out near KP_TRUE.
    assert abs(gains["left"]["kp"] - KP_TRUE) < 0.2, gains
    assert abs(gains["right"]["kp"] - KP_TRUE) < 0.2, gains

    # Yaw cross-terms should recover too (looser tolerance — they're
    # smaller in magnitude).
    assert abs(gains["left"]["kpyaw"] - KYAW_LEFT_TRUE) < 0.05, gains
    assert abs(gains["right"]["kpyaw"] - KYAW_RIGHT_TRUE) < 0.05, gains

    # Fit quality on noiseless synthetic data should be excellent.
    assert gains["r2_left"] > 0.95, gains
    assert gains["r2_right"] > 0.95, gains
    assert gains["n_samples"] == 2000


def test_fit_pid_gains_input_validation():
    """Mismatched / malformed inputs raise ValueError."""
    obs = np.zeros((10, 6))
    bad = np.zeros((9, 2))
    with pytest.raises(ValueError):
        distill.fit_pid_gains(obs, bad, dt=0.01)

    bad_act = np.zeros((10, 3))
    with pytest.raises(ValueError):
        distill.fit_pid_gains(obs, bad_act, dt=0.01)


def test_render_diff_drive_pid_emits_valid_pybricks_source(tmp_path: Path):
    """The emitted skill file must include the recovered numbers as
    Python literals and parse as valid Python source."""
    obs, actions = _make_synthetic_trajectory(n=500, dt=0.01, seed=7)
    gains = distill.fit_pid_gains(obs, actions, dt=0.01)
    src = distill.render_diff_drive_pid(
        skill="diff_drive_pid",
        policy_path="runs/diff_drive_pid/policy.zip",
        gains=gains,
    )

    out = tmp_path / "diff_drive_pid_distilled.py"
    out.write_text(src)
    text = out.read_text()

    # The recovered KP values must appear verbatim in the source so that
    # MicroPython on the hub can parse them as float literals.
    kp_l_repr = repr(gains["left"]["kp"])
    kp_r_repr = repr(gains["right"]["kp"])
    assert kp_l_repr in text, f"missing {kp_l_repr} in emitted source"
    assert kp_r_repr in text, f"missing {kp_r_repr} in emitted source"

    # Pybricks-shaped imports / structure markers.
    assert "from pybricks.hubs import PrimeHub" in text
    assert "from pybricks.pupdevices import Motor" in text
    assert "from pybricks.parameters import Port" in text
    assert "while True:" in text
    assert "hub.ble.observe(CHANNEL)" in text

    # Source must be syntactically valid Python (MicroPython is a strict
    # subset for this code path; if CPython parses it, the hub will too).
    compile(text, str(out), "exec")


def test_build_features_shapes_are_consistent():
    """_build_features must produce the documented column layout."""
    obs = np.array(
        [
            [0.5, 0.1, 0.0, 0, 0, 0.3],
            [0.4, 0.1, 0.0, 0, 0, 0.3],
            [0.3, 0.1, 0.0, 0, 0, 0.3],
        ],
        dtype=np.float64,
    )
    feats = distill._build_features(obs, dt=0.01)
    assert feats.shape == (3, 5)
    # column 0 = v_err
    np.testing.assert_allclose(feats[:, 0], obs[:, 0])
    # column 1 = causal cumulative integral
    expected_int = np.array([0.5 * 0.01, 0.5 * 0.01 + 0.4 * 0.01,
                              0.5 * 0.01 + 0.4 * 0.01 + 0.3 * 0.01])
    np.testing.assert_allclose(feats[:, 1], expected_int)
    # column 3 = yaw, column 4 = yaw_rate
    np.testing.assert_allclose(feats[:, 3], obs[:, 1])
    np.testing.assert_allclose(feats[:, 4], obs[:, 2])
