"""Agentic cortex.

The LEGO brick is a tool. The cortex (Claude Opus 4.7, adaptive
thinking) takes a natural-language goal, plans, and calls into the
skills + perception + brick layers. It can also push **muscle memory**
to the brick — small Python programs that run on the hub itself for
closer-to-the-metal control — by calling the `load_skill` tool.

    "follow me" / "move in a circle" / "pick that thing up" /
    "learn to walk like a crab"
           │
           ▼
    Claude (cortex) ----- drive / turn / arc / stop ----▶ Skills
           │         ----- look                        ──▶ Perception
           │         ----- load_skill / skill_message  ──▶ motorctl → hub
           │
           ▼
      get_status
"""

from __future__ import annotations

import json
from pathlib import Path

import anthropic
from anthropic import beta_tool

from .ipc import MotorCtl, MotorCtlError
from .perception import describe as _perception_describe
from .skills import Skills

SYSTEM_PROMPT = """You are the cognitive cortex of a LEGO Mindstorms robot.

The robot's body is a LEGO Technic build powered by a SPIKE Prime / Robot
Inventor hub. The hub handles low-level motor control (PID, encoders,
power) so you don't have to think about millisecond timing — your job is
to plan, perceive, and act at the level of human-meaningful goals.

You drive the robot through these tools:

- drive(distance_cm, speed)         straight-line motion
- turn(degrees, speed)              rotate in place; positive = clockwise
- arc(radius_cm, angle_deg, speed)  curved path
- stop()                            halt all motion
- look(prompt)                      take a camera frame and get a description
- get_status()                      read the brick's firmware and port state

You can also push "muscle memory" — small control programs that run on
the brick itself — if the hub is running Pybricks firmware:

- load_skill(name)                  upload a named skill from skills_lib/
- unload_skill()                    stop the running skill
- skill_message(payload)            send a message to the running skill

Muscle memory is for closed-loop work that is too fast or too jittery
to run from the Pi: balance reflexes, tight diff-drive PID, gait
cycles. The cortex stays in charge of *what* to do; the skill on the
brick decides *how* in milliseconds.

Guidelines:
- Speeds are 0..1. Default to 0.3-0.5; only push higher when asked.
- Before any move into uncertain space, call look(). Don't drive blind.
- For "follow" / "find" / "pick up" tasks, alternate look() with small
  motion steps. Re-check after every move, not just at the start.
- Only load_skill when the task genuinely benefits from on-brick control;
  for a single short move, the high-level tools above are enough.
- If something feels unsafe or you don't know what to do, stop() and
  ask the user.
- Keep narration short. The user is watching the robot, not reading.
"""

SKILLS_LIB_DIR = Path(__file__).resolve().parent.parent.parent / "skills_lib"


def _list_skills() -> list[str]:
    if not SKILLS_LIB_DIR.is_dir():
        return []
    return sorted(p.stem for p in SKILLS_LIB_DIR.glob("*.py") if p.stem != "__init__")


def build_tools(skills: Skills, motor: MotorCtl):
    """Construct the @beta_tool functions bound to this robot's skills."""

    @beta_tool
    def drive(distance_cm: float, speed: float = 0.4) -> str:
        """Drive in a straight line.

        Args:
            distance_cm: How far to travel, in centimetres. Negative reverses.
            speed: 0..1. Default 0.4.
        """
        skills.drive(distance_cm, speed)
        return f"drove {distance_cm:.1f} cm at speed {speed}"

    @beta_tool
    def turn(degrees: float, speed: float = 0.3) -> str:
        """Rotate the robot in place. Positive = clockwise (right turn).

        Args:
            degrees: Rotation in degrees.
            speed: 0..1. Default 0.3.
        """
        skills.turn(degrees, speed)
        return f"turned {degrees:.1f} deg at speed {speed}"

    @beta_tool
    def arc(radius_cm: float, angle_deg: float, speed: float = 0.3) -> str:
        """Drive a curved path. Positive radius curves right, negative curves left.

        Args:
            radius_cm: Turning radius in centimetres. 0 for a pivot turn.
            angle_deg: How far around the arc to travel.
            speed: 0..1. Default 0.3.
        """
        skills.arc(radius_cm, angle_deg, speed)
        return f"arc r={radius_cm:.1f}cm angle={angle_deg:.1f}deg"

    @beta_tool
    def stop() -> str:
        """Stop all motion immediately."""
        skills.stop()
        return "stopped"

    @beta_tool
    def look(prompt: str = "Describe what is visible in front of the robot. Mention people, obstacles, distances, and which side of the frame they are on.") -> str:
        """Take a picture from the robot's camera and get a textual description.

        Args:
            prompt: What to look for or how to describe the scene.
        """
        return _perception_describe(prompt).description

    @beta_tool
    def get_status() -> str:
        """Return the motorctl daemon's view of firmware, ports, and any running skill."""
        return json.dumps(motor.status())

    @beta_tool
    def load_skill(name: str) -> str:
        """Upload a named skill from skills_lib/ to the brick and start it.

        Available skills: {skills}

        Args:
            name: Skill name (filename stem under skills_lib/).
        """
        path = SKILLS_LIB_DIR / f"{name}.py"
        if not path.is_file():
            return f"unknown skill {name!r}; available: {_list_skills()}"
        try:
            motor.load_skill(name, path)
        except MotorCtlError as e:
            return f"load_skill failed: {e}"
        return f"loaded skill {name!r}"

    @beta_tool
    def unload_skill() -> str:
        """Stop and unload any running brick-side skill."""
        try:
            motor.unload_skill()
        except MotorCtlError as e:
            return f"unload_skill failed: {e}"
        return "unloaded"

    @beta_tool
    def skill_message(payload: str) -> str:
        """Send a message to the running brick-side skill.

        Args:
            payload: String passed to the skill via Pybricks BLE messaging.
        """
        try:
            motor.skill_message(payload)
        except MotorCtlError as e:
            return f"skill_message failed: {e}"
        return "sent"

    # Inject the available skills list into load_skill's docstring so the
    # model sees what's actually on disk.
    skills_listing = ", ".join(_list_skills()) or "(none yet)"
    load_skill.__doc__ = (load_skill.__doc__ or "").format(skills=skills_listing)

    return [drive, turn, arc, stop, look, get_status, load_skill, unload_skill, skill_message]


class Cortex:
    """Single-turn agentic loop. Call `.act(instruction)` once per goal."""

    def __init__(
        self,
        motor: MotorCtl,
        skills: Skills | None = None,
        model: str = "claude-opus-4-7",
    ) -> None:
        self.motor = motor
        self.skills = skills or Skills(motor)
        self.model = model
        self.client = anthropic.Anthropic()

    def act(self, instruction: str, *, effort: str = "high") -> None:
        """Execute a natural-language instruction."""
        runner = self.client.beta.messages.tool_runner(
            model=self.model,
            max_tokens=16000,
            thinking={"type": "adaptive"},
            output_config={"effort": effort},
            system=SYSTEM_PROMPT,
            tools=build_tools(self.skills, self.motor),
            messages=[{"role": "user", "content": instruction}],
        )
        for message in runner:
            for block in message.content:
                if block.type == "text" and block.text.strip():
                    print(f"[cortex] {block.text}")
