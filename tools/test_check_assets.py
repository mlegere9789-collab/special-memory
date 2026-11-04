#!/usr/bin/env python3
"""Tests for tools/check_clips.py, tools/check_rights.py and tools/validators/assets.py (SPEC-016 T16-11, T16-12, T16-13).
Run: python3 tools/test_check_assets.py"""
import copy
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(TOOLS)
GOOD = os.path.join(REPO, "qc", "fixtures", "provenance", "good")
sys.path.insert(0, TOOLS)
import check_clips  # noqa: E402
import check_rights  # noqa: E402

GOOD_NAMES = ["inf_fire_cycle_stand_veteran_v01", "mov_walk_flat_dry_v02", "rea_fall_back_dead_v10", "art_load_ram_no3_v01", "cav_dismount_hold_horse_v03",
              "off_signal_flag_left_right_up_down_v01", "grp_wheel_line_left_v04", "ufr_take_cover_kneel_v01", "mel_bayonet_thrust_pair_v01", "car_stretcher_pair_lift_v01"]
BAD_NAMES = ["Inf_fire_cycle_stand_v01", "inf_fire_v01", "inf_fire_cycle_stand_veteran", "infantry_fire_cycle_v01", "inf_fire_cycle_stand_v1",
             "inf-fire-cycle-stand-v01", "inf_fire_cycle_stand_v001", "in_fire_cycle_stand_v01", "inf_fire__cycle_v01", "inf_a_b_c_d_e_f_g_h_v01"]


def load(name):
    with open(os.path.join(GOOD, name), "rb") as f:
        return tomllib.load(f)


def good():
    return {"asset": load("data/provenance/assets.toml")["asset"], "consent": load("data/provenance/consents.toml")["consent"],
            "recording": load("data/audio/recordings.toml")["recording"], "clip": load("data/animation/clips.toml")["clip"]}


def full_set():
    """A synthetic manifest that meets section 6.1 exactly, with 12 performers rotating."""
    consents = [dict(good()["consent"][0], id="c%02d" % i, person_ref="p%02d" % i) for i in range(12)]
    clips, n = [], 0
    for cat, count in check_clips.COUNTS.items():
        for i in range(count):
            word = check_clips.REACTIONS[n % 8] if n < 8 else "x"
            p = n % 12
            clips.append(dict(good()["clip"][0], id="%s_%s_y_%d_v01" % (cat, word, i), category=cat, performer="p%02d" % p, consent="c%02d" % p))
            n += 1
    return clips, consents


class Clips(unittest.TestCase):
    def test_ten_names_accepted_and_ten_rejected(self):
        self.assertEqual([n for n in GOOD_NAMES if not check_clips.NAME_RE.match(n)], [])
        self.assertEqual([n for n in BAD_NAMES if check_clips.NAME_RE.match(n)], [])
        self.assertEqual((len(GOOD_NAMES), len(BAD_NAMES)), (10, 10))

    def test_good_fixture_passes(self):
        d = good()
        self.assertEqual(check_clips.check(d["clip"], d["consent"]), [])

    def test_missing_and_unsigned_consent(self):
        d = good()
        d["clip"][0]["consent"] = "cns-404"
        d["consent"][1]["signed"] = 0
        out = "\n".join(check_clips.check(d["clip"], d["consent"]))
        self.assertIn("does not exist", out)
        self.assertIn("not signed", out)

    def test_consent_of_another_performer(self):
        d = good()
        d["clip"][0]["consent"] = "cns-002"
        self.assertIn("another performer", "\n".join(check_clips.check(d["clip"], d["consent"])))

    def test_performer_share_over_a_quarter(self):
        d = good()
        clips = [dict(d["clip"][0], id="inf_fire_cycle_stand_v%02d" % i, performer="performer-0%d" % (1 if i < 2 else 2),
                      consent="cns-001" if i < 2 else "cns-002") for i in range(4)]
        out = "\n".join(check_clips.check(clips, d["consent"]))
        self.assertIn("limit 25 percent", out)
        self.assertNotIn("limit 25 percent", "\n".join(check_clips.check(clips[:3], d["consent"])))

    def test_frames_fps_and_contacts(self):
        d = good()
        d["clip"][0].update(frames=29, fps=30, contact_l=[5, 3], contact_r=[240])
        out = "\n".join(check_clips.check(d["clip"], d["consent"]))
        for text in ("outside 30..1800", "fps 30", "contact_l", "contact_r"):
            self.assertIn(text, out)

    def test_counts_must_match_section_6_1(self):
        clips, consents = full_set()
        self.assertEqual(sum(check_clips.COUNTS.values()), 300)
        self.assertEqual(check_clips.check(clips, consents, full=True), [])
        out = "\n".join(check_clips.check(clips[:-1], consents, full=True))
        self.assertIn("section 6.1 lists", out)

    def test_missing_reaction(self):
        clips, consents = full_set()
        clips = [dict(c, id=c["id"].replace("_wheel_", "_turn_")) for c in clips]
        self.assertIn("reaction 'wheel'", "\n".join(check_clips.check(clips, consents, full=True)))


class Rights(unittest.TestCase):
    def test_good_fixture_passes(self):
        self.assertEqual(check_rights.check(good()), [])

    def case(self, kind, **change):
        d = good()
        d[kind][0].update(change)
        return "\n".join(check_rights.check(d))

    def test_unknown_licence(self):
        self.assertIn("unknown licence", self.case("asset", licence="all-rights-reserved"))

    def test_ai_tool_without_human_authorship(self):
        self.assertIn("without human_authored", self.case("asset", ai_tool="image-tool", human_authored=False, **{"class": "terrain"}))
        self.assertEqual(self.case("asset", ai_tool="image-tool"), "")

    def test_character_needs_human_authorship(self):
        self.assertIn("needs human_authored", self.case("asset", human_authored=False))

    def test_voice_row_without_consent(self):
        d = good()
        d["recording"][0]["consent"] = ""
        self.assertIn("voice row has no consent", "\n".join(check_rights.check(d)))
        d["recording"][0]["consent"] = "cns-404"
        self.assertIn("does not exist", "\n".join(check_rights.check(d)))

    def test_replica_without_counsel_reference(self):
        out = self.case("consent", id="cns-001", replica_allowed=True, counsel_ref="")
        self.assertIn("replica_allowed = true without a counsel_ref", out)
        self.assertIn("ml_training_allowed", self.case("consent", ml_training_allowed=True))

    def test_bad_dates_and_duplicate_ids(self):
        self.assertIn("not a calendar date", self.case("asset", rights_check=20261301))
        d = good()
        d["asset"].append(copy.deepcopy(d["asset"][0]))
        self.assertIn("duplicate id", "\n".join(check_rights.check(d)))

    def test_cli_exit_codes(self):
        run = lambda *a: subprocess.run([sys.executable, *a], capture_output=True, text=True, check=False)
        self.assertEqual(run(os.path.join(TOOLS, "check_rights.py"), GOOD).returncode, 0)
        self.assertEqual(run(os.path.join(TOOLS, "check_clips.py"), GOOD).returncode, 0)
        self.assertEqual(run(os.path.join(TOOLS, "check_clips.py"), "--full", GOOD).returncode, 1)
        with tempfile.TemporaryDirectory() as root:
            shutil.copytree(GOOD, root, dirs_exist_ok=True)
            p = os.path.join(root, "data", "provenance", "assets.toml")
            with open(p, encoding="utf-8") as f:
                text = f.read().replace("assigned-ip", "unknown-terms")
            with open(p, "w", encoding="utf-8") as f:
                f.write(text)
            r = run(os.path.join(TOOLS, "check_rights.py"), root)
            self.assertEqual(r.returncode, 1)
            self.assertIn("unknown licence", r.stdout)


def validate(edit=None, skeleton_edit=None):
    """Run validate_data.py on a throw-away root: real schemas, the good fixture, the real skeleton."""
    with tempfile.TemporaryDirectory() as root:
        shutil.copytree(os.path.join(REPO, "data", "schemas"), os.path.join(root, "data", "schemas"))
        shutil.copytree(GOOD, root, dirs_exist_ok=True)
        shutil.copy(os.path.join(REPO, "data", "animation", "skeleton_soldier_v1.toml"), os.path.join(root, "data", "animation"))
        for rel, fn in (("data/provenance/assets.toml", edit), ("data/animation/skeleton_soldier_v1.toml", skeleton_edit)):
            if fn:
                p = os.path.join(root, rel)
                with open(p, encoding="utf-8") as f:
                    text = f.read()
                with open(p, "w", encoding="utf-8") as f:
                    f.write(fn(text))
        p = subprocess.run([sys.executable, os.path.join(TOOLS, "validate_data.py"), root], capture_output=True, text=True, check=False)
        return p.returncode, p.stdout


class Plugin(unittest.TestCase):
    def test_good_root_passes(self):
        code, out = validate()
        self.assertEqual(code, 0, out)

    def test_float_is_refused(self):
        code, out = validate(lambda t: t.replace("rights_check = 20260401", "rights_check = 20260401.0"))
        self.assertEqual(code, 1)
        self.assertIn("float", out)

    def test_unknown_key_is_refused(self):
        code, out = validate(lambda t: t.replace('gate = "G5"', 'gate = "G5"\nmystery = 1', 1))
        self.assertEqual(code, 1)
        self.assertIn("mystery", out)

    def test_provenance_rule_applies(self):
        code, out = validate(lambda t: t.replace('confidence = "partial"', 'confidence = "verified"'))
        self.assertEqual(code, 1)
        self.assertIn("provenance", out)

    def test_semantic_rule_reaches_the_validator(self):
        code, out = validate(lambda t: t.replace("assigned-ip", "unknown-terms"))
        self.assertEqual(code, 1)
        self.assertIn("unknown licence", out)

    def test_repository_files_validate(self):
        p = subprocess.run([sys.executable, os.path.join(TOOLS, "validate_data.py")], capture_output=True, text=True, check=False)
        self.assertEqual(p.returncode, 0, p.stdout)


class Skeleton(unittest.TestCase):
    def test_shipped_skeleton(self):
        with open(os.path.join(REPO, "data", "animation", "skeleton_soldier_v1.toml"), "rb") as f:
            rec = tomllib.load(f)
        names = [j["name"] for j in rec["joint"]]
        self.assertEqual((len(names), len(set(names))), (64, 64))
        self.assertTrue(all(j["parent"] < i for i, j in enumerate(rec["joint"]) if i))
        self.assertEqual(sum(j["tier_b"] for j in rec["joint"]), 28)
        self.assertEqual(len([n for n in names if n.startswith("socket_")]), 5)
        self.assertEqual(len([n for n in names if "twist" in n]), 6)
        self.assertEqual(len([n for n in names if n.split("_")[0] in ("thumb", "index", "middle", "ring", "pinky")]), 30)

    def test_plugin_rejects_broken_skeletons(self):
        code, out = validate(skeleton_edit=lambda t: t.replace('{ name = "pelvis", parent = 0', '{ name = "pelvis", parent = 5'))
        self.assertEqual(code, 1)
        self.assertIn("parent", out)
        code, out = validate(skeleton_edit=lambda t: t.replace('name = "neck"', 'name = "head"'))
        self.assertIn("not unique", out)
        code, out = validate(skeleton_edit=lambda t: t.replace("tier_b = true", "tier_b = false", 1))
        self.assertIn("tier-B subset has 27", out)
        code, out = validate(skeleton_edit=lambda t: t.replace("  { name = \"ball_r\"", "  { name = \"ball_r2\", parent = 0, tier_b = false },\n  { name = \"ball_r\"", 1))
        self.assertIn("has 65 joints", out)


if __name__ == "__main__":
    unittest.main()
