# Virtual testing

The digital twin lets you exercise the cortex stack without a brick:
the same Skills layer, the same `MotorCtl` IPC client, the same JSON
wire format — just `sim_daemon` standing in for the Rust `motorctl`
daemon, with MuJoCo doing physics under the hood.

## Setup

From the repo root:

```sh
pip install mujoco gymnasium numpy pytest
pip install -e backpack/python
pip install -e backpack/twin
```

The `dev` extra (`pip install -e 'backpack/twin[dev]'`) adds `pytest`
and `ruff`; the line above keeps deps minimal.

## Running the smoke test

```sh
cd backpack/twin
python -m pytest tests/test_smoke.py -v
```

Expected: 2 tests pass in under 30 seconds (typically ~5 seconds on a
laptop). The smoke test:

1. Spawns `twin.sim_daemon` as a subprocess on a temp socket
   (`tempfile.mkdtemp()`).
2. Connects with the Python `MotorCtl` client from `backpack.ipc`.
3. Drives a short course through the `Skills` layer:
   `drive(20)` → `turn(90)` → `drive(20)`.
4. Reads chassis pose between moves via the daemon's debug
   `get_pose` command and asserts the bot actually translated and
   rotated.
5. Verifies the daemon stays responsive while a `run_for_degrees`
   move is in flight (the regression check for the parallel-motor
   bug — see "Notes" below).
6. Tears the subprocess down in a `finally` block.

If the smoke test fails with `drive(20) moved only X cm`, the
parallel-motor handling in `sim_daemon` has regressed: both
`run_for_degrees` calls must run concurrently in the background
step loop, not sequentially in the request handler.

## Running the cortex against the simulator

`sim_daemon` speaks the same protocol as the real Rust `motorctl`,
so the cortex switches over by pointing `MOTORCTL_SOCKET` at the
sim's socket:

```sh
# Terminal 1: launch the simulator
python -m twin.sim_daemon --socket /tmp/motorctl-sim.sock

# Terminal 2: run the cortex against it
MOTORCTL_SOCKET=/tmp/motorctl-sim.sock \
  python -m backpack 'drive in a circle of radius 50 cm'
```

The cortex code in `backpack/python/backpack/` is unchanged — it
reads `MOTORCTL_SOCKET` and connects to whatever Unix socket is
there. Skills, perception (minus the camera, which doesn't have a
sim source yet), and the agent loop all run end-to-end.

To visualise what's happening, open the MuJoCo viewer in another
terminal:

```sh
python -m twin.world --view
```

This opens a separate `Twin` instance — it will *not* share state
with the running `sim_daemon`. For shared visualisation, attach a
viewer inside `sim_daemon` itself (TODO; the scaffold doesn't yet).

## Notes

### Wheel mirroring

`backpack.skills.DriveConfig.invert_right=True` by default because
real LEGO builds usually mirror the right motor. The MuJoCo robot
in `assets/robot.xml` does *not* mirror — both wheel hinges share
the same axis. The smoke test sets up `DriveConfig(invert_left=False,
invert_right=False)` accordingly. If you run the cortex against
sim, override `DriveConfig` the same way (or fix `robot.xml` to
mirror the right wheel — pick one).

### Calibration mismatch

The MuJoCo actuators in `robot.xml` use `kv=0.05` velocity gain,
which is loose enough that commanded encoder degrees produce ~3-4x
less linear travel than `Skills.drive(distance_cm)` would predict
on a real LEGO chassis. This is fine for "moves are happening"
testing but unsuitable for sim-to-real distance accuracy. Tune the
actuator gains in `assets/robot.xml` against your real build before
relying on absolute distances.

### The parallel-motor bug

`Skills.drive` issues `run_for_degrees(left)` then
`run_for_degrees(right)`. On the brick, both run in parallel
because each port has its own PID controller in firmware. In
`sim_daemon`, that requires the IPC handler to record a target and
return immediately, while a background task ticks physics and stops
each motor when its encoder hits its target. The pre-fix
`sim_daemon` called `Twin.run_for_degrees` synchronously inside the
handler — which spins one wheel for the full move duration before
the second call even starts. End result: bot crawls forward at
half-speed instead of running both wheels in parallel.

`tests/test_smoke.py::test_skills_drive_through_sim_daemon` is the
regression check: with the bug, `drive(20)` moves the chassis only
~2 cm of sim distance; without the bug, ~6 cm. The 4 cm threshold
cleanly separates the two.
