"""Agentic cortex.

The LEGO brick is just another tool to the cortex — alongside the
camera. Claude (Opus 4.7, adaptive thinking) takes a natural-language
goal, plans, and calls into the skills layer, which translates each
call into LWP3 commands the brick actually executes.

    "follow me" / "move in a circle" / "pick that thing up"
           │
           ▼
    Claude (cortex) ----- drive / turn / arc / stop ----▶ Skills
           │                                              │
           │ look()                                       ▼
           ▼                                          motorctl (BLE)
      Claude vision                                       │
      (Haiku 4.5)                                         ▼
           │                                          LEGO 51515 hub
      Pi camera                                           │
                                                          ▼
                                                       motors
"""

from __future__ import annotations

import json

import anthropic
from anthropic import beta_tool

from .ipc import MotorCtl
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
- get_status()                      read the motorctl daemon's view of each port

Guidelines:
- Speeds are in 0..1. Default to 0.3-0.5; only push higher when the user
  asks for it.
- Before any move into uncertain space, call look(). Don't drive blind.
- For "follow" / "find" / "pick up" tasks, alternate look() with small
  motion steps. Re-check after every move, not just at the start.
- If something feels unsafe or you don't know what to do, stop() and
  ask the user.
- Keep narration short. The user is watching the robot, not reading.
"""


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
            radius_cm: Turning radius in centimetres. Use 0 for a pivot turn.
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
        """Return the motorctl daemon's view of each port (last commanded speed)."""
        return json.dumps(motor.status())

    return [drive, turn, arc, stop, look, get_status]


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
        """Execute a natural-language instruction.

        Claude plans, calls tools (which run on this process and the
        connected hub), and finishes when it judges the goal achieved —
        or when it asks for clarification.
        """
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
