# Backpack digital twin

A MuJoCo-based simulation of the LEGO robot, plus a Gymnasium
environment for training brick-level "muscle memory" skills with RL,
and a `sim_daemon` that speaks the same Unix-socket protocol as the
real `motorctl` so the Pi cortex code runs unchanged against
simulation.

## Why a twin

Two training loops, both expensive on real hardware, both reasonable in
simulation:

1. **Cortex-level training** — evaluate prompts, tools, and reward
   signals for the Claude-driven cortex ("does it actually go fetch the
   red brick?") without burning brick batteries.
2. **Muscle-memory RL** — train PID gains, gait cycles, and balance
   reflexes that get distilled into the small Pybricks skills under
   `skills_lib/`.

A twin also gives you reproducibility: the same seed, the same physics,
the same outcome. Real brick + real motors + real friction will not.

## Layout

```
twin/
  pyproject.toml
  twin/
    __init__.py
    world.py            # MuJoCo wrapper around assets/robot.xml
    env.py              # Gymnasium env: reset/step/observation/reward
    sim_daemon.py       # exposes motorctl-compatible IPC against the sim
    train_skill.py      # PPO training scaffold (stable-baselines3)
    distill.py          # policy -> MicroPython skill in skills_lib/
  assets/
    robot.xml           # MuJoCo MJCF: simple skid-steer two-wheel robot
```

## Quickstart

```sh
cd backpack/twin
pip install -e '.[dev]'

# 1. Visualise the robot
python -m twin.world --view

# 2. Run a Gym env smoke test (random actions)
python -m twin.env --smoke

# 3. Train a diff-drive PID skill in sim
python -m twin.train_skill --skill diff_drive_pid --steps 500_000

# 4. Distill the trained policy into skills_lib/
python -m twin.distill --skill diff_drive_pid \
  --policy runs/diff_drive_pid/policy.zip \
  --out ../skills_lib/diff_drive_pid_learned.py

# 5. Run the cortex against simulation instead of the real brick
python -m twin.sim_daemon --socket /tmp/motorctl-sim.sock &
MOTORCTL_SOCKET=/tmp/motorctl-sim.sock \
  python -m backpack 'drive in a circle of radius 50 cm'
```

## Status

- `world.py`, `env.py`, `sim_daemon.py` — working scaffolds.
- `assets/robot.xml` — a deliberately simple skid-steer model. Replace
  with your actual LEGO build for sim-to-real fidelity.
- `train_skill.py` — calls into stable-baselines3 PPO and saves a
  policy. Reward shaping per skill is intentionally minimal; tune for
  your task.
- `distill.py` — emits a Pybricks skill file with placeholder gains
  and a TODO marker. Filling in the real distillation (lookup table /
  fitted PID / tinyMLP) is the next step.

The twin is meant to grow with the project, not be a one-shot drop.
