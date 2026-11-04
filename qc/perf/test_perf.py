#!/usr/bin/env python3
"""Self-tests for the performance tooling (SPEC-016 T16-07 and the T-163 acceptance rows).
Run: python3 qc/perf/test_perf.py"""
import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)

import flight  # noqa: E402
import summarise  # noqa: E402


def load_plugin():
    spec = importlib.util.spec_from_file_location("perf_plugin", os.path.join(REPO, "tools", "validators", "perf.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def make_root(mutate=None):
    """Copy schemas and qc/perf into a throw-away root; `mutate(root)` may damage it. Returns the root."""
    root = tempfile.mkdtemp()
    shutil.copytree(os.path.join(REPO, "data", "schemas"), os.path.join(root, "data", "schemas"))
    shutil.copytree(os.path.join(HERE), os.path.join(root, "qc", "perf"), ignore=shutil.ignore_patterns("__pycache__"))
    if mutate:
        mutate(root)
    return root


def row(**kw):
    base = dict(id="H16-1-p99", scene="S1", machine="RM-R", metric="frame_us", stat="p99", op="le", value=16600,
                unit="us", locked=False, confidence="unverified", placeholder=True, calibration_target="CT-016-2")
    base.update(kw)
    return base


def write_run(runs, cls, scene, k, metric, values):
    d = os.path.join(runs, cls, scene)
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, "run%d.csv" % k), "a", encoding="utf-8") as f:
        if f.tell() == 0:
            f.write("metric,index,value\n")
        for i, v in enumerate(values):
            f.write("%s,%d,%d\n" % (metric, i, v))


class Percentiles(unittest.TestCase):
    def test_t16_07_one_to_thousand(self):
        s = list(range(1, 1001))
        got = {n: summarise.percentile(s, q) for n, q in (("p50", 500), ("p95", 950), ("p99", 990), ("p999", 999))}
        self.assertEqual(got, {"p50": 500, "p95": 950, "p99": 990, "p999": 999})
        self.assertEqual(summarise.stat_value(s, "max"), 1000)

    def test_fixture_through_the_command_line(self):
        out = subprocess.run([sys.executable, os.path.join(HERE, "summarise.py"), "percentiles",
                              os.path.join(HERE, "fixtures", "ramp_1_1000.csv")],
                             capture_output=True, text=True, check=True).stdout.split()
        self.assertEqual(out, ["sample", "p50", "500", "sample", "p95", "950", "sample", "p99", "990",
                               "sample", "p999", "999", "sample", "max", "1000"])

    def test_small_and_unsorted(self):
        self.assertEqual(summarise.percentile([7], 999), 7)
        self.assertEqual(summarise.percentile([9, 1, 5], 500), 5)
        self.assertEqual(summarise.stat_value([1, 2, 4], "mean"), 2)
        self.assertEqual(summarise.stat_value([1, 60000, 50000], "over_50000"), 1)
        with self.assertRaises(ValueError):
            summarise.percentile([], 500)

    def test_floats_are_refused(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "r.csv")
            with open(p, "w", encoding="utf-8") as f:
                f.write("metric,index,value\nframe_us,0,1.5\n")
            with self.assertRaises(ValueError):
                summarise.load_run(p)


class Thresholds(unittest.TestCase):
    def test_committed_rows_are_sound(self):
        with open(os.path.join(HERE, "thresholds.toml"), "rb") as f:
            rows = tomllib.load(f)["threshold"]
        hyps = {r["id"].split("-")[1] for r in rows}
        self.assertEqual(hyps, {str(n) for n in range(1, 12)})
        for r in rows:
            self.assertEqual(summarise.row_problems(r), [], r["id"])
            self.assertIs(r["locked"], False)
            self.assertIs(r["placeholder"], True)

    def test_row_without_calibration_target_fails(self):
        r = row()
        del r["calibration_target"]
        self.assertTrue(summarise.row_problems(r))
        with tempfile.TemporaryDirectory() as d:
            write_run(d, "RM-R", "S1", 1, "frame_us", [1] * 10)
            res = summarise.evaluate([r], d, ["RM-R"], warmup=0)
            self.assertEqual([x["verdict"] for x in res if x["scene"] == "S1"], ["FAIL"])
            self.assertIn("calibration target", res[0]["reason"])

    def test_median_of_three_and_worst(self):
        r = row(stat="max", value=100)
        with tempfile.TemporaryDirectory() as d:
            for k, top in ((1, 90), (2, 95), (3, 400)):
                write_run(d, "RM-R", "S1", k, "frame_us", [10, top])
            res = [x for x in summarise.evaluate([r], d, ["RM-R"], warmup=0) if x["scene"] == "S1"][0]
            self.assertEqual((res["value"], res["worst"], res["verdict"]), (95, 400, "PASS"))
            r2 = row(stat="max", value=94)
            res = [x for x in summarise.evaluate([r2], d, ["RM-R"], warmup=0) if x["scene"] == "S1"][0]
            self.assertEqual(res["verdict"], "FAIL")

    def test_missing_run_or_metric_fails(self):
        r = row(scene="S1-S2", stat="max", value=100)
        with tempfile.TemporaryDirectory() as d:
            for k in (1, 2, 3):
                write_run(d, "RM-R", "S1", k, "frame_us", [10])
            write_run(d, "RM-R", "S2", 1, "frame_us", [10])
            verdicts = {x["scene"]: x["verdict"] for x in summarise.evaluate([r], d, ["RM-R"], warmup=0)}
            self.assertEqual(verdicts, {"S1": "PASS", "S2": "FAIL"})

    def test_warm_up_frames_are_discarded(self):
        r = row(stat="max", value=100)
        with tempfile.TemporaryDirectory() as d:
            for k in (1, 2, 3):
                write_run(d, "RM-R", "S1", k, "frame_us", [9999] * 600 + [50] * 3600)
            res = [x for x in summarise.evaluate([r], d, ["RM-R"]) if x["scene"] == "S1"][0]
            self.assertEqual((res["value"], res["verdict"]), (50, "PASS"))

    def test_both_expands_to_two_classes(self):
        r = row(machine="both", scene="S0", stat="max", value=5)
        with tempfile.TemporaryDirectory() as d:
            res = summarise.evaluate([r], d, ["RM-R", "RM-M"], warmup=0)
            self.assertEqual(sorted({x["machine"] for x in res}), ["RM-M", "RM-R"])


class Flights(unittest.TestCase):
    def test_five_flights_load_with_integers_only(self):
        for n in range(1, 6):
            fl = flight.load_flight(os.path.join(HERE, "flights", "F%d.toml" % n))
            self.assertEqual(fl["id"], "F%d" % n)
            self.assertEqual(fl["frames"], 3600)
            for w in fl["waypoint"]:
                for v in w.values():
                    self.assertIs(type(v), int)

    def test_interpolation_is_integer_and_by_frame(self):
        fl = {"frames": 100, "waypoint": [dict(frame=0, x_cm=0, y_cm=0, z_cm=0, yaw_turn=0, pitch_turn=0),
                                          dict(frame=99, x_cm=99, y_cm=-99, z_cm=10, yaw_turn=65535, pitch_turn=-3)]}
        self.assertEqual(flight.pose_at(fl, 0)["x_cm"], 0)
        self.assertEqual(flight.pose_at(fl, 50)["y_cm"], -50)
        self.assertEqual(flight.pose_at(fl, 99)["yaw_turn"], 65535)
        self.assertEqual(flight.pose_at(fl, 1)["z_cm"], 0)

    def test_f1_walks_at_walking_pace(self):
        fl = flight.load_flight(os.path.join(HERE, "flights", "F1.toml"))
        a, b = flight.pose_at(fl, 0), flight.pose_at(fl, 60)
        self.assertEqual(a["z_cm"], 170)
        self.assertEqual(abs(b["x_cm"] - a["x_cm"]) + abs(b["y_cm"] - a["y_cm"]), 150)

    def test_f5_cuts_every_120_frames(self):
        fl = flight.load_flight(os.path.join(HERE, "flights", "F5.toml"))
        jumps = [f for f in range(1, 3600) if flight.pose_at(fl, f)["x_cm"] != flight.pose_at(fl, f - 1)["x_cm"]
                 or flight.pose_at(fl, f)["y_cm"] != flight.pose_at(fl, f - 1)["y_cm"]]
        self.assertEqual(jumps, list(range(120, 3600, 120)))
        stations = {(p["x_cm"], p["y_cm"], p["z_cm"]) for p in (flight.pose_at(fl, f) for f in range(0, 3600, 120))}
        self.assertEqual(len(stations), 8)


class Plugin(unittest.TestCase):
    def problems(self, mutate=None):
        root = make_root(mutate)
        try:
            return load_plugin().validate(root)
        finally:
            shutil.rmtree(root)

    def edit(self, rel, old, new):
        def mutate(root):
            p = os.path.join(root, "qc", "perf", rel)
            with open(p, encoding="utf-8") as f:
                text = f.read()
            self.assertIn(old, text)
            with open(p, "w", encoding="utf-8") as f:
                f.write(text.replace(old, new, 1))
        return mutate

    def test_committed_data_is_clean(self):
        self.assertEqual(self.problems(), [])

    def test_absent_directory_is_clean(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(load_plugin().validate(d), [])

    def test_float_in_flight(self):
        self.assertTrue(any("float" in p for p in self.problems(self.edit("flights/F2.toml", "z_cm = 1000", "z_cm = 1000.5"))))

    def test_unknown_key_in_flight(self):
        self.assertTrue(self.problems(self.edit("flights/F2.toml", "frames = 3600", "frames = 3600\nspeed = 3")))

    def test_waypoints_out_of_order(self):
        self.assertTrue(self.problems(self.edit("flights/F4.toml", "frame = 600", "frame = 0")))

    def test_threshold_without_calibration_target(self):
        mutate = self.edit("thresholds.toml", ', calibration_target = "CT-016-2" }', " }")
        self.assertTrue(any("calibration" in p for p in self.problems(mutate)))

    def test_threshold_float_and_unknown_op(self):
        self.assertTrue(any("float" in p for p in self.problems(self.edit("thresholds.toml", "value = 13500", "value = 13500.0"))))
        self.assertTrue(self.problems(self.edit("thresholds.toml", 'op = "le"', 'op = "lt"')))

    def test_machine_record_fields(self):
        def mutate(root):
            with open(os.path.join(root, "qc", "perf", "machine.toml"), "a", encoding="utf-8") as f:
                f.write('[[machine]]\nid = "rm-r-01"\ncpu_model = "x"\ncpu_cores = 8\nram_mb = 32768\ngpu_model = "y"\n'
                        'vram_mb = 8192\ndriver = "1"\nos_build = "2"\ninternal_w = 2560\ninternal_h = 1440\n'
                        'upscaler = "off"\ndate = 20261104\n')
        self.assertEqual(self.problems(mutate), [])
        def bad(root):
            mutate(root)
            with open(os.path.join(root, "qc", "perf", "machine.toml"), "a", encoding="utf-8") as f:
                f.write('[[machine]]\nid = "rm-r-01"\n')
        self.assertTrue(self.problems(bad))


if __name__ == "__main__":
    unittest.main()
