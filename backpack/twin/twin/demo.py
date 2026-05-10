"""Watch the virtual bot move.

Three ways:

1. Interactive 3D viewer (default). Opens a MuJoCo window; you can
   orbit, zoom, and pause. Needs a desktop / X / Wayland session.

        python -m twin.demo
        python -m twin.demo --laps 3

2. Headless MP4 recording. Renders offscreen — works over SSH, in a
   container, on a server. Output is shareable. Needs imageio +
   imageio-ffmpeg (`pip install -e '.[video]'`).

        python -m twin.demo --record demo.mp4
        python -m twin.demo --record demo.mp4 --laps 2 --width 1280 --height 720

3. Cortex-in-the-loop. Run sim_daemon, point the agent at it. See
   `twin/TESTING.md` for the env-var setup. The agent will call into
   the *same* `Twin` from the inside; you can attach a viewer with
   approach 1 or 2 in parallel by reading the same MJCF — currently
   not wired up to share state with the daemon, see TESTING.md TODO.

The scripted maneuver in this file drives a rough square (forward,
turn, repeat) using direct `Twin.set_speed` calls — no daemon, no
IPC. It's intentionally calibration-agnostic: the goal is to *see*
the bot, not to land precisely back at the origin.
"""

from __future__ import annotations

import argparse
from pathlib import Path

import mujoco

from .world import Twin

DT = 0.02              # control-loop tick (50 Hz)
LEG_SECONDS = 2.0      # straight segment
TURN_SECONDS = 1.4     # 90°-ish in-place pivot


def _square(twin: Twin, on_step) -> None:
    """Drive one lap of a square. `on_step(twin)` runs every tick."""

    def segment(left: float, right: float, seconds: float) -> None:
        twin.set_speed(0, left)
        twin.set_speed(1, right)
        for _ in range(int(seconds / DT)):
            twin.step(DT)
            on_step(twin)
        twin.stop(0)
        twin.stop(1)

    for _ in range(4):
        segment(0.5, 0.5, LEG_SECONDS)       # forward
        segment(-0.4, 0.4, TURN_SECONDS)     # pivot left


def run_viewer(laps: int) -> None:
    import mujoco.viewer  # local import — needs a display

    twin = Twin()
    with mujoco.viewer.launch_passive(twin.model, twin.data) as v:
        def on_step(_twin: Twin) -> None:
            v.sync()

        for _ in range(laps):
            _square(twin, on_step)


def run_record(out: Path, laps: int, fps: int, width: int, height: int) -> None:
    try:
        import imageio.v3 as iio  # noqa: F401  -- presence check
        import imageio
    except ImportError as e:
        raise SystemExit(
            "recording needs imageio + imageio-ffmpeg.\n"
            "Install with:  pip install -e '.[video]'  (from backpack/twin)"
        ) from e

    twin = Twin()
    chassis_id = mujoco.mj_name2id(twin.model, mujoco.mjtObj.mjOBJ_BODY, "chassis")

    renderer = mujoco.Renderer(twin.model, height=height, width=width)
    cam = mujoco.MjvCamera()
    mujoco.mjv_defaultFreeCamera(twin.model, cam)
    cam.distance = 0.8
    cam.elevation = -30.0
    cam.azimuth = 135.0

    writer = imageio.get_writer(str(out), fps=fps, codec="libx264", quality=8)
    frames_per_step = max(1, int(round(1.0 / (fps * DT))))
    try:
        tick = {"i": 0}

        def on_step(t: Twin) -> None:
            # Render at `fps`, not at every physics tick — saves a lot of time
            tick["i"] += 1
            if tick["i"] % frames_per_step != 0:
                return
            cam.lookat[:] = t.data.xpos[chassis_id]
            renderer.update_scene(t.data, camera=cam)
            writer.append_data(renderer.render())

        for _ in range(laps):
            _square(twin, on_step)
    finally:
        writer.close()
        renderer.close()
    print(f"wrote {out} ({laps} lap(s))")


def main() -> None:
    p = argparse.ArgumentParser(description="watch the virtual bot move")
    p.add_argument("--record", type=Path, help="write MP4 instead of opening viewer")
    p.add_argument("--laps", type=int, default=1)
    p.add_argument("--fps", type=int, default=30)
    p.add_argument("--width", type=int, default=640)
    p.add_argument("--height", type=int, default=480)
    args = p.parse_args()
    if args.record:
        run_record(args.record, args.laps, args.fps, args.width, args.height)
    else:
        run_viewer(args.laps)


if __name__ == "__main__":
    main()
