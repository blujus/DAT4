"""PPO training scaffold for brick-level skills.

Uses stable-baselines3 to train a policy on `BackpackEnv` (or a subclass
you register per skill), then saves the policy under `runs/<skill>/`.
Distill that policy into a Pybricks-runnable skill via `distill.py`.

This is intentionally minimal — reward shaping, hyperparameters, and
env variations are the part you'll actually iterate on. Treat this as
a working starting point, not the answer.
"""

from __future__ import annotations

import argparse
from pathlib import Path

from .env import BackpackEnv


def train(skill: str, steps: int, out_dir: Path) -> Path:
    try:
        from stable_baselines3 import PPO
    except ImportError as e:
        raise SystemExit(
            "stable-baselines3 is not installed. "
            "Run `pip install -e '.[train]'` from backpack/twin."
        ) from e

    out_dir.mkdir(parents=True, exist_ok=True)

    # Per-skill envs would go here; default to plain BackpackEnv.
    env = BackpackEnv()

    model = PPO(
        policy="MlpPolicy",
        env=env,
        verbose=1,
        tensorboard_log=str(out_dir / "tb"),
    )
    model.learn(total_timesteps=steps)
    policy_path = out_dir / "policy.zip"
    model.save(str(policy_path))
    return policy_path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--skill", required=True, help="skill name (used for the run directory)")
    parser.add_argument("--steps", type=int, default=200_000)
    parser.add_argument("--out", type=Path, default=None)
    args = parser.parse_args()
    out = args.out or Path("runs") / args.skill
    policy = train(args.skill, args.steps, out)
    print(f"saved policy to {policy}")


if __name__ == "__main__":
    main()
