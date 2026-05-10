"""gRPC client for the amygdala motorctl bridge.

Let the cortex run anywhere within ping distance of the robot. The
wire format is `proto/backpack.proto`; this module wraps the
generated stubs in a class that mirrors `MotorCtl` (the Unix-socket
client) so cortex code is transport-agnostic:

    # local (cortex on the robot)
    with open_motorctl("unix:///run/motorctl.sock") as m: ...

    # remote (cortex on a laptop or in the cloud)
    with open_motorctl("grpc://amygdala.local:50051") as m: ...

Status: stub. To finish:

  1. `pip install grpcio grpcio-tools` (declare under the `remote`
     extra in pyproject.toml).
  2. Generate stubs into `python/backpack/_grpc/`:
        python -m grpc_tools.protoc \\
            --proto_path=../proto \\
            --python_out=backpack/_grpc \\
            --grpc_python_out=backpack/_grpc \\
            ../proto/backpack.proto
  3. Implement `GrpcMotorCtl` below to delegate each method to the
     generated `BrickLinkStub`. Translate the proto enum/oneof shape
     back into the same dict the Unix-socket client returns from
     `status()`, so callers don't notice the swap.
  4. Update `open_motorctl()` in `ipc.py` to dispatch on URL scheme.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass
class GrpcMotorCtl:
    """Placeholder. Will mirror `MotorCtl` from `backpack.ipc`."""

    target: str  # e.g. "amygdala.local:50051"

    def connect(self) -> None:
        raise NotImplementedError(
            "backpack.grpc_client is a stub. "
            "See module docstring for the implementation steps."
        )

    # The full MotorCtl-compatible surface lives here once implemented:
    #   set_speed / run_for_degrees / stop / brake / status
    #   load_skill / unload_skill / skill_message

    def close(self) -> None:
        pass

    def __enter__(self) -> "GrpcMotorCtl":
        self.connect()
        return self

    def __exit__(self, *_: Any) -> None:
        self.close()
