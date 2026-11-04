#!/usr/bin/env python3
"""Rights and consent checker (SPEC-016 sections 6.2 and 9, acceptance T16-12). Stdlib only.

Run: python3 tools/check_rights.py [root]
Reads data/provenance/assets.toml, data/provenance/consents.toml, data/audio/recordings.toml and data/animation/clips.toml
(a missing file counts as no rows) and reports one line per problem:
  - an asset whose licence is not in LICENCES, with no licence_ref, a bad rights_check date, `ai_tool` other than "none"
    without human_authored, or a character, signature art, score or sound identity class without human_authored
  - a consent that allows a digital replica or machine-learning training without a counsel_ref, or has a bad date
  - a voice recording without a consent that exists, is signed and allows game use
  - an id used twice within one file
Exit 1 on any problem.
"""
import datetime
import os
import sys
import tomllib

LICENCES = frozenset([
    "public-domain-us-federal", "cc0-1.0", "cc-by-4.0", "mit", "apache-2.0", "bsd-3-clause", "ofl-1.1",
    "assigned-work-for-hire", "assigned-ip", "commercial-written",
])
HUMAN_CLASSES = frozenset(["character", "signature_art", "score", "sound_identity"])
PATHS = {
    "asset": "data/provenance/assets.toml",
    "consent": "data/provenance/consents.toml",
    "recording": "data/audio/recordings.toml",
    "clip": "data/animation/clips.toml",
}


def load(root):
    """Return {kind: [row, ...]} for the four files under `root`; a missing file gives an empty list."""
    data = {}
    for kind, rel in PATHS.items():
        path = os.path.join(root, rel)
        rows = []
        if os.path.exists(path):
            with open(path, "rb") as f:
                rows = tomllib.load(f).get(kind, [])
        data[kind] = rows
    return data


def valid_ymd(n):
    """True for 0 (not set) or an integer YYYYMMDD that is a real calendar date."""
    if n == 0:
        return True
    try:
        datetime.date(n // 10000, n // 100 % 100, n % 100)
    except (TypeError, ValueError):
        return False
    return isinstance(n, int)


def consent_problem(cid, index):
    """Why consent id `cid` cannot cover a use in the game, or None when it can."""
    row = index.get(cid)
    if row is None:
        return "consent %r does not exist" % (cid,)
    if not row.get("signed"):
        return "consent %r is not signed" % (cid,)
    if not row.get("use_game"):
        return "consent %r does not allow use in the game" % (cid,)
    return None


def check(data):
    out = []
    for kind, rows in data.items():
        ids = [r.get("id") for r in rows]
        out += ["%s: duplicate id %r" % (kind, i) for i in sorted(set(i for i in ids if ids.count(i) > 1), key=str)]
    for a in data["asset"]:
        who = "asset %r" % a.get("id")
        if a.get("licence") not in LICENCES:
            out.append("%s: unknown licence %r" % (who, a.get("licence")))
        if not str(a.get("licence_ref", "")).strip():
            out.append("%s: no licence_ref (evidence of the licence must be stored)" % who)
        if not valid_ymd(a.get("rights_check", 0)):
            out.append("%s: rights_check is not a calendar date" % who)
        if a.get("ai_tool", "none") != "none" and not a.get("human_authored"):
            out.append("%s: ai_tool %r set without human_authored" % (who, a.get("ai_tool")))
        if a.get("class") in HUMAN_CLASSES and not a.get("human_authored"):
            out.append("%s: class %r needs human_authored" % (who, a.get("class")))
    for c in data["consent"]:
        who = "consent %r" % c.get("id")
        for key in ("replica_allowed", "ml_training_allowed"):
            if c.get(key) and not str(c.get("counsel_ref", "")).strip():
                out.append("%s: %s = true without a counsel_ref" % (who, key))
        if not (valid_ymd(c.get("signed", 0)) and valid_ymd(c.get("expires", 0))):
            out.append("%s: signed or expires is not a calendar date" % who)
        elif c.get("expires", 0) and c["expires"] < c.get("signed", 0):
            out.append("%s: expires before it was signed" % who)
    index = {c.get("id"): c for c in data["consent"]}
    for r in data["recording"]:
        who = "recording %r" % r.get("id")
        cid = r.get("consent", "")
        if r.get("category") == "voice" or cid:
            problem = consent_problem(cid, index) if cid else "voice row has no consent"
            if problem:
                out.append("%s: %s" % (who, problem))
    return out


def main(argv):
    root = os.path.abspath(argv[1]) if len(argv) > 1 else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    try:
        problems = check(load(root))
    except (tomllib.TOMLDecodeError, OSError) as e:
        problems = ["cannot read the data files: %s" % e]
    for p in problems:
        print("check_rights: " + p)
    print("check_rights: %d problems" % len(problems))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
