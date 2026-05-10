"""Sim daemon: speaks the same Unix-socket protocol as the real motorctl.

Run this in place of the Rust daemon to drive the cortex against a
simulated robot. The wire format matches `crates/motorctl/src/proto.rs`
so `MOTORCTL_SOCKET=... python -m backpack '...'` runs unchanged.

LoadSkill / UnloadSkill / SkillMessage are accepted but stored only as
metadata in this scaffold — there's nothing to upload to. Skill
training still runs against the Gym env directly.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
from pathlib import Path

from .world import Twin


class SimServer:
    def __init__(self) -> None:
        self.twin = Twin()
        self.last_speeds = [0.0] * 6
        self.current_skill: str | None = None
        # Background task that steps physics so set_speed actions persist.
        self._step_task: asyncio.Task | None = None

    async def start(self) -> None:
        self._step_task = asyncio.create_task(self._step_loop())

    async def _step_loop(self) -> None:
        while True:
            self.twin.step(0.02)
            await asyncio.sleep(0.02)

    async def handle(self, req: dict) -> dict:
        cmd = req.get("cmd")
        try:
            if cmd == "set_speed":
                self.twin.set_speed(req["port"], req["speed"])
                self.last_speeds[req["port"]] = req["speed"]
                return {"ok": "ack"}
            if cmd == "run_for_degrees":
                self.twin.run_for_degrees(req["port"], req["degrees"], req["speed"])
                return {"ok": "ack"}
            if cmd == "stop":
                self.twin.stop(req["port"])
                self.last_speeds[req["port"]] = 0.0
                return {"ok": "ack"}
            if cmd == "brake":
                self.twin.stop(req["port"])
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
