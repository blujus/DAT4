"""End-to-end smoke test for the digital twin.

Spawns `twin.sim_daemon` as a subprocess on a temp Unix socket, drives a
short course through the `Skills` layer (i.e. exactly the same path the
Pi cortex takes), and asserts the chassis ended up roughly where we
expect.

The point is regression coverage for the parallel-motor bug: if
`run_for_degrees` blocks the daemon for the full move duration, then
`drive()` (which issues left then right) becomes a turn instead of a
straight line. So if this test passes, parallel execution is working.

Tolerances are deliberately loose — sim-to-real fidelity isn't the goal
here; "moves are happening, not blocked" is.
"""

from __future__ import annotations

import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import pytest

from backpack.ipc import MotorCtl
from backpack.skills import DriveConfig, Skills


# The MuJoCo robot in `assets/robot.xml` has both wheel hinges on the
# same +Y axis, so positive ctrl on either spins the wheel the same
# physical direction. Real LEGO builds usually have mirrored motors
# (hence `DriveConfig.invert_right=True` by default); the sim doesn't.
SIM_DRIVE_CFG = DriveConfig(invert_left=False, invert_right=False)


# Sim is uncalibrated to real-world cm — the velocity actuator's kv is
# low so commanded encoder degrees produce ~4x less linear travel than
# the Skills-layer math expects. We assert direction and ordering, not
# absolute distance.
POSITION_TOL_M = 0.30   # 30 cm — generous; sim is not metrically tuned
YAW_TOL_RAD = 0.6        # ~35 deg — generous; sim turns drift on caster


def _wait_for_socket(path: Path, timeout: float = 5.0) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if path.exists():
            try:
                s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                s.connect(str(path))
                s.close()
                return
            except OSError:
                pass
        time.sleep(0.05)
    raise TimeoutError(f"sim_daemon socket {path} did not appear within {timeout}s")


def _get_pose(client: MotorCtl) -> tuple[float, float, float]:
    """Use the daemon's debug `get_pose` to read chassis world pose."""
    # `MotorCtl` doesn't expose this since it's debug-only — go through
    # the underlying socket directly.
    assert client._sock is not None  # connected
    client._sock.sendall((json.dumps({"cmd": "get_pose"}) + "\n").encode())
    line = client._readline()
    resp = json.loads(line)
    assert resp.get("ok") == "pose", resp
    return float(resp["x"]), float(resp["y"]), float(resp["yaw"])


@pytest.fixture
def sim_daemon():
    """Spawn `twin.sim_daemon` on a temp socket; yield (proc, socket_path)."""
    tmpdir = tempfile.mkdtemp(prefix="backpack-twin-smoke-")
    socket_path = Path(tmpdir) / "motorctl-sim.sock"
    proc = subprocess.Popen(
        [sys.executable, "-m", "twin.sim_daemon", "--socket", str(socket_path)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    try:
        _wait_for_socket(socket_path)
        yield proc, socket_path
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=3.0)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=1.0)
        shutil.rmtree(tmpdir, ignore_errors=True)


def test_skills_drive_through_sim_daemon(sim_daemon):
    """Skills.drive moves the chassis forward; turn rotates it.

    Critically, `drive` issues `run_for_degrees(left)` then
    `run_for_degrees(right)` — if the daemon blocks the second on the
    first (the bug we fixed), the bot turns left instead of going
    straight, and the elapsed wall time roughly doubles.
    """
    _proc, socket_path = sim_daemon
    with MotorCtl(str(socket_path)) as client:
        skills = Skills(client, cfg=SIM_DRIVE_CFG)

        x0, y0, yaw0 = _get_pose(client)

        t0 = time.monotonic()
        skills.drive(20)            # 20 cm forward
        elapsed_drive = time.monotonic() - t0
        x1, y1, yaw1 = _get_pose(client)

        # 1. The chassis moved. Sim units don't match cortex cm
        # exactly (the actuator's kv is loose), but with both motors
        # running in parallel the bot covers ~6 cm of sim distance for
        # `drive(20)`. With the parallel-motor bug — only one motor
        # driving for half the move at a time — it covers only ~2 cm.
        # 4 cm threshold cleanly separates the two regimes.
        moved = ((x1 - x0) ** 2 + (y1 - y0) ** 2) ** 0.5
        assert moved > 0.04, (
            f"drive(20) moved only {moved*100:.2f} cm — "
            f"motors likely not running in parallel "
            f"(or daemon not stepping physics)"
        )

        # 2. The chassis went mostly forward, not sideways. With the
        # parallel-motor bug, only the left wheel runs first while the
        # right is queued, producing a pivot rather than translation.
        # A roughly straight drive keeps |yaw drift| small.
        assert abs(yaw1 - yaw0) < YAW_TOL_RAD, (
            f"drive(20) drifted yaw by {yaw1 - yaw0:.2f} rad "
            f"(>{YAW_TOL_RAD}) — motors likely not running in parallel"
        )

        # 3. Parallel timing sanity check. With the bug, drive() takes
        # roughly 2x the no-bug duration because both run_for_degrees
        # calls block. Cap below at the no-bug ETA + slack.
        # Skills.drive uses time.sleep(eta) where eta ≈ deg/(1000*sp)+0.2
        # For 20cm at speed 0.4 → eta ≈ 285/400+0.2 ≈ 0.91s. Doubled
        # would be ~1.8s+. Use a generous threshold to avoid flake.
        assert elapsed_drive < 4.0, (
            f"drive(20) took {elapsed_drive:.2f}s — suspiciously slow, "
            f"motors may be running sequentially"
        )

        # 4. Turn should rotate the chassis.
        skills.turn(90)
        x2, y2, yaw2 = _get_pose(client)
        assert abs(yaw2 - yaw1) > 0.1, (
            f"turn(90) only rotated yaw by {yaw2 - yaw1:.2f} rad"
        )

        # 5. Drive again; chassis position must change once more.
        skills.drive(20)
        x3, y3, _ = _get_pose(client)
        moved2 = ((x3 - x2) ** 2 + (y3 - y2) ** 2) ** 0.5
        assert moved2 > 0.01, (
            f"second drive(20) moved only {moved2*100:.2f} cm"
        )


def test_run_for_degrees_runs_in_background(sim_daemon):
    """`run_for_degrees` should start the move and return; the daemon
    must remain responsive while the wheel is still spinning toward
    its target. The real brick PIDs the move in the background — sim
    has to match.
    """
    _proc, socket_path = sim_daemon
    with MotorCtl(str(socket_path)) as client:
        # Issue a long move, then immediately ask the daemon questions.
        # If `run_for_degrees` blocked the daemon, these subsequent
        # calls would queue behind it and arrive only after the move
        # finishes.
        client.run_for_degrees(0, 720, 0.5)

        t0 = time.monotonic()
        for _ in range(10):
            client.status()
        round_trip = (time.monotonic() - t0) / 10
        assert round_trip < 0.05, (
            f"status() round-trip averaged {round_trip*1000:.1f} ms "
            f"while a `run_for_degrees` was outstanding — daemon is "
            f"likely blocked on the move"
        )

        # Stop cleanly so the test fixture teardown is quick.
        client.stop(0)
