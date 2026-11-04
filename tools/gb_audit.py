#!/usr/bin/env python3
"""Gettysburg data audit tooling (SPEC-013). Standard library plus jsonschema; no clock, no randomness.

Holds the time functions, the source registry loader, the citation rules GB04 and GB05, the schema metaschema check,
the audit-report writer (ticket T-130), the corpus loader and the structural rules GB01 to GB03 and GB07 to GB09
(ticket T-131). The remaining rules come in later tickets.

Usage: python3 tools/gb_audit.py sources [ROOT]    check the schemas and data/history/gettysburg/sources.toml
       python3 tools/gb_audit.py check [ROOT]      load the corpus under ROOT and print every finding
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


# ---------------------------------------------------------------- corpus (T-131)

GB_SCHEMAS = {"units": "gb-regiment", "batteries": "gb-battery", "arrivals": "gb-arrival", "terrain": "gb-terrain-object",
              "events": "gb-timeline-event", "weather": "gb-weather"}
STAMP_KEYS = ("at", "early", "late", "at_late", "from", "to", "exists_from", "exists_to")
BATTLE_KINDS = ("attack_begins", "attack_ends", "bombardment_begins", "bombardment_ends")  # held to the battle days
RANK = {"army": 0, "corps": 1, "division": 2, "brigade": 3, "regiment": 4}
FAR = 10 ** 9  # stands for an open end of an assignment interval


class Record(NamedTuple):
    kind: str    # folder name, a key of GB_SCHEMAS
    label: str   # file path relative to the corpus root, with "/" separators
    stem: str    # file name without ".toml"
    doc: object  # parsed TOML table, or None when the file could not be read


class Corpus(NamedTuple):
    root: str
    schema_dir: str
    sources: dict    # code -> registry row
    records: list    # Record, in sorted path order
    findings: list   # findings raised while loading (unreadable files, registry)


def _label(root, path):
    return os.path.relpath(path, root).replace(os.sep, "/")


def _toml_files(d):
    out = []
    if os.path.isdir(d):
        for name in sorted(os.listdir(d)):
            p = os.path.join(d, name)
            if os.path.isdir(p):
                out += _toml_files(p)
            elif name.endswith(".toml"):
                out.append(p)
    return out


def load_corpus(root, schema_dir=None):
    """Sorted walk of the record folders under `root`. `root` is the repository root (records under
    data/history/gettysburg) or, when that folder is absent, a folder laid out like it. No clock is read."""
    root = os.path.abspath(root)
    base = os.path.join(root, "data", "history", "gettysburg")
    if not os.path.isdir(base):
        base = root
    if schema_dir is None:
        own = os.path.join(root, "data", "schemas")
        schema_dir = own if os.path.isdir(own) else SCHEMA_DIR
    sources, findings = {}, []
    sp = os.path.join(base, "sources.toml")
    if os.path.isfile(sp):
        sources, found = load_sources(sp, schema_dir)
        findings += [f._replace(path=_label(root, sp)) for f in found]
    records = []
    for kind in sorted(GB_SCHEMAS):
        for path in _toml_files(os.path.join(base, kind)):
            label, doc = _label(root, path), None
            try:
                with open(path, "rb") as f:
                    doc = tomllib.load(f)
            except (OSError, ValueError) as e:  # TOMLDecodeError and UnicodeDecodeError are ValueErrors
                findings.append(Finding(label, "GB01", "E", "record unreadable: %s" % e))
            records.append(Record(kind, label, os.path.basename(path)[:-5], doc))
    return Corpus(root, os.path.abspath(schema_dir), sources, records, findings)


def _fact_nodes(node, path=""):
    """Every (path, table) that is an integer fact (value and confidence) or a time fact (at and confidence)."""
    if isinstance(node, dict):
        if "confidence" in node and (isinstance(node.get("value"), int) or isinstance(node.get("at"), str)):
            yield path, node
        for k in sorted(node):
            yield from _fact_nodes(node[k], path + "/" + k if path else k)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from _fact_nodes(v, "%s/%d" % (path, i))


def _stamp_nodes(node, path=""):
    """Every (path, table) holding a stamp key, for rule GB08."""
    if isinstance(node, dict):
        if any(isinstance(node.get(k), str) for k in STAMP_KEYS):
            yield path, node
        for k in sorted(node):
            yield from _stamp_nodes(node[k], path + "/" + k if path else k)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from _stamp_nodes(v, "%s/%d" % (path, i))


def _gb07(doc):
    """low <= value <= high; with claims, low <= min and high >= max of the claim bounds (times: early, at, late)."""
    out = []
    for p, f in _fact_nodes(doc):
        where = p or "(root)"
        if "value" in f:
            lo, hi, v = f.get("low", f["value"]), f.get("high", f["value"]), f["value"]
            if not lo <= v <= hi:
                out.append("%s: value %d is outside low %d, high %d" % (where, v, lo, hi))
            claims = [c for c in f.get("claims", []) if isinstance(c.get("figure"), int)]
            if claims:
                cmin = min(c["figure"] for c in claims)
                cmax = max(c.get("figure_high", c["figure"]) for c in claims)
                if lo > cmin or hi < cmax:
                    out.append("%s: range %d to %d does not span the claims %d to %d" % (where, lo, hi, cmin, cmax))
        else:
            try:
                at = t_min(f["at"])
                lo = t_min(f["early"]) if "early" in f else at
                hi = t_min(f["late"]) if "late" in f else at
                claims = [c for c in f.get("claims", []) if "at" in c]
                cmin = min((t_min(c["at"]) for c in claims), default=None)
                cmax = max((t_min(c.get("at_late", c["at"])) for c in claims), default=None)
            except ValueError:
                continue  # an invalid stamp is GB08
            if not lo <= at <= hi:
                out.append("%s: at %s is outside early %s, late %s" % (where, f["at"], stamp_of(lo), stamp_of(hi)))
            if cmin is not None and (lo > cmin or hi < cmax):
                out.append("%s: early/late do not span the claims %s to %s" % (where, stamp_of(cmin), stamp_of(cmax)))
    return out


def _gb08(rec):
    """Invalid or out-of-window stamps, battle-day events outside the battle days, intervals with from > to."""
    doc, out = rec.doc, []
    for p, node in _stamp_nodes(doc):
        for k in STAMP_KEYS:
            if isinstance(node.get(k), str):
                bad = check_stamp(node[k])
                if bad:
                    out.append("%s: %s" % ("%s/%s" % (p, k) if p else k, bad))
        for a, b in (("from", "to"), ("exists_from", "exists_to")):
            if a in node and b in node and check_stamp(node[a]) is None and check_stamp(node[b]) is None \
                    and t_min(node[a]) > t_min(node[b]):
                out.append("%s: %s %s is after %s %s" % (p or "(root)", a, node[a], b, node[b]))
    if rec.kind == "events" and doc.get("kind") in BATTLE_KINDS and isinstance(doc.get("at"), dict):
        bad = check_battle_day(doc["at"].get("at", ""))
        if bad and check_stamp(doc["at"].get("at", "")) is None:
            out.append("at/at: %s" % bad)
    if rec.kind == "weather" and isinstance(doc.get("date"), str):
        bad = check_stamp(doc["date"] + "T00:00") if re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}", doc["date"]) else "malformed date"
        if bad:
            out.append("date: %s" % bad)
    return out


def _adjacent(level, parent_level):
    if level == "battery":  # attaches to an army, corps, division or brigade-level group
        return RANK.get(parent_level, 9) <= RANK["brigade"]
    return level in RANK and RANK.get(parent_level, -9) == RANK[level] - 1


def _gb09(recs, index):
    """Parent present, adjacent, same side, acyclic; organic assignments do not overlap in time."""
    parent_of, out = {}, []
    for r in recs:
        asg = r.doc.get("assignments", [])
        pick = [a for a in asg if a["kind"] == "organic"] or asg
        if pick:
            parent_of[r.doc["id"]] = pick[0]["parent"]
    for r in recs:
        d, me, level, found = r.doc, r.doc["id"], r.doc.get("level", "battery"), []
        for i, a in enumerate(d.get("assignments", [])):
            where, par = "assignments/%d" % i, index.get(a["parent"])
            if par is None:
                found.append("%s: parent %s is not in the corpus" % (where, a["parent"]))
                continue
            if par[1] != d["side"]:
                found.append("%s: parent %s is on the other side" % (where, a["parent"]))
            if not _adjacent(level, par[0]):
                found.append("%s: parent %s (%s) is not adjacent to a %s in the chain" % (where, a["parent"], par[0], level))
        seen, cur = {me}, parent_of.get(me)
        while cur in parent_of or cur == me:
            if cur == me:
                found.append("assignments: parent chain returns to %s" % me)
                break
            if cur in seen:
                break
            seen.add(cur)
            cur = parent_of[cur]
        spans = []
        for i, a in enumerate(d.get("assignments", [])):
            if a["kind"] == "organic" and check_stamp(a.get("from", "1863-07-01T00:00")) is None \
                    and check_stamp(a.get("to", "1863-07-01T00:00")) is None:
                spans.append((t_min(a["from"]) if "from" in a else -FAR, t_min(a["to"]) if "to" in a else FAR, i))
        for x in range(len(spans)):
            for y in range(x + 1, len(spans)):
                if spans[x][0] < spans[y][1] and spans[y][0] < spans[x][1]:
                    found.append("assignments/%d and assignments/%d: organic assignments overlap" % (spans[x][2], spans[y][2]))
        out += [Finding(r.label, "GB09", "E", m) for m in found]
    return out


def check_corpus(c):
    """Structural rules GB01 to GB03 and GB07 to GB09 over a loaded corpus. Sorted findings; an I or W would sort in too."""
    out, seen, valid, validators = list(c.findings), {}, [], {}
    for r in c.records:
        if r.doc is None:
            continue
        out += [Finding(r.label, "GB02", "E", "float at %s (records are integer-only)" % p) for p in find_floats(r.doc)]
        if r.kind not in validators:
            validators[r.kind] = _validator(GB_SCHEMAS[r.kind] + ".schema.json", c.schema_dir)
        errs = sorted(validators[r.kind].iter_errors(r.doc), key=lambda e: ("/".join(map(str, e.absolute_path)), e.message))
        out += [Finding(r.label, "GB01", "E", "%s: %s" % ("/".join(map(str, e.absolute_path)) or "(root)", e.message[:200]))
                for e in errs]
        rid = r.doc.get("id")
        if rid != r.stem:
            out.append(Finding(r.label, "GB03", "E", "id %r does not match the file name %r" % (rid, r.stem)))
        if isinstance(rid, str):
            if (r.kind, rid) in seen:
                out.append(Finding(r.label, "GB03", "E", "id %s is also used by %s" % (rid, seen[(r.kind, rid)])))
            seen.setdefault((r.kind, rid), r.label)
        if not errs:
            valid.append(r)
    index = {}
    for r in c.records:
        if r.kind in ("units", "batteries") and isinstance(r.doc, dict) and isinstance(r.doc.get("id"), str):
            index.setdefault(r.doc["id"], (r.doc.get("level", "battery"), r.doc.get("side")))
    for r in valid:
        out += [Finding(r.label, "GB07", "E", m) for m in _gb07(r.doc)]
        out += [Finding(r.label, "GB08", "E", m) for m in _gb08(r)]
    out += _gb09([r for r in valid if r.kind in ("units", "batteries")], index)
    return sorted(out)


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
    if len(argv) >= 2 and argv[1] == "check":
        findings = check_corpus(load_corpus(argv[2] if len(argv) > 2 else ROOT))
        for f in findings:
            print("%s: %s %s %s" % (f.path, f.rule, f.severity, f.message))
        return 1 if any(f.severity == "E" for f in findings) else 0
    print("usage: gb_audit.py sources|check [ROOT]", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
