#!/usr/bin/env python3
"""Clip manifest checker (SPEC-016 sections 6.3 and 6.5, acceptance T16-11). Stdlib only.

Run: python3 tools/check_clips.py [--full] [root]
Reads data/animation/clips.toml and data/provenance/consents.toml under `root` and reports one line per problem:
  - a clip id that does not match NAME_RE, or whose first token is not its category, or whose category is unknown
  - frames outside 30..1800, fps other than 60, a contact frame outside the clip or not ascending
  - a missing or unsigned consent, a consent that does not belong to the performer or forbids use in the game
  - a performer above 25 percent of a category that holds at least 4 clips
With --full (the finished set, CT-016-8) also: category counts that differ from section 6.1 and a Plan reaction
(flinch, cover, aid, fall, load, fire, march, wheel) that no clip name contains as a token.
Exit 1 on any problem.
"""
import collections
import os
import re
import sys
import tomllib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from check_rights import consent_problem, load  # noqa: E402

NAME_RE = re.compile(r"^[a-z]{3}_[a-z0-9]+(_[a-z0-9]+){2,6}_v[0-9]{2}$")
# Section 6.1 groups I1 to I10: category token and clip count.
COUNTS = {"inf": 45, "mov": 50, "grp": 18, "ufr": 24, "rea": 52, "mel": 20, "car": 12, "off": 28, "art": 30, "cav": 21}
REACTIONS = ("flinch", "cover", "aid", "fall", "load", "fire", "march", "wheel")
MAX_SHARE_PERCENT = 25
MIN_CLIPS_FOR_SHARE = 4


def check(clips, consents, full=False):
    out = []
    index = {c.get("id"): c for c in consents}
    seen = set()
    for c in clips:
        cid = c.get("id", "")
        who = "clip %r" % cid
        if cid in seen:
            out.append("%s: duplicate id" % who)
        seen.add(cid)
        if not NAME_RE.match(cid):
            out.append("%s: name does not match the delivery pattern" % who)
        elif cid[:3] != c.get("category"):
            out.append("%s: first token is not the category %r" % (who, c.get("category")))
        if c.get("category") not in COUNTS:
            out.append("%s: unknown category %r" % (who, c.get("category")))
        frames = c.get("frames", 0)
        if not 30 <= frames <= 1800:
            out.append("%s: %r frames is outside 30..1800" % (who, frames))
        if c.get("fps") != 60:
            out.append("%s: fps %r, the cleaned delivery is 60" % (who, c.get("fps")))
        for side in ("contact_l", "contact_r"):
            marks = c.get(side, [])
            if marks != sorted(set(marks)) or any(not 0 <= m < frames for m in marks):
                out.append("%s: %s must ascend and lie inside the clip" % (who, side))
        problem = consent_problem(c.get("consent"), index)
        if problem:
            out.append("%s: %s" % (who, problem))
        elif index[c["consent"]].get("person_ref") != c.get("performer"):
            out.append("%s: consent %r belongs to another performer" % (who, c["consent"]))
    by_cat = collections.defaultdict(collections.Counter)
    for c in clips:
        by_cat[c.get("category")][c.get("performer")] += 1
    for cat, people in sorted(by_cat.items(), key=str):
        total = sum(people.values())
        for person, n in sorted(people.items(), key=str):
            if total >= MIN_CLIPS_FOR_SHARE and n * 100 > MAX_SHARE_PERCENT * total:
                out.append("category %r: performer %r has %d of %d clips (limit %d percent)" % (cat, person, n, total, MAX_SHARE_PERCENT))
    if full:
        for cat, want in COUNTS.items():
            got = sum(by_cat[cat].values()) if cat in by_cat else 0
            if got != want:
                out.append("category %r: %d clips, section 6.1 lists %d" % (cat, got, want))
        tokens = set(t for c in clips for t in c.get("id", "").split("_"))
        out += ["no clip name carries the reaction %r" % r for r in REACTIONS if r not in tokens]
    return out


def main(argv):
    args = [a for a in argv[1:] if a != "--full"]
    root = os.path.abspath(args[0]) if args else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    try:
        data = load(root)
        problems = check(data["clip"], data["consent"], "--full" in argv)
    except (tomllib.TOMLDecodeError, OSError) as e:
        problems = ["cannot read the data files: %s" % e]
    for p in problems:
        print("check_clips: " + p)
    print("check_clips: %d problems" % len(problems))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
