#!/usr/bin/env python3
"""Tests for tools/validators/environment.py and for the committed outputs of qc/fixtures/ref_exp.py and
ref_weather.py (the Python side of T9-01 and T9-03).
Run: python3 tools/test_environment_validator.py"""
import contextlib
import importlib.util
import io
import os
import re
import shutil
import sys
import tempfile
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(TOOLS)
FIXTURES = os.path.join(REPO, "qc", "fixtures")


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


plugin = load("environment_validator", os.path.join(TOOLS, "validators", "environment.py"))
ref_exp = load("ref_exp", os.path.join(FIXTURES, "ref_exp.py"))
ref_weather = load("ref_weather", os.path.join(FIXTURES, "ref_weather.py"))
ENV = os.path.join("data", "rules", "environment.toml")
WEATHER = os.path.join("data", "scenarios", "gettysburg", "weather.toml")


def read(rel):
    with open(os.path.join(REPO, rel), encoding="utf-8") as f:
        return f.read()


def run(env_text=None, weather_text=None):
    """Build a throw-away root with the real schemas and terrain table; return the plugin's problems."""
    with tempfile.TemporaryDirectory() as root:
        shutil.copytree(os.path.join(REPO, "data", "schemas"), os.path.join(root, "data", "schemas"))
        os.makedirs(os.path.join(root, "data", "rules"))
        shutil.copy(os.path.join(REPO, "data", "rules", "terrain.toml"), os.path.join(root, "data", "rules"))
        for rel, text in ((ENV, env_text), (WEATHER, weather_text)):
            if text is not None:
                os.makedirs(os.path.dirname(os.path.join(root, rel)), exist_ok=True)
                with open(os.path.join(root, rel), "w", encoding="utf-8") as f:
                    f.write(text)
        return plugin.validate(root)


RELIEF = (
    'schema = "relief/1"\ncell_mm = 8000\norigin_x_mm = 0\norigin_y_mm = 0\nconfidence = "unverified"\n'
    'placeholder = true\ncalibration_target = "CT-009-3"\n[[row]]\ndm = "0 10 -5"\n[[row]]\ndm = "3 0 0"\n'
)


def run_relief(text):
    """Problems the plugin reports for one relief sidecar in a throw-away root."""
    with tempfile.TemporaryDirectory() as root:
        shutil.copytree(os.path.join(REPO, "data", "schemas"), os.path.join(root, "data", "schemas"))
        path = os.path.join(root, "data", "scenarios", "demo", "relief.toml")
        os.makedirs(os.path.dirname(path))
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)
        return plugin.validate(root)


def output(fn):
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        fn()
    return buf.getvalue()


class Data(unittest.TestCase):
    def assertProblem(self, problems, word):
        self.assertTrue(any(word in p for p in problems), "%r not in %r" % (word, problems))

    def env(self, old, new):
        text = read(ENV)
        self.assertIn(old, text)
        return run(env_text=text.replace(old, new, 1))

    def weather(self, old, new):
        text = read(WEATHER)
        self.assertIn(old, text)
        return run(weather_text=text.replace(old, new, 1))

    def test_committed_data_pass(self):
        self.assertEqual(run(read(ENV), read(WEATHER)), [])
        self.assertEqual(plugin.validate(REPO), [])

    def test_environment_rules(self):
        self.assertProblem(self.env("drain_pm = 1100", "drain_pm = 900"), "drain_pm")
        self.assertProblem(self.env("rest_pm = 950", "rest_pm = 1100"), "rest_pm")
        self.assertProblem(self.env("apparent_f10 = 800", "apparent_f10 = 700"), "apparent_f10")
        self.assertProblem(self.env("{ class = 7,", "{ class = 99,"), "class 99")
        self.assertProblem(self.env("{ class = 11,", "{ class = 12,"), "class 11 has no retain row")
        self.assertProblem(self.env("diffuse_ppm = 10000", "diffuse_ppm = 80000"), "flow sum")
        self.assertProblem(self.env('"CT-009-5"', '"CT-007-5"'), "does not belong to spec 009")
        self.assertProblem(self.env('calibration_target = "CT-009-5"\n', ""), "provenance")

    def test_weather_rules(self):
        self.assertProblem(self.weather("t_min = 270", "t_min = 0"), "ascending")
        self.assertProblem(self.weather("wind_x_mm_s = 400, wind_y_mm_s = 200", "wind_x_mm_s = 15000, wind_y_mm_s = 15000"), "wind speed")
        self.assertProblem(self.weather("temp_f10 = 680", "temp_f10 = 680.5"), "float")
        self.assertProblem(self.weather("rain_pm = 0", "rain_pm = 1001"), "schema")
        self.assertProblem(self.weather('{ document = "CWV", page = 79, note = "hot (high 80s F)" }', '{ document = "CWV", page = 80 }'), "880 key")
        self.assertProblem(self.weather('calibration_target = "CT-009-6"', 'note = "x"'), "provenance")

    def test_relief_rules(self):
        self.assertEqual(run_relief(RELIEF), [])
        self.assertProblem(run_relief(RELIEF.replace('"3 0 0"', '"3 0"')), "rows differ")
        self.assertProblem(run_relief(RELIEF.replace('"3 0 0"', '"3 0 20000"')), "decimetres")
        self.assertProblem(run_relief(RELIEF.replace('"3 0 0"', '"3 0 1.5"')), "schema")
        self.assertProblem(run_relief(RELIEF.replace("cell_mm = 8000\n", "")), "schema")
        self.assertProblem(run_relief(RELIEF + "extra = 1\n"), "schema")
        self.assertProblem(run_relief(RELIEF.replace('calibration_target = "CT-009-3"\n', "")), "provenance")


class Fixtures(unittest.TestCase):
    """The committed outputs equal a fresh run of the scripts, and the Rust constant equals the table."""

    def committed(self, name):
        with open(os.path.join(FIXTURES, name), encoding="utf-8") as f:
            return f.read()

    def test_ref_exp_csv_is_current(self):
        self.assertEqual(output(ref_exp.main), self.committed("ref_exp.csv"))

    def test_ref_weather_csv_is_current(self):
        self.assertEqual(output(ref_weather.main), self.committed("ref_weather.csv"))

    def test_rust_constant_equals_the_regenerated_table(self):
        src = read(os.path.join("crates", "battle-rules", "src", "environment.rs"))
        body = re.search(r"EXP_TABLE: \[u32; 129\] = \[(.*?)\];", src, re.S).group(1)
        self.assertEqual([int(x) for x in re.findall(r"\d+", body)], ref_exp.table())


if __name__ == "__main__":
    unittest.main()
