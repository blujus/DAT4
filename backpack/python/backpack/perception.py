"""Camera perception.

Grabs a frame from the Pi camera and uses Claude's vision capability
(Haiku 4.5 — fast and cheap, good enough for captioning) to turn it into
a textual description the cortex can reason about. For development
without the hardware in front of you, set `BACKPACK_FAKE_CAMERA=1` and
the module returns a stub sighting instead of opening the camera.
"""

from __future__ import annotations

import base64
import io
import os
from dataclasses import dataclass

import anthropic

_VISION_MODEL = "claude-haiku-4-5"


@dataclass
class Sighting:
    description: str
    raw_image_b64: str | None = None


def _capture_jpeg() -> bytes:
    if os.environ.get("BACKPACK_FAKE_CAMERA"):
        return b""
    try:
        from picamera2 import Picamera2  # type: ignore
    except ImportError as e:
        raise RuntimeError(
            "picamera2 not installed. Install with `pip install backpack[camera]`,"
            " or set BACKPACK_FAKE_CAMERA=1 for off-robot development."
        ) from e

    cam = Picamera2()
    cam.configure(cam.create_still_configuration())
    cam.start()
    try:
        buf = io.BytesIO()
        cam.capture_file(buf, format="jpeg")
        return buf.getvalue()
    finally:
        cam.close()


def describe(prompt: str = "Describe what is visible in front of the robot. Mention any people, obstacles, distances, and which side of the frame they are on.") -> Sighting:
    """Take a picture and return a textual description of what the robot sees."""
    img = _capture_jpeg()
    if not img:
        return Sighting(
            description=(
                "(fake camera) An indoor space; the user appears about 1m"
                " ahead, slightly to the left of centre. Floor is clear."
            )
        )

    client = anthropic.Anthropic()
    b64 = base64.b64encode(img).decode()
    resp = client.messages.create(
        model=_VISION_MODEL,
        max_tokens=512,
        messages=[{
            "role": "user",
            "content": [
                {
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": "image/jpeg",
                        "data": b64,
                    },
                },
                {"type": "text", "text": prompt},
            ],
        }],
    )
    text = next((b.text for b in resp.content if b.type == "text"), "")
    return Sighting(description=text, raw_image_b64=b64)
