"""Tighter differential-drive control running on the hub itself.

The Pi cortex broadcasts a target body velocity (forward, yaw) on
channel 1 via Pybricks BLE messaging; this skill closes the loop on
the hub at 100 Hz so we don't pay BLE latency per update.

Message format: `"target <v_fwd> <yaw_rate>"` where v_fwd is in m/s
and yaw_rate is in rad/s. Send `"stop"` to coast.
"""

from pybricks.hubs import PrimeHub
from pybricks.pupdevices import Motor
from pybricks.parameters import Port, Direction
from pybricks.tools import wait

# --- robot calibration ----------------------------------------------------
WHEEL_DIAMETER_MM = 56.0
WHEELBASE_MM = 120.0
MAX_WHEEL_DEG_PER_SEC = 1000.0

# --- PID gains (tune in the digital twin first, then on hardware) --------
KP = 1.5
KI = 0.05
KD = 0.10

# --- ports ----------------------------------------------------------------
LEFT_PORT = Port.A
RIGHT_PORT = Port.B
LEFT_DIR = Direction.COUNTERCLOCKWISE
RIGHT_DIR = Direction.CLOCKWISE

hub = PrimeHub()
left = Motor(LEFT_PORT, LEFT_DIR)
right = Motor(RIGHT_PORT, RIGHT_DIR)
CHANNEL = 1
hub.ble.observe(CHANNEL)

import math

MM_PER_DEG = math.pi * WHEEL_DIAMETER_MM / 360.0


def body_to_wheel_deg_per_s(v_fwd: float, yaw_rate: float) -> tuple[float, float]:
    # v_left = v_fwd - yaw_rate * (wheelbase / 2);   v_right = v_fwd + yaw_rate * (wheelbase / 2)
    half_wb_m = (WHEELBASE_MM / 1000.0) / 2.0
    v_l = v_fwd - yaw_rate * half_wb_m
    v_r = v_fwd + yaw_rate * half_wb_m
    # m/s -> mm/s -> deg/s
    return v_l * 1000.0 / MM_PER_DEG, v_r * 1000.0 / MM_PER_DEG


target_l_dps = 0.0
target_r_dps = 0.0
e_l_int = 0.0
e_r_int = 0.0
e_l_prev = 0.0
e_r_prev = 0.0
DT = 0.01  # 100 Hz

while True:
    msg = hub.ble.observe(CHANNEL)
    if msg:
        try:
            parts = msg.split()
            if parts[0] == "target" and len(parts) == 3:
                v_fwd = float(parts[1])
                yaw = float(parts[2])
                target_l_dps, target_r_dps = body_to_wheel_deg_per_s(v_fwd, yaw)
            elif parts[0] == "stop":
                target_l_dps = target_r_dps = 0.0
        except (ValueError, IndexError):
            pass

    e_l = target_l_dps - left.speed()
    e_r = target_r_dps - right.speed()
    e_l_int = max(min(e_l_int + e_l * DT, MAX_WHEEL_DEG_PER_SEC), -MAX_WHEEL_DEG_PER_SEC)
    e_r_int = max(min(e_r_int + e_r * DT, MAX_WHEEL_DEG_PER_SEC), -MAX_WHEEL_DEG_PER_SEC)
    de_l = (e_l - e_l_prev) / DT
    de_r = (e_r - e_r_prev) / DT
    u_l = KP * e_l + KI * e_l_int + KD * de_l
    u_r = KP * e_r + KI * e_r_int + KD * de_r
    e_l_prev, e_r_prev = e_l, e_r

    pct_l = max(min(u_l / MAX_WHEEL_DEG_PER_SEC * 100.0, 100.0), -100.0)
    pct_r = max(min(u_r / MAX_WHEEL_DEG_PER_SEC * 100.0, 100.0), -100.0)
    left.dc(pct_l)
    right.dc(pct_r)

    wait(int(DT * 1000))
