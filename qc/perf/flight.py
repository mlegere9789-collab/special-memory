"""Camera flights for the M1 performance runs (SPEC-016 sections 2 and 3.3). Integers only.

A flight file holds `schema`, `id`, `frames` and `[[waypoint]]` rows `{frame, x_cm, y_cm, z_cm, yaw_turn, pitch_turn}`.
Positions are slab-local centimetres (origin at the south-west corner, x east, y north); z_cm is the height above the
ground under the camera. Angles are 1/65536 turns: yaw 0 looks north and grows clockwise (16384 looks east); yaw is
unwrapped, so a full circle runs 0 to 65536; pitch 0 is level and negative looks down.

`pose_at` interpolates linearly by frame index with floor division, never by clock. Two waypoints on consecutive
frames make a cut.
"""
import tomllib

KEYS = ("frame", "x_cm", "y_cm", "z_cm", "yaw_turn", "pitch_turn")


def load_flight(path):
    with open(path, "rb") as f:
        fl = tomllib.load(f)
    for w in fl.get("waypoint", []):
        for k in KEYS:
            if type(w.get(k)) is not int:
                raise ValueError("%s: waypoint field %s must be an integer" % (path, k))
    if type(fl.get("frames")) is not int or not fl.get("waypoint"):
        raise ValueError("%s: frames and waypoints required" % path)
    return fl


def pose_at(flight, frame):
    """Pose (dict of the five integer fields) at an integer frame index."""
    wps = flight["waypoint"]
    if frame <= wps[0]["frame"]:
        return {k: wps[0][k] for k in KEYS[1:]}
    for a, b in zip(wps, wps[1:]):
        if frame <= b["frame"]:
            span, t = b["frame"] - a["frame"], frame - a["frame"]
            return {k: (a[k] * (span - t) + b[k] * t) // span for k in KEYS[1:]}
    return {k: wps[-1][k] for k in KEYS[1:]}
