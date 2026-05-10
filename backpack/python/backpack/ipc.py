"""Client for the motorctl Unix-socket IPC.

The wire format is one JSON object per line. See
`crates/motorctl/src/proto.rs` for the canonical schema.
"""

from __future__ import annotations

import json
import socket
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterator

DEFAULT_SOCKET = "/run/motorctl.sock"


@dataclass
class MotorCtlError(RuntimeError):
    message: str

    def __str__(self) -> str:  # pragma: no cover - trivial
        return self.message


class MotorCtl:
    """Synchronous client. One request, one response, blocking."""

    def __init__(self, path: str = DEFAULT_SOCKET) -> None:
        self._path = path
        self._sock: socket.socket | None = None
        self._buf = b""

    def connect(self) -> None:
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.connect(self._path)
        self._sock = s

    def close(self) -> None:
        if self._sock is not None:
            self._sock.close()
            self._sock = None

    def __enter__(self) -> "MotorCtl":
        self.connect()
        return self

    def __exit__(self, *_: Any) -> None:
        self.close()

    # --- direct motor control ---------------------------------------------

    def set_speed(self, port: int, speed: float) -> None:
        self._call({"cmd": "set_speed", "port": port, "speed": speed})

    def run_for_degrees(self, port: int, degrees: int, speed: float) -> None:
        self._call({
            "cmd": "run_for_degrees",
            "port": port,
            "degrees": degrees,
            "speed": speed,
        })

    def stop(self, port: int) -> None:
        self._call({"cmd": "stop", "port": port})

    def brake(self, port: int) -> None:
        self._call({"cmd": "brake", "port": port})

    def status(self) -> dict[str, Any]:
        return self._call({"cmd": "status"})

    # --- muscle memory (Pybricks only) ------------------------------------

    def load_skill(self, name: str, code: str | Path) -> None:
        """Push a skill (Pybricks Python program) to the hub and start it.

        `code` may be a string of Python source or a Path to a .py file.
        Requires Pybricks firmware on the hub.
        """
        if isinstance(code, Path):
            code = code.read_text()
        self._call({"cmd": "load_skill", "name": name, "code": code})

    def unload_skill(self) -> None:
        self._call({"cmd": "unload_skill"})

    def skill_message(self, payload: str) -> None:
        self._call({"cmd": "skill_message", "payload": payload})

    # --- transport --------------------------------------------------------

    def _call(self, req: dict[str, Any]) -> dict[str, Any]:
        if self._sock is None:
            self.connect()
        assert self._sock is not None
        self._sock.sendall((json.dumps(req) + "\n").encode())
        line = self._readline()
        resp = json.loads(line)
        if resp.get("ok") == "err":
            raise MotorCtlError(resp.get("message", "unknown error"))
        return resp

    def _readline(self) -> bytes:
        assert self._sock is not None
        while b"\n" not in self._buf:
            chunk = self._sock.recv(4096)
            if not chunk:
                raise ConnectionError("motorctl closed the socket")
            self._buf += chunk
        line, _, rest = self._buf.partition(b"\n")
        self._buf = rest
        return line


@contextmanager
def open_motorctl(path: str = DEFAULT_SOCKET) -> Iterator[MotorCtl]:
    client = MotorCtl(path)
    client.connect()
    try:
        yield client
    finally:
        client.close()
