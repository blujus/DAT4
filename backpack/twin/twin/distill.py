"""Distill an RL policy into a small Pybricks-runnable skill.

A neural-net policy from `train_skill.py` won't fit / run on the hub.
We distill it into a much smaller form that does:

- a fitted PID controller (good when the policy is essentially a
  velocity tracker with bias terms), or
- a lookup table indexed by a low-dim observation, or
- a tiny MLP exported as float arrays in MicroPython source.

This stub emits a Pybricks skill file with placeholder gains and a
`# TODO: replace with distilled gains` marker. The actual distillation
pass (sample policy, fit a model, write coefficients) is the next step
— see comments inline.
"""

from __future__ import annotations

import argparse
import textwrap
from pathlib import Path


DISTILLED_TEMPLATE = '''\
"""Distilled skill: {skill}.

Generated from {policy_path}. Do not hand-edit — re-run
`twin/twin/distill.py` if you want to update.
"""

from pybricks.hubs import PrimeHub
from pybricks.pupdevices import Motor
from pybricks.parameters import Port
from pybricks.tools import wait

hub = PrimeHub()
left = Motor(Port.A)
right = Motor(Port.B)
CHANNEL = 1
hub.ble.observe(CHANNEL)

# TODO: replace with distilled gains.
GAINS = {gains!r}

while True:
    msg = hub.ble.observe(CHANNEL)
    # TODO: implement distilled controller using GAINS.
    wait(10)
'''


def distill(skill: str, policy_path: Path, out_path: Path) -> None:
    """Write a Pybricks skill file derived from `policy_path`.

    For a real implementation:

    1. Load the policy: `model = PPO.load(policy_path)`.
    2. Sample (obs, action) pairs across the observation space.
    3. Fit a parsimonious model:
         - Linear regression for PID-style controllers.
         - Decision tree / quantised lookup for switching policies.
         - tinyMLP (1-2 layers) exported as numpy arrays for general
           policies, then printed into the skill source.
    4. Render those parameters into the template below.
    """
    if not policy_path.exists():
        raise SystemExit(f"policy not found: {policy_path}")

    # Placeholder gains; real distillation fills these in.
    gains = {"kp": 1.5, "ki": 0.05, "kd": 0.10}

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(
        DISTILLED_TEMPLATE.format(skill=skill, policy_path=policy_path, gains=gains)
    )
    print(textwrap.dedent(f"""\
        wrote {out_path}
        next: replace the GAINS placeholder with values fit from the policy,
              and implement the controller body in distill.py.
    """))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--skill", required=True)
    parser.add_argument("--policy", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    distill(args.skill, args.policy, args.out)


if __name__ == "__main__":
    main()
'''
