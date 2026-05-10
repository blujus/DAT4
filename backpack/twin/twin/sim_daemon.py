"""Sim daemon: speaks the same Unix-socket protocol as the real motorctl.

Run this in place of the Rust daemon to drive the cortex against a
simulated robot. The wire format matches `crates/motorctl/src/proto.rs`
so `MOTORCTL_SOCKET=... python -m backpack '...'` runs unchanged.

LoadSkill / UnloadSkill / SkillMessage are accepted but stored only as
metadata in this scaffold — there's nothing to upload to. Skill
training still runs against the Gym env directly.

Motor state machine
-------------------
Every motor port lives in one of three states:

* IDLE — `ctrl=0`; motor is stopped.
* CONTINUOUS — `set_speed` was issued; the motor spins until told
  otherwise.
* TARGETED — `run_for_degrees` was issued; the motor spins toward an
  absolute encoder target and auto-stops on arrival.

A single background task ticks physics at ~50 Hz and, after each
`twin.step`, checks every TARGETED port and stops the ones that have
reached their goal. This is what makes parallel `run_for_degrees`
calls actually run in parallel — the IPC handler records the target
and acks immediately.

Debug commands
--------------
`get_pose` returns the chassis world pose `{x, y, yaw}` in metres and
radians. Used by the integration smoke test; not part of the real
motorctl protocol.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import math
import os
import sys
from dataclasses import dataclass
from pathlib import Path

from .world import Twin


# Step cadence for the background physics loop. Matches the original
# scaffold and gives ~20 ms latency on `run_for_degrees` completion
# detection — tighter than the cortex needs.
_STEP_DT = 0.02


@dataclass
class _MotorTarget:
    target_deg: float   # absolute encoder position to reach
    speed: float        # signed speed command driving toward target
    direction: int      # +1 if target_deg > start, -1 otherwise


class SimServer:
    def __init__(self) -> None:
        self.twin = Twin()
        self.last_speeds: list[float] = [0.0] * 6
        self.current_skill: str | None = None
        # Per-port pending `run_for_degrees` targets. Absent => not
        # targeted (either IDLE or CONTINUOUS, distinguished by
        # last_speeds[port]).
        self._targets: dict[int, _MotorTarget] = {}
        # Background task that steps physics and resolves targets.
        self._step_task: asyncio.Task | None = None
        # Guards _targets / last_speeds / twin against concurrent
        # mutation between the step loop and request handlers.
        self._lock = asyncio.Lock()

    async def start(self) -> None:
        self._step_task = asyncio.create_task(self._step_loop())

    async def _step_loop(self) -> None:
        while True:
            try:
                async with self._lock:
                    self.twin.step(_STEP_DT)
                    self._resolve_targets()
            except Exception as e:  # pragma: no cover - defensive
                print(f"sim_daemon step error: {e}", file=sys.stderr)
            await asyncio.sleep(_STEP_DT)

    def _resolve_targets(self) -> None:
        """Stop any TARGETED motor that has reached its goal."""
        done: list[int] = []
        for port, tgt in self._targets.items():
            pos = self.twin.motor_state(port).position_deg
            # Direction-aware "reached" check: the motor is moving in
            # `direction`, so we're done once we've crossed the target
            # in that direction.
            if tgt.direction > 0 and pos >= tgt.target_deg:
                done.append(port)
            elif tgt.direction < 0 and pos <= tgt.target_deg:
                done.append(port)
        for port in done:
            self.twin.stop(port)
            self.last_speeds[port] = 0.0
            self._targets.pop(port, None)

    async def handle(self, req: dict) -> dict:
        cmd = req.get("cmd")
        try:
            if cmd == "set_speed":
                async with self._lock:
                    port = req["port"]
                    self._targets.pop(port, None)  # cancel any pending target
                    self.twin.set_speed(port, req["speed"])
                    self.last_speeds[port] = req["speed"]
                return {"ok": "ack"}
            if cmd == "run_for_degrees":
                async with self._lock:
                    port = req["port"]
                    degrees = int(req["degrees"])
                    speed = float(req["speed"])
                    # Cancel any prior target on this port.
                    self._targets.pop(port, None)
                    if abs(speed) < 1e-3 or degrees == 0:
                        self.twin.stop(port)
                        self.last_speeds[port] = 0.0
                    else:
                        # `degrees` is the magnitude of rotation requested;
                        # `speed` carries the sign. Compute absolute target.
                        start_pos = self.twin.motor_state(port).position_deg
                        signed_delta = math.copysign(abs(degrees), speed)
                        target_pos = start_pos + signed_delta
                        direction = 1 if signed_delta > 0 else -1
                        self.twin.set_speed(port, speed)
                        self.last_speeds[port] = speed
                        self._targets[port] = _MotorTarget(
                            target_deg=target_pos,
                            speed=speed,
                            direction=direction,
                        )
                return {"ok": "ack"}
            if cmd == "stop":
                async with self._lock:
                    port = req["port"]
                    self._targets.pop(port, None)
                    self.twin.stop(port)
                    self.last_speeds[port] = 0.0
                return {"ok": "ack"}
            if cmd == "brake":
                async with self._lock:
                    port = req["port"]
                    self._targets.pop(port, None)
                    self.twin.stop(port)
                    self.last_speeds[port] = 0.0
                return {"ok": "ack"}
            if cmd == "status":
                return {
                    "ok": "status",
                    "ports": [
                        {"port": p, "device": None, "last_speed": self.last_speeds[p]}
                        for p in range(6)
                    ],
                    "firmware": "pybricks",  # the sim pretends to be Pybricks
                    "current_skill": self.current_skill,
                }
            if cmd == "get_pose":
                # Debug-only: not part of the real motorctl protocol. The
                # smoke test uses this to assert the chassis actually moved.
                async with self._lock:
                    x, y, yaw = self.twin.chassis_pose()
                return {"ok": "pose", "x": x, "y": y, "yaw": yaw}
            if cmd == "load_skill":
                # In sim there's nothing to upload to; we just record it.
                self.current_skill = req["name"]
                return {"ok": "ack"}
            if cmd == "unload_skill":
                self.current_skill = None
                return {"ok": "ack"}
            if cmd in ("skill_message", "subscribe_skill_events"):
                return {"ok": "ack"}
            return {"ok": "err", "message": f"unknown cmd {cmd!r}"}
        except KeyError as e:
            return {"ok": "err", "message": f"missing field: {e}"}
        except Exception as e:  # pragma: no cover - defensive
            return {"ok": "err", "message": f"{type(e).__name__}: {e}"}

    async def serve_client(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        try:
            while True:
                line = await reader.readline()
                if not line:
                    return
                try:
                    req = json.loads(line)
                except json.JSONDecodeError as e:
                    resp = {"ok": "err", "message": f"bad json: {e}"}
                else:
                    resp = await self.handle(req)
                writer.write(json.dumps(resp).encode() + b"\n")
                await writer.drain()
        finally:
            writer.close()


async def _main(socket_path: Path) -> None:
    if socket_path.exists():
        socket_path.unlink()
    server_state = SimServer()
    await server_state.start()
    server = await asyncio.start_unix_server(server_state.serve_client, path=str(socket_path))
    print(f"sim_daemon listening on {socket_path}", file=sys.stderr)
    async with server:
        await server.serve_forever()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--socket",
        default=os.environ.get("MOTORCTL_SIM_SOCKET", "/tmp/motorctl-sim.sock"),
    )
    args = parser.parse_args()
    asyncio.run(_main(Path(args.socket)))


if __name__ == "__main__":
    main()
