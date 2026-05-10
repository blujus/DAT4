"""Self-balance reflex. Stub.

Keeps a two-wheel balancing robot upright using the hub's IMU and the
drive motors. The Pi cortex sets a desired heading via BLE messaging;
this skill handles the inverted-pendulum control loop.

This is a starting point — the gains here are placeholders. Train them
in the digital twin (`twin/twin/train_skill.py`) and distill back via
`twin/twin/distill.py`.
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

# Placeholder gains — do not run on a real robot until tuned.
KP_PITCH = 8.0
KD_PITCH = 0.5
KP_RATE = 0.4

target_yaw_rate = 0.0
pitch_offset_deg = 0.0  # mounting bias

while True:
    msg = hub.ble.observe(CHANNEL)
    if msg:
        try:
            parts = msg.split()
            if parts[0] == "yaw" and len(parts) == 2:
                target_yaw_rate = float(parts[1])
        except (ValueError, IndexError):
            pass

    pitch = hub.imu.tilt()[0] - pitch_offset_deg          # degrees
    pitch_rate = hub.imu.angular_velocity()[1]            # deg/s about X

    base = KP_PITCH * pitch + KD_PITCH * pitch_rate
    diff = KP_RATE * target_yaw_rate
    pct_l = max(min(base - diff, 100.0), -100.0)
    pct_r = max(min(base + diff, 100.0), -100.0)
    left.dc(pct_l)
    right.dc(pct_r)
    wait(10)
