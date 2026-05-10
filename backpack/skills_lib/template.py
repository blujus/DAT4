"""Minimal Pybricks skill template.

Copy this file, rename it, and replace the body of `tick()`. The
cortex pushes this to the hub via `motorctl.load_skill(name, code)`,
and the hub runs it as the user program. The skill stays resident
until `unload_skill` arrives.
"""

from pybricks.hubs import PrimeHub
from pybricks.pupdevices import Motor
from pybricks.parameters import Port
from pybricks.tools import wait

hub = PrimeHub()

# Adjust to whichever ports your build uses.
left = Motor(Port.A)
right = Motor(Port.B)

# Pybricks BLE messaging channel. The Pi cortex broadcasts on this
# channel via `motor.skill_message(...)`; we listen here.
CHANNEL = 1
hub.ble.observe(CHANNEL)


def tick(message: str | None) -> None:
    """Called every control cycle. `message` is the latest cortex
    message, or `None` if nothing arrived since last tick."""
    if message:
        # parse / dispatch on cortex commands here
        pass
    # control loop body goes here


last_message: str | None = None
while True:
    msg = hub.ble.observe(CHANNEL)
    if msg is not None:
        last_message = msg
    tick(last_message)
    wait(10)  # 100 Hz control rate
