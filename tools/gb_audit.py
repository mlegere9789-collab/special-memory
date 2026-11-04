#!/usr/bin/env python3
"""Gettysburg data audit tooling (SPEC-013). Standard library plus jsonschema; no clock, no randomness.

This part (ticket T-130) holds the time functions, the source registry loader, the citation rules GB04 and GB05,
the schema metaschema check and the audit-report writer. Corpus loading and the remaining rules come in later tickets.

Usage: python3 tools/gb_audit.py sources [ROOT]    check the schemas and data/history/gettysburg/sources.toml
Exit 1 on any E finding.
"""
import json
import os
import re
import sys
import tomllib
from functools import lru_cache
from typing import NamedTuple

from jsonschema import Draft202012Validator
from referencing import Registry, Resource

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCHEMA_DIR = os.path.join(ROOT, "data", "schemas")
SOURCES_REL = os.path.join("data", "history", "gettysburg", "sources.toml")

GB_SCHEMA_FILES = ["gb-arrival.schema.json", "gb-battery.schema.json", "gb-regiment.schema.json",
                   "gb-terrain-object.schema.json", "gb-timeline-event.schema.json", "gb-weather.schema.json",
                   "common.schema.json"]
REGISTRY_SCHEMA = "gb-regiment.schema.json#/$defs/source_registry"
REPORT_SCHEMA = "gb-regiment.schema.json#/$defs/audit_report"

WINDOW_LO, WINDOW_HI = -4320, 20160   # 1863-06-28T00:00 up to, not including, 1863-07-15T00:00
BATTLE_LO, BATTLE_HI = 0, 4320        # battle days, 1863-07-01 to 1863-07-03
TIME_TOL_MIN = 15                     # placeholder, CT-013-1301

NEEDS_CLEARANCE = ("permission_required", "unknown")
EVIDENCE_FIELDS = ("edition", "access", "licence_text", "consulted_on", "page_scheme")


class Finding(NamedTuple):
    path: str
    rule: str
    severity: str
    message: str


# ---------------------------------------------------------------- time

_STAMP = re.compile(r"1863-([0-9]{2})-([0-9]{2})T([0-9]{2}):([0-9]{2})")
_DAYS = (31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31)


def _leap(y):
    return y % 4 == 0 and (y % 100 != 0 or y % 400 == 0)


def days_from_civil(y, m, d):
    """Days since 1970-01-01 in the proleptic Gregorian calendar."""
    y -= m <= 2
    era = y // 400
    yoe = y - era * 400
    doy = (153 * (m + (-3 if m > 2 else 9)) + 2) // 5 + d - 1
    doe = yoe * 365 + yoe // 4 - yoe // 100 + doy
    return era * 146097 + doe - 719468


def civil_from_days(z):
    z += 719468
    era = z // 146097
    doe = z - era * 146097
    yoe = (doe - doe // 1460 + doe // 36524 - doe // 146096) // 365
    y = yoe + era * 400
    doy = doe - (365 * yoe + yoe // 4 - yoe // 100)
    mp = (5 * doy + 2) // 153
    d = doy - (153 * mp + 2) // 5 + 1
    m = mp + (3 if mp < 10 else -9)
    return y + (m <= 2), m, d


_EPOCH = days_from_civil(1863, 7, 1)


def t_min(stamp):
    """Minutes since 1863-07-01T00:00 for a local-clock stamp. ValueError when malformed or not a real date."""
    m = _STAMP.fullmatch(stamp) if isinstance(stamp, str) else None
    if m is None:
        raise ValueError("malformed stamp %r (want 1863-MM-DDThh:mm)" % (stamp,))
    mo, d, hh, mi = (int(g) for g in m.groups())
    if not 1 <= mo <= 12 or not 1 <= d <= _DAYS[mo - 1] + (mo == 2 and _leap(1863)):
        raise ValueError("impossible date in stamp %r" % stamp)
    if hh > 23 or mi > 59:
        raise ValueError("malformed stamp %r (clock out of range)" % stamp)
    return (days_from_civil(1863, mo, d) - _EPOCH) * 1440 + hh * 60 + mi


def stamp_of(t):
    """Inverse of t_min."""
    days, rem = divmod(t, 1440)
    y, m, d = civil_from_days(days + _EPOCH)
    return "%04d-%02d-%02dT%02d:%02d" % (y, m, d, rem // 60, rem % 60)


def hour_of(t):
    """Hour bucket, floor(t / 60)."""
    return t // 60


def check_stamp(stamp):
    """Problem text for rule GB08 (invalid stamp or outside the campaign window), or None."""
    try:
        t = t_min(stamp)
    except ValueError as e:
        return str(e)
    if not WINDOW_LO <= t < WINDOW_HI:
        return "stamp %s is outside the window [%d, %d)" % (stamp, WINDOW_LO, WINDOW_HI)
    return None


def check_battle_day(stamp):
    """Problem text for a battle-day event (GB08): invalid, outside the window, or outside [0, 4320)."""
    problem = check_stamp(stamp)
    if problem:
        return problem
    if not BATTLE_LO <= t_min(stamp) < BATTLE_HI:
        return "stamp %s is outside the battle days [%d, %d)" % (stamp, BATTLE_LO, BATTLE_HI)
    return None


# ---------------------------------------------------------------- schemas

@lru_cache(maxsize=None)
def _schema_registry(schema_dir):
    reg = Registry()
    for name in sorted(os.listdir(schema_dir)):
        if name.endswith(".schema.json"):
            with open(os.path.join(schema_dir, name), encoding="utf-8") as f:
                reg = reg.with_resource(name, Resource.from_contents(json.load(f)))
    return reg


def _validator(ref, schema_dir=SCHEMA_DIR):
    return Draft202012Validator({"$ref": ref}, registry=_schema_registry(os.path.abspath(schema_dir)))


def _schema_errors(ref, doc, schema_dir=SCHEMA_DIR):
    errs = sorted(_validator(ref, schema_dir).iter_errors(doc), key=lambda e: ("/".join(map(str, e.absolute_path)), e.message))
    return ["%s: %s" % ("/".join(map(str, e.absolute_path)) or "(root)", e.message) for e in errs]


def check_schemas(schema_dir=SCHEMA_DIR):
    """Metaschema check (Draft 2020-12) of the six gb-* schemas and common.schema.json (T13-01, schema half)."""
    out = []
    for name in GB_SCHEMA_FILES:
        p = os.path.join(schema_dir, name)
        try:
            with open(p, encoding="utf-8") as f:
                doc = json.load(f)
        except (OSError, ValueError) as e:
            out.append(Finding(name, "GB01", "E", "schema unreadable: %s" % e))
            continue
        try:
            Draft202012Validator.check_schema(doc)
        except Exception as e:  # SchemaError and its relatives
            out.append(Finding(name, "GB01", "E", "not a valid Draft 2020-12 schema: %s" % str(e).splitlines()[0]))
            continue
        if name == "gb-regiment.schema.json":
            for need in ("source_registry", "audit_report"):
                if need not in doc.get("$defs", {}):
                    out.append(Finding(name, "GB01", "E", "missing $defs/%s" % need))
    return sorted(out)


# ---------------------------------------------------------------- source registry

def find_floats(node, path=""):
    if isinstance(node, float):
        yield path or "(root)"
    elif isinstance(node, dict):
        for k in sorted(node):
            yield from find_floats(node[k], path + "/" + k if path else k)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from find_floats(v, "%s[%d]" % (path, i))


def load_sources(path, schema_dir=SCHEMA_DIR):
    """Read sources.toml. Returns ({code: row}, findings); rows are returned only for a file that parses."""
    label = os.path.relpath(path, ROOT) if os.path.abspath(path).startswith(ROOT + os.sep) else path
    try:
        with open(path, "rb") as f:
            doc = tomllib.load(f)
    except (OSError, tomllib.TOMLDecodeError) as e:
        return {}, [Finding(label, "GB01", "E", "sources unreadable: %s" % e)]
    out = [Finding(label, "GB02", "E", "float at %s" % p) for p in find_floats(doc)]
    out += [Finding(label, "GB01", "E", m) for m in _schema_errors(REGISTRY_SCHEMA, doc, schema_dir)]
    rows = {}
    for i, r in enumerate(doc.get("source", []) if isinstance(doc.get("source"), list) else []):
        if not isinstance(r, dict) or not isinstance(r.get("code"), str):
            continue
        code = r["code"]
        if code in rows:
            out.append(Finding(label, "GB03", "E", "duplicate source code %s" % code))
            continue
        rows[code] = r
        if r.get("consulted") is False:
            for field in EVIDENCE_FIELDS:
                if field in r:
                    out.append(Finding(label, "GB01", "E", "source %s is not consulted but records %s" % (code, field)))
    return rows, sorted(out)


# ---------------------------------------------------------------- citations (GB04, GB05)

def citations_of(node, path=""):
    """Every (path, document code) cited in a record: source refs, claims and second_source."""
    out = []
    if isinstance(node, dict):
        if isinstance(node.get("document"), str) and "page" in node:
            out.append((path, node["document"]))
        for k in sorted(node):
            v = node[k]
            sub = path + "/" + k if path else k
            if k == "second_source" and isinstance(v, str) and ":" in v:
                out.append((sub, v.split(":", 1)[0]))
            else:
                out += citations_of(v, sub)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            out += citations_of(v, "%s/%d" % (path, i))
    return sorted(out)


def check_citations(registry, record, citations):
    """GB04: cited document missing from the registry or not consulted. GB05: permission_required or unknown
    source cited without cleared_by. `citations` is a list of (path, code)."""
    out = []
    for p, code in citations:
        where = "%s/%s" % (record, p) if p else record
        src = registry.get(code)
        if src is None:
            out.append(Finding(where, "GB04", "E", "document %s is not in the source registry" % code))
            continue
        if not src.get("consulted"):
            out.append(Finding(where, "GB04", "E", "document %s is not consulted" % code))
        if src.get("licence_class") in NEEDS_CLEARANCE and not src.get("cleared_by"):
            out.append(Finding(where, "GB05", "E", "document %s is %s and has no cleared_by" % (code, src["licence_class"])))
    return sorted(out)


def check_record_citations(registry, rec):
    return check_citations(registry, str(rec.get("id", "(no id)")), citations_of(rec))


# ---------------------------------------------------------------- audit report

def check_report(report):
    return [Finding("report", "GB01", "E", m) for m in _schema_errors(REPORT_SCHEMA, report)]


_BARE = re.compile(r"[A-Za-z0-9_-]+")
_ESC = {"\\": "\\\\", '"': '\\"', "\b": "\\b", "\t": "\\t", "\n": "\\n", "\f": "\\f", "\r": "\\r"}


def _str(s):
    return '"' + "".join(_ESC.get(c) or ("\\u%04x" % ord(c) if ord(c) < 32 or ord(c) == 127 else c) for c in s) + '"'


def _key(k):
    return k if _BARE.fullmatch(k) else _str(k)


def _inline(v):
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, str):
        return _str(v)
    if isinstance(v, list):
        return "[" + ", ".join(_inline(x) for x in v) + "]"
    if isinstance(v, dict):
        return "{ " + ", ".join("%s = %s" % (_key(k), _inline(v[k])) for k in sorted(v)) + " }" if v else "{}"
    raise TypeError("cannot write %s to TOML (integer-only data)" % type(v).__name__)


def _is_table_array(v):
    if isinstance(v, list) and v and any(isinstance(x, dict) for x in v):
        if not all(isinstance(x, dict) for x in v):
            raise TypeError("mixed list of tables and values")
        return True
    return False


def _emit(lines, path, table, array):
    if path:
        lines.append("")
        lines.append(("[[%s]]" if array else "[%s]") % ".".join(_key(p) for p in path))
    keys = sorted(table, key=lambda k: (k != "schema", k))
    for k in keys:
        v = table[k]
        if not isinstance(v, dict) and not _is_table_array(v):
            lines.append("%s = %s" % (_key(k), _inline(v)))
    for k in keys:
        v = table[k]
        if isinstance(v, dict):
            _emit(lines, path + [k], v, False)
        elif _is_table_array(v):
            for item in v:
                _emit(lines, path + [k], item, True)


def toml_dumps(doc):
    """Deterministic TOML text: scalars first, then tables, keys sorted (`schema` first), LF line ends."""
    lines = []
    _emit(lines, [], doc, False)
    return "\n".join(lines).strip("\n") + "\n"


def _sorted_report(report, date):
    rep = json.loads(json.dumps(report))  # deep copy; also refuses non-JSON values
    rep["date"] = date
    if "findings" in rep:
        rep["findings"] = sorted(rep["findings"], key=lambda f: (f.get("record", ""), f.get("path", ""), f.get("rule", "")))
    for k in ("reconciliation", "headline"):
        if k in rep:
            rep[k] = sorted(rep[k], key=lambda r: (r.get("record", ""), r.get("path", "")))
    return rep


def write_report(path, report, date):
    """Write an audit report (schema $defs/audit_report) with findings sorted by (record, path, rule). `date` is
    supplied by the caller and never read from a clock. ValueError, with nothing written, when the report is invalid."""
    if not isinstance(date, str) or re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}", date) is None:
        raise ValueError("date must be YYYY-MM-DD, got %r" % (date,))
    rep = _sorted_report(report, date)
    floats = list(find_floats(rep))
    if floats:
        raise ValueError("report holds a float at %s" % floats[0])
    problems = check_report(rep)
    if problems:
        raise ValueError("invalid report: " + "; ".join(p.message for p in problems[:5]))
    text = toml_dumps(rep)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)


# ---------------------------------------------------------------- command line

def main(argv):
    if len(argv) >= 2 and argv[1] == "sources":
        root = os.path.abspath(argv[2]) if len(argv) > 2 else ROOT
        findings = check_schemas(os.path.join(root, "data", "schemas"))
        _, more = load_sources(os.path.join(root, SOURCES_REL), os.path.join(root, "data", "schemas"))
        findings = sorted(findings + more)
        for f in findings:
            print("%s: %s %s %s" % (f.path, f.rule, f.severity, f.message))
        return 1 if any(f.severity == "E" for f in findings) else 0
    print(__doc__.strip().splitlines()[-2], file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
