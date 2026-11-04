#!/usr/bin/env python3
"""Self-test: the committed survey fixtures satisfy the survey schemas, and each broken variant is refused.

Run from the repository root: python3 qc/fixtures/terrain/test_schemas.py"""
import copy
import json
import os
import sys
import tomllib
import unittest

from jsonschema import Draft202012Validator
from referencing import Registry, Resource

HERE = os.path.dirname(os.path.abspath(__file__))
SCHEMAS = os.path.join(HERE, "..", "..", "..", "data", "schemas")
FILES = {"survey.toml": "survey", "landcover.toml": "survey-landcover", "linear.toml": "survey-linear",
         "buildings.toml": "survey-buildings", "names.toml": "survey-names"}


def registry():
    reg = Registry()
    for n in sorted(os.listdir(SCHEMAS)):
        if n.endswith(".schema.json"):
            with open(os.path.join(SCHEMAS, n), encoding="utf-8") as f:
                reg = reg.with_resource(n, Resource.from_contents(json.load(f)))
    return reg


def validator(kind):
    with open(os.path.join(SCHEMAS, kind + ".schema.json"), encoding="utf-8") as f:
        return Draft202012Validator(json.load(f), registry=registry())


def load(*parts):
    with open(os.path.join(HERE, *parts), "rb") as f:
        return tomllib.load(f)


class SurveySchemas(unittest.TestCase):
    def test_synthetic_files_validate(self):
        for name, kind in FILES.items():
            errs = list(validator(kind).iter_errors(load("synthetic", name)))
            self.assertEqual(errs, [], name)

    def test_negative_linear_fixtures_are_schema_valid(self):
        for d in sorted(os.listdir(os.path.join(HERE, "negative"))):
            self.assertEqual(list(validator("survey-linear").iter_errors(load("negative", d, "linear.toml"))), [], d)

    def refused(self, name, mutate):
        doc = copy.deepcopy(load("synthetic", name))
        mutate(doc)
        self.assertTrue(list(validator(FILES[name]).iter_errors(doc)))

    def test_broken_variants_are_refused(self):
        self.refused("landcover.toml", lambda d: d["polygon"][0].update(extra=1))
        self.refused("landcover.toml", lambda d: d["polygon"][0].update(**{"class": "open"}))
        self.refused("landcover.toml", lambda d: d["polygon"][0].pop("sources"))
        self.refused("landcover.toml", lambda d: d["polygon"][0].update(confidence="verified"))
        self.refused("landcover.toml", lambda d: d["polygon"][0].update(outer=[[1.5, 2]]))
        self.refused("linear.toml", lambda d: d["line"][0].pop("half_width_mm"))
        self.refused("linear.toml", lambda d: d["line"][1].update(dead_end=True))
        self.refused("linear.toml", lambda d: d["line"][2].update(half_width_mm=10))
        self.refused("buildings.toml", lambda d: d["building"][0].pop("height_mm"))
        self.refused("names.toml", lambda d: d["name"][0].pop("anchor"))
        self.refused("survey.toml", lambda d: d.update(tiles_x=0))
        self.refused("linear.toml", lambda d: d["line"][0]["sources"][0].update(page=1))


if __name__ == "__main__":
    unittest.main()
