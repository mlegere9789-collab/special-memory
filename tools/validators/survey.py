"""Validator plugin for the survey source manifests and the obstacle kinds (SPEC-014 sections 3 and 4.3; rule R8 of
validate_data.py).

`validate(root)` returns the failures: a manifest that breaks data/schemas/survey-sources.schema.json, a float, a
duplicate source id, a source that claims a licence, a coverage result or a hash without having been consulted, a
cleared source without its evidence, and an obstacle table whose ids are not unique and ascending.

Outstanding human work is not a failure. `pending(root)` lists it, one text per source and open item, and
`python3 tools/validators/survey.py [root]` prints that list. `check_sources(path, strict=True)` additionally fails
every source that is not cleared with evidence (the rule `hd-terrain check` applies, T14-15)."""
import json
import os
import sys
import tomllib

import jsonschema

HERE = os.path.dirname(os.path.abspath(__file__))
SCHEMA_NAME = "survey-sources.schema.json"
EVIDENCE = ("acquired", "retrieved", "data_sha256", "index_query_sha256", "licence_basis", "licence_evidence_sha256",
            "cleared_by")


def _floats(node, path=""):
    if isinstance(node, float):
        yield path or "(root)"
    elif isinstance(node, dict):
        for k, v in node.items():
            yield from _floats(v, path + "/" + str(k))
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from _floats(v, "%s[%d]" % (path, i))


def _load(path):
    with open(path, "rb") as f:
        return tomllib.load(f)


def _manifests(root):
    base = os.path.join(root, "data", "survey")
    if not os.path.isdir(base):
        return []
    return [os.path.join(base, d, "sources.toml") for d in sorted(os.listdir(base))
            if os.path.isfile(os.path.join(base, d, "sources.toml"))]


def _schema(root):
    for d in (os.path.join(root, "data", "schemas"), os.path.join(os.path.dirname(os.path.dirname(HERE)), "data", "schemas")):
        p = os.path.join(d, SCHEMA_NAME)
        if os.path.exists(p):
            with open(p, encoding="utf-8") as f:
                return json.load(f)
    raise FileNotFoundError(SCHEMA_NAME)


def _label(s):
    return "source %s" % s.get("id", "?")


def check_sources(path, schema, strict=False, rel=None):
    """Returns the failure texts for one sources.toml (each starting with `rel`, default the path)."""
    rel = rel or path
    try:
        doc = _load(path)
    except tomllib.TOMLDecodeError as e:
        return ["%s: not valid TOML: %s" % (rel, e)]
    out = []
    for p in _floats(doc):
        out.append("%s: float at %s (survey data are integers only)" % (rel, p))
    for err in sorted(jsonschema.Draft202012Validator(schema).iter_errors(doc), key=lambda e: list(map(str, e.absolute_path))):
        where = "/".join(map(str, err.absolute_path))
        out.append("%s: schema: %s at /%s" % (rel, err.message[:140], where))
    sources = doc.get("source", []) if isinstance(doc.get("source"), list) else []
    seen = set()
    for s in sources:
        if not isinstance(s, dict):
            continue
        sid = s.get("id")
        if sid in seen:
            out.append("%s: duplicate source id %r" % (rel, sid))
        seen.add(sid)
        if s.get("consulted") is False:
            claimed = [k for k in EVIDENCE + ("vertical_rmse_mm", "georef_rms_mm") if k in s]
            if claimed or s.get("licence_status") not in (None, "unverified") or s.get("coverage_status") not in (None, "unchecked"):
                out.append("%s: %s was not consulted but records %s" % (
                    rel, _label(s), ", ".join(claimed + [k for k in ("licence_status", "coverage_status")
                                                          if s.get(k) not in (None, "unverified", "unchecked")]) or "a result"))
        if s.get("licence_status") == "cleared":
            missing = [k for k in EVIDENCE if k not in s]
            if missing or s.get("consulted") is not True or s.get("coverage_status") != "passed":
                out.append("%s: %s is cleared without %s" % (rel, _label(s), ", ".join(
                    missing + (["consulted = true"] if s.get("consulted") is not True else [])
                    + (["coverage_status = passed"] if s.get("coverage_status") != "passed" else []))))
        if strict and s.get("licence_status") != "cleared":
            out.append("%s: %s licence_status is %r, not cleared" % (rel, _label(s), s.get("licence_status")))
    return out


def pending_sources(path, rel=None):
    """Returns the open items of one sources.toml as plain texts (never failures)."""
    rel = rel or path
    out = []
    for s in _load(path).get("source", []):
        lab = _label(s)
        if s.get("consulted") is not True:
            out.append("%s: %s not consulted" % (rel, lab))
        if s.get("coverage_status", "unchecked") != "passed":
            out.append("%s: %s coverage %s" % (rel, lab, s.get("coverage_status", "unchecked")))
        if s.get("licence_status") != "cleared":
            out.append("%s: %s licence %s" % (rel, lab, s.get("licence_status")))
        absent = [k for k in EVIDENCE + (("vertical_rmse_mm",) if s.get("kind") == "dem" else ()) if k not in s]
        if absent:
            out.append("%s: %s fields not recorded: %s" % (rel, lab, ", ".join(absent)))
    return out


def check_obstacles(path):
    """Ids unique and ascending, names unique (the schema cannot say so)."""
    try:
        rows = _load(path).get("kind", [])
    except tomllib.TOMLDecodeError as e:
        return ["%s: not valid TOML: %s" % (path, e)]
    out, last, names = [], 0, set()
    for r in rows:
        i, n = r.get("id"), r.get("name")
        if not isinstance(i, int) or i <= last:
            out.append("%s: kind id %r is not strictly ascending" % (path, i))
        else:
            last = i
        if n in names:
            out.append("%s: duplicate kind name %r" % (path, n))
        names.add(n)
    return out


def validate(root):
    out = []
    manifests = _manifests(root)
    if manifests:
        schema = _schema(root)
        for p in manifests:
            out.extend(check_sources(p, schema, rel=os.path.relpath(p, root)))
    obstacles = os.path.join(root, "data", "rules", "obstacles.toml")
    if os.path.exists(obstacles):
        out.extend(check_obstacles(os.path.relpath(obstacles, root)))
    return out


def pending(root):
    out = []
    for p in _manifests(root):
        out.extend(pending_sources(p, os.path.relpath(p, root)))
    return out


def main(argv):
    root = os.path.abspath(argv[1]) if len(argv) > 1 else os.path.dirname(os.path.dirname(HERE))
    items = pending(root)
    for t in items:
        print("pending: " + t)
    print("survey: %d pending items (not failures)" % len(items))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
