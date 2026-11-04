#!/usr/bin/env python3
"""Gettysburg data audit tooling (SPEC-013). Standard library plus jsonschema; no clock, no randomness.

Holds the time functions, the source registry loader, the citation rules GB04 and GB05, the schema metaschema check,
the audit-report writer (ticket T-130), the corpus loader and the structural rules GB01 to GB03 and GB07 to GB09
(ticket T-131), the evidence ladder with the rules GB04 to GB06 and the known-conflict fixtures with GB17 (ticket
T-132), and the chain sums, casualty, arms and terrain checks GB10 to GB12 and GB18 (ticket T-133). The remaining
rules come in later tickets.

Usage: python3 tools/gb_audit.py sources [ROOT]    check the schemas and data/history/gettysburg/sources.toml
       python3 tools/gb_audit.py check [ROOT]      load the corpus under ROOT and print every finding, fixtures included
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


# ---------------------------------------------------------------- evidence ladder (T-132, section 5)

def lineage_of(registry, code):
    """Lineage of a registered source, or None."""
    row = registry.get(code)
    return row.get("lineage") if row else None


def _consulted(registry, code):
    row = registry.get(code)
    return bool(row and row.get("consulted"))


def _qualifies(registry, claim):
    """A claim counts toward `verified` when its source is consulted and licence-cleared and it names a page."""
    row = registry.get(claim.get("document"))
    return bool(row and row.get("consulted") and isinstance(claim.get("page"), int)
                and (row.get("licence_class") not in NEEDS_CLEARANCE or row.get("cleared_by")))


def claim_span(claim):
    """(low, high) of a claim: its figure and figure_high, or its time and at_late in minutes. None when it has
    neither or the stamp is invalid."""
    try:
        if isinstance(claim.get("figure"), int):
            return claim["figure"], claim.get("figure_high", claim["figure"])
        if isinstance(claim.get("at"), str):
            return t_min(claim["at"]), t_min(claim.get("at_late", claim["at"]))
    except ValueError:
        pass
    return None


def claims_agree(claims):
    """Figures: every claim a single point and all equal. Times: max(at) - min(at) <= TIME_TOL_MIN, at_late counted.
    Claims carrying neither (plain source rows) agree."""
    spans = [claim_span(c) for c in claims]
    if not spans:
        return True
    if any(c.get("figure") is not None for c in claims):
        return all(s is not None and "at" not in c and s[0] == s[1] for s, c in zip(spans, claims)) \
            and len({s[0] for s in spans}) == 1
    if all(s is None for s in spans):
        return not any("at" in c for c in claims)
    return None not in spans and max(b for _, b in spans) - min(a for a, _ in spans) <= TIME_TOL_MIN


def _claims_of(fact):
    return fact["claims"] if isinstance(fact.get("claims"), list) else [c for c in fact.get("sources", []) if isinstance(c, dict)]


def second_source_matches(fact):
    """`second_source` equals the CODE:page of one of the fact's claims."""
    ss = fact.get("second_source")
    return isinstance(ss, str) and any("%s:%s" % (c.get("document"), c.get("page")) == ss for c in _claims_of(fact))


def evidence_problems(registry, fact):
    """Reasons a fact cannot stand at `verified` (rule GB06); empty when it can."""
    claims, out = _claims_of(fact), []
    if len(claims) < 2:
        out.append("needs two claims, has %d" % len(claims))
    good = [c for c in claims if _qualifies(registry, c)]
    for c in claims:
        if c not in good:
            out.append("claim from %s page %s is not from a consulted, licence-cleared source" % (c.get("document"), c.get("page")))
    lineages = {lineage_of(registry, c["document"]) for c in good}
    if len(lineages) < 2:
        out.append("needs consulted claims of two lineages, has %d" % len(lineages))
    if not claims_agree(claims):
        out.append("the claims disagree")
    spans = [claim_span(c) for c in claims]
    if isinstance(fact.get("value"), int) and any(c.get("figure") is not None for c in claims):
        if any(s is None or s[0] != fact["value"] for s in spans):
            out.append("value %d is not the claimed figure" % fact["value"])
    elif isinstance(fact.get("at"), str) and spans and None not in spans:
        try:
            if not min(a for a, _ in spans) <= t_min(fact["at"]) <= max(b for _, b in spans):
                out.append("at %s is outside the claims" % fact["at"])
        except ValueError:
            out.append("at %r is not a stamp" % fact["at"])
    if not second_source_matches(fact):
        out.append("second_source %r does not name one of the claims" % fact.get("second_source"))
    else:
        code, page = fact["second_source"].split(":", 1)
        if not any(c.get("document") == code and str(c.get("page")) == page for c in good):
            out.append("second_source %s is not a consulted, licence-cleared claim" % fact["second_source"])
    return out


def evidence_level(registry, fact):
    """Highest rung the claims support: verified, partial (a consulted claim with a page, or disagreeing claims stored
    as a range) or unverified (no consulted claim, or an unresolved conflict)."""
    if not evidence_problems(registry, fact):
        return "verified"
    claims = _claims_of(fact)
    ranged = any(k in fact for k in ("low", "high", "early", "late"))
    if not claims_agree(claims) and not ranged:
        return "unverified"
    return "partial" if any(_consulted(registry, c.get("document")) and isinstance(c.get("page"), int) for c in claims) else "unverified"


def demoted(registry, fact):
    """A copy of the fact with `verified` lowered to the rung its claims support; the input is not changed."""
    out = json.loads(json.dumps(fact))
    if out.get("confidence") == "verified":
        out["confidence"] = evidence_level(registry, out)
    return out


def _verified_nodes(node, path=""):
    if isinstance(node, dict):
        if node.get("confidence") == "verified" and ("sources" in node or "claims" in node):
            yield path, node
        for k in sorted(node):
            yield from _verified_nodes(node[k], path + "/" + k if path else k)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from _verified_nodes(v, "%s/%d" % (path, i))


def _citation_findings(registry, rec):
    """GB04, GB05 and GB06 for one record, reported on the record's file."""
    out = []
    for f in check_record_citations(registry, rec.doc):
        where = f.path.split("/", 1)[1] if "/" in f.path else "(root)"
        out.append(Finding(rec.label, f.rule, f.severity, "%s: %s" % (where, f.message)))
    for p, n in _verified_nodes(rec.doc):
        out += [Finding(rec.label, "GB06", "E", "%s: verified but %s" % (p or "(root)", m)) for m in evidence_problems(registry, n)]
    return out


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


def _parents_of_record(recs):
    """{unit id: parent id}: the first organic assignment, else the first assignment, of each record."""
    out = {}
    for r in recs:
        asg = r.doc.get("assignments", [])
        pick = [a for a in asg if a["kind"] == "organic"] or asg
        if pick:
            out[r.doc["id"]] = pick[0]["parent"]
    return out


def _gb09(recs, index):
    """Parent present, adjacent, same side, acyclic; organic assignments do not overlap in time."""
    parent_of, out = _parents_of_record(recs), []
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


# ---------------------------------------------------------------- sums, casualties, arms, terrain (T-133)

RULES_REL = os.path.join("data", "rules")
FRAME_MIN_MM, FRAME_MAX_MM = 0, 100_000_000   # gb_local_v1: origin at the south-west corner, 100 km on each axis (SPEC-014)
DAY_SCOPES = ("day1", "day2", "day3")
LOSS_PARTS = ("killed", "wounded", "missing_captured")


def _lohi(fact):
    """Bounds of an integer fact: low or value, high or value."""
    return fact.get("low", fact["value"]), fact.get("high", fact["value"])


def _exact(facts):
    return all(x.get("confidence") == "verified" for x in facts)


def _stored_conflict(fact, got):
    """True when a flagged fact stores both its own figure and the sum it disagrees with as claims, so the conflict
    is reported, not hidden (fixture C03)."""
    figures = {c.get("figure") for c in fact.get("claims", []) if isinstance(c, dict)}
    return isinstance(fact.get("flag"), str) and bool(fact["flag"]) and fact["value"] in figures and got in figures


def chain_sum_problem(parent, children, hq=None, complete=True):
    """None when the children (and the parent's headquarters `hq`) fit the parent, else the problem text.
    Interval test: sum(lo) <= parent high and sum(hi) >= parent low. All facts verified: sum(value) == value.
    `complete=False` says some parts are not listed, so only the first half of the interval test applies."""
    facts = list(children) + ([hq] if hq else [])
    plo, phi = _lohi(parent)
    got = sum(x["value"] for x in facts)
    if complete and _exact(facts + [parent]):
        if got != parent["value"] and not _stored_conflict(parent, got):
            return "parts sum to %d, not %d" % (got, parent["value"])
        return None
    slo, shi = sum(_lohi(x)[0] for x in facts), sum(_lohi(x)[1] for x in facts)
    if _stored_conflict(parent, got):
        return None
    if slo > phi:
        return "parts sum to at least %d, above the high of %d" % (slo, phi)
    if complete and shi < plo:
        return "parts sum to at most %d, below the low of %d" % (shi, plo)
    return None


def rate_problem(loss, base, recorded):
    """None when a recorded percentage is within 1 of pct(loss, base), else the problem text."""
    if base <= 0:
        return "base %d is not positive" % base
    want = pct(loss, base)
    if abs(recorded - want) > 1:
        return "recorded %d percent, but pct(%d, %d) is %d" % (recorded, loss, base, want)
    return None


def _loss_bounds(row):
    """(low, high) of a casualty row's loss: its total, else the sum of its listed components; None when empty."""
    if "total" in row:
        return _lohi(row["total"])
    parts = [row[k] for k in LOSS_PARTS if k in row]
    return (sum(_lohi(x)[0] for x in parts), sum(_lohi(x)[1] for x in parts)) if parts else None


def casualty_problems(rows, engaged):
    """GB11 problem texts for a record's casualty rows: components against the total, day scopes against the battle
    scope, losses above `engaged.high`. `engaged` is an integer fact or None."""
    out = []
    for i, row in enumerate(rows):
        where = "casualties/%s" % row["scope"]
        parts = [row[k] for k in LOSS_PARTS if k in row]
        if "total" in row and parts:
            bad = chain_sum_problem(row["total"], parts, complete=len(parts) == len(LOSS_PARTS))
            if bad:
                out.append("%s: components against total: %s" % (where, bad))
        bounds = _loss_bounds(row)
        if engaged is not None and bounds is not None and bounds[0] > _lohi(engaged)[1]:
            out.append("%s: losses of at least %d exceed engaged high %d" % (where, bounds[0], _lohi(engaged)[1]))
    by_scope = {row["scope"]: row for row in rows}
    battle, days = by_scope.get("battle"), [by_scope[s] for s in DAY_SCOPES if s in by_scope]
    for key in ("total",) + LOSS_PARTS:
        if battle is not None and key in battle:
            have = [d[key] for d in days if key in d]
            if have:
                bad = chain_sum_problem(battle[key], have, complete=len(have) == len(DAY_SCOPES))
                if bad:
                    out.append("casualties/battle/%s: day scopes against battle scope: %s" % (key, bad))
    return out


def _ids(path, table, key):
    """Set of `key` values (as text) of the [[table]] rows of a rules file, or None when it cannot be read."""
    try:
        with open(path, "rb") as fh:
            return {str(r[key]) for r in tomllib.load(fh).get(table, []) if isinstance(r, dict) and key in r}
    except (OSError, ValueError):
        return None


def _rules_file(root, name):
    own = os.path.join(root, RULES_REL, name)
    return own if os.path.isfile(own) else os.path.join(ROOT, RULES_REL, name)


def _gb10(recs, parent_of, by_id):
    """Chain sums of engaged and present (with headquarters) and of battery guns against the parent's arms."""
    kids, out = {}, []
    for r in recs:
        if r.doc["id"] in parent_of:
            kids.setdefault(parent_of[r.doc["id"]], []).append(r.doc)
    for pid in sorted(kids):
        parent = by_id.get(pid)
        if parent is None:
            continue
        # a child whose link fails GB09 (other side, not adjacent) is not summed; GB09 reports it
        kids[pid] = [k for k in kids[pid] if k["side"] == parent["side"] and _adjacent(k.get("level", "battery"), parent.get("level", "battery"))]
        if not kids[pid]:
            continue
        sums, label = [], by_id[pid]["_label"]
        for key in ("engaged", "present"):
            pf = parent.get("strength", {}).get(key)
            if pf is None:
                continue
            missing = [k["id"] for k in kids[pid] if key not in k.get("strength", {})]
            if missing:
                out.append(Finding(label, "GB10", "I", "strength/%s: not summed, %s has no %s" % (key, ", ".join(sorted(missing)), key)))
                continue
            hq = parent["strength"].get("headquarters") if key == "engaged" else None
            sums.append(("strength/" + key, pf, [k["strength"][key] for k in kids[pid]], hq))
        bats = [k for k in kids[pid] if "guns" in k]
        for w in sorted({a["weapon"] for a in parent.get("arms", [])}) if bats else []:
            mine = [a["count"] for a in parent["arms"] if a["weapon"] == w]
            total = {"value": sum(x["value"] for x in mine), "low": sum(_lohi(x)[0] for x in mine),
                     "high": sum(_lohi(x)[1] for x in mine), "confidence": "verified" if _exact(mine) else "unverified"}
            sums.append(("arms/" + w, total, [g["count"] for b in bats for g in b["guns"] if g["weapon"] == w], None))
        for path, pf, cf, hq in sums:
            bad = chain_sum_problem(pf, cf, hq)
            if bad:
                out.append(Finding(label, "GB10", "E", "%s: children of %s: %s" % (path, pid, bad)))
    return out


def _gb11_gb12(r, weapons):
    """GB11 over the casualty rows, GB12 over arms and battery guns."""
    d, out = r.doc, []
    engaged = d.get("strength", {}).get("engaged")
    out += [Finding(r.label, "GB11", "E", m) for m in casualty_problems(d.get("casualties", []), engaged)]
    rows = d.get("arms", []) if r.kind == "units" else d.get("guns", [])
    name = "arms" if r.kind == "units" else "guns"
    for i, a in enumerate(rows):
        if weapons is not None and a["weapon"] not in weapons:
            out.append(Finding(r.label, "GB12", "E", "%s/%d: weapon %s is not in weapons.toml" % (name, i, a["weapon"])))
    if engaged is not None and rows:
        lo = sum(_lohi(a["count"])[0] for a in rows)
        if lo > _lohi(engaged)[1]:
            out.append(Finding(r.label, "GB12", "E", "%s: counts sum to at least %d, above engaged high %d" % (name, lo, _lohi(engaged)[1])))
    return out


def _gb18(r, classes):
    """Terrain class, polygon size and frame bounds; arrival places are held to the frame bounds."""
    d, out = r.doc, []
    if r.kind == "terrain":
        if classes is not None and "terrain_class" in d and d["terrain_class"] not in classes:
            out.append("terrain_class %s is not in terrain.toml" % d["terrain_class"])
        geo = d["geometry"]
        if geo["type"] == "polygon" and len(geo["points"]) < 3:
            out.append("geometry: a polygon has %d points, needs 3" % len(geo["points"]))
        points = geo["points"]
    else:
        pl = d.get("place", {}) if r.kind == "arrivals" else {}
        points = [[pl["x_mm"], pl["y_mm"]]] if "x_mm" in pl and "y_mm" in pl else []
    for i, (x, y) in enumerate(points):
        if not (FRAME_MIN_MM <= x <= FRAME_MAX_MM and FRAME_MIN_MM <= y <= FRAME_MAX_MM):
            out.append("point %d (%d, %d) is outside the frame [%d, %d]" % (i, x, y, FRAME_MIN_MM, FRAME_MAX_MM))
    return [Finding(r.label, "GB18", "E", m) for m in out]


def check_cross(c, valid):
    """Rules GB10 to GB12 and GB18 over the schema-valid records of a corpus."""
    out = []
    weapons = _ids(_rules_file(c.root, "weapons.toml"), "weapon", "id")
    classes = _ids(_rules_file(c.root, "terrain.toml"), "class", "name")
    if weapons is None:
        out.append(Finding(RULES_REL.replace(os.sep, "/") + "/weapons.toml", "GB12", "E", "weapons.toml cannot be read"))
    if classes is None:
        out.append(Finding(RULES_REL.replace(os.sep, "/") + "/terrain.toml", "GB18", "E", "terrain.toml cannot be read"))
    units = [r for r in valid if r.kind in ("units", "batteries")]
    by_id = {}
    for r in units:
        by_id.setdefault(r.doc["id"], dict(r.doc, _label=r.label))
    out += _gb10(units, _parents_of_record(units), by_id)
    for r in units:
        out += _gb11_gb12(r, weapons)
    for r in valid:
        if r.kind in ("terrain", "arrivals"):
            out += _gb18(r, classes)
    return out


def check_corpus(c):
    """Rules GB01 to GB12 and GB18 over a loaded corpus (GB04 to GB06 when a registry is present). Sorted findings."""
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
    out += check_cross(c, valid)
    if c.sources:  # without a registry there is nothing to judge a citation against
        for r in c.records:
            if isinstance(r.doc, dict):
                out += _citation_findings(c.sources, r)
    return sorted(out)


# ---------------------------------------------------------------- known-conflict fixtures (T-132, section 9)

FIXTURE_FILE = "gb-known-conflicts.toml"
CONFLICT_KINDS = ("headline", "range", "time", "sum_conflict", "pair", "control", "roster", "stint", "onset")
ALWAYS_UNVERIFIED = ("headline", "onset")                # never verified, whatever the claims say
UNVERIFIED_WHILE_DISAGREEING = ("range", "time", "sum_conflict")
RULE_CODE = re.compile(r"GB(0[1-9]|1[0-9])")


def pct(loss, base):
    """Integer percentage, floor((loss * 100 + base / 2) / base); a recorded rate matches within 1."""
    return (loss * 100 + base // 2) // base


def _resolve(node, path):
    """Node at a `/` path: a dict key, a list index, or for a list the element whose scope is the segment.
    Returns (found, node)."""
    for seg in path.split("/"):
        if isinstance(node, dict) and seg in node:
            node = node[seg]
        elif isinstance(node, list) and seg.isdigit() and int(seg) < len(node):
            node = node[int(seg)]
        elif isinstance(node, list) and any(isinstance(x, dict) and x.get("scope") == seg for x in node):
            node = next(x for x in node if isinstance(x, dict) and x.get("scope") == seg)
        else:
            return False, None
    return True, node


def _groups(row):
    """Claims of a row grouped by the (record, path) they belong to, in sorted order."""
    out = {}
    for cl in row.get("claims", []):
        out.setdefault((cl.get("record", row["record"]), cl.get("path", row["path"])), []).append(cl)
    return sorted(out.items())


def _bounds(claims):
    spans = [claim_span(c) for c in claims]
    return min(a for a, _ in spans), max(b for _, b in spans)


def _intervals(stints):
    """(from, to, person) of command stints; open ends are FAR."""
    out = []
    for st in stints:
        if isinstance(st, dict) and st.get("kind") == "command" and check_stamp(st.get("from", "1863-07-01T00:00")) is None \
                and check_stamp(st.get("to", "1863-07-01T00:00")) is None:
            out.append((t_min(st["from"]) if "from" in st else -FAR, t_min(st["to"]) if "to" in st else FAR))
    return out


def _row_integrity(row):
    """Problems with the row itself: shape, bounds against its claims, sums, rates and counts."""
    out = []
    for k in ("id", "kind", "record", "path", "rule"):
        if not isinstance(row.get(k), str) or not row[k]:
            out.append("%s is missing" % k)
    if not isinstance(row.get("claims"), list):
        out.append("claims is missing")
    if out:
        return out
    if row["kind"] not in CONFLICT_KINDS:
        out.append("unknown kind %r" % row["kind"])
    if RULE_CODE.fullmatch(row["rule"]) is None:
        out.append("rule %r is not a rule code" % row["rule"])
    for i, cl in enumerate(row["claims"]):
        if not isinstance(cl, dict) or not isinstance(cl.get("document"), str) or not isinstance(cl.get("page"), int) \
                or claim_span(cl) is None or ("figure" in cl) == ("at" in cl):
            out.append("claims/%d is not a figure or time claim with document and page" % i)
    if out:
        return out
    prim = [c for k, c in _groups(row) if k == (row["record"], row["path"])]
    if prim:
        lo, hi = _bounds(prim[0])
        for a, b, fmt in (("low", "high", lambda v: v), ("early", "late", stamp_of)):
            for key, want in ((a, lo), (b, hi)):
                if key in row:
                    try:
                        have = row[key] if a == "low" else t_min(row[key])
                    except ValueError:
                        have = None
                    if have != want:
                        out.append("%s %r is not the bound %s of the row's claims" % (key, row[key], fmt(want)))
    for s in row.get("sums", []):
        if (sum(s["parts"]) == s["total"]) != s["agrees"]:
            out.append("sum of %s against %d: agrees = %s is wrong" % (s["parts"], s["total"], str(s["agrees"]).lower()))
    for r in row.get("rates", []):
        if pct(r["loss"], r["base"]) != r["pct"]:
            out.append("pct(%d, %d) is %d, not %d" % (r["loss"], r["base"], pct(r["loss"], r["base"]), r["pct"]))
    for n in row.get("counts", []):
        if n["stated"] == n["listed"]:
            out.append("count of %s: stated and listed are both %d" % (n["what"], n["stated"]))
    if "ends_by" in row and not (isinstance(row["ends_by"], str) and check_stamp(row["ends_by"]) is None):
        out.append("ends_by %r is not a stamp in the window" % (row["ends_by"],))
    return out


def _span_problem(claims, node):
    """None when the node's range spans the group's claims, else the problem text."""
    lo, hi = _bounds(claims)
    try:
        if "at" in claims[0]:
            have = (t_min(node["early"]), t_min(node["late"])) if "early" in node and "late" in node else None
            text = "early %s, late %s" % (stamp_of(lo), stamp_of(hi))
        else:
            have = (node["low"], node["high"]) if "low" in node and "high" in node else None
            text = "low %d, high %d" % (lo, hi)
    except ValueError:
        return None  # an invalid stamp is GB08
    if have is None or have[0] > lo or have[1] < hi:
        return "range does not span the claims (needs %s)" % text
    return None


def _state(rows_row, rec_of):
    """GB17 problems of one row against the corpus: (record label, message) pairs."""
    row, out = rows_row, []
    kind = row["kind"]

    def say(rec, msg):
        out.append((rec.label, "%s: %s" % (row["id"], msg)))

    if kind in ALWAYS_UNVERIFIED + UNVERIFIED_WHILE_DISAGREEING:
        for (rid, path), claims in _groups(row):
            rec = rec_of.get(rid)
            found, node = _resolve(rec.doc, path) if rec else (False, None)
            if not found:
                continue
            disagree = not claims_agree(claims)
            for n in node if isinstance(node, list) else [node]:
                if not isinstance(n, dict):
                    continue
                if n.get("confidence") == "verified" and (kind in ALWAYS_UNVERIFIED or disagree):
                    say(rec, "%s is verified while the volume's claims conflict" % path)
                if kind in ("range", "time") and disagree and (rid, path) == (row["record"], row["path"]):
                    problem = _span_problem(claims, n)
                    if problem:
                        say(rec, "%s: %s" % (path, problem))
                if kind == "sum_conflict" and (rid, path) == (row["record"], row["path"]):
                    if not (isinstance(n.get("flag"), str) and n["flag"]):
                        say(rec, "%s: the components and the headline disagree and the fact carries no flag" % path)
                    have = {c.get("figure") for c in n.get("claims", []) if isinstance(c, dict)}
                    for s in row.get("sums", []):
                        for want in (s["total"], sum(s["parts"])):
                            if want not in have:
                                say(rec, "%s: figure %d is not stored as a claim" % (path, want))
    rec = rec_of.get(row["record"])
    if kind == "range" and row.get("counts") and rec is not None and _resolve(rec.doc, row["path"])[0]:
        node = _resolve(rec.doc, row["path"])[1]
        if not (isinstance(node, dict) and isinstance(node.get("flag"), str) and node["flag"]):
            say(rec, "%s: the stated and listed counts differ and the fact carries no flag" % row["path"])
    if kind == "time" and row.get("not_before") and rec is not None:
        if row["not_before"] not in [n.get("event") for n in rec.doc.get("not_before", []) if isinstance(n, dict)]:
            say(rec, "not_before does not name %s" % row["not_before"])
    if kind == "pair" and rec is not None:
        have = [p for p in row["keep"] if _resolve(rec.doc, p)[0]]
        for p in row["keep"]:
            if have and p not in have:
                say(rec, "%s is dropped while %s is kept" % (p, have[0]))
    if kind == "stint" and rec is not None:
        for st in rec.doc.get("commanders", []):
            if isinstance(st, dict) and st.get("person", {}).get("id") == row["person"]:
                if "to" not in st or (check_stamp(st["to"]) is None and t_min(st["to"]) > t_min(row["ends_by"])):
                    say(rec, "%s's stint does not end by %s" % (row["person"], row["ends_by"]))
    if kind == "roster":
        for m in row.get("members", []):
            mrec = rec_of.get(m["record"])
            if mrec is None:
                continue
            if mrec.doc.get("level") != "brigade":
                say(mrec, "%s (%s) is not a brigade record" % (m["record"], m["label"]))
            spans = sorted(_intervals(mrec.doc.get("commanders", [])))
            if any(spans[i][1] > spans[i + 1][0] for i in range(len(spans) - 1)):
                say(mrec, "%s has two simultaneous command stints" % m["record"])
    return out


def check_fixtures(c, fixture_dir):
    """Rule GB17: the rows of `gb-known-conflicts.toml` in `fixture_dir` against the corpus. A row binds once its
    record exists. Without the file nothing is checked. Sorted findings."""
    path = os.path.join(fixture_dir, FIXTURE_FILE)
    if not os.path.isfile(path):
        return []
    label = _label(c.root, path) if os.path.abspath(path).startswith(c.root + os.sep) else FIXTURE_FILE
    try:
        with open(path, "rb") as f:
            doc = tomllib.load(f)
    except (OSError, ValueError) as e:
        return [Finding(label, "GB17", "E", "fixture file unreadable: %s" % e)]
    out = [Finding(label, "GB17", "E", "float at %s" % p) for p in find_floats(doc)]
    rows = doc.get("conflict") if isinstance(doc.get("conflict"), list) else []
    if not rows:
        out.append(Finding(label, "GB17", "E", "no [[conflict]] rows"))
    rec_of = {}
    for r in c.records:
        if isinstance(r.doc, dict) and isinstance(r.doc.get("id"), str):
            rec_of.setdefault(r.doc["id"], r)
    seen = set()
    for i, row in enumerate(rows):
        name = row.get("id", "row %d" % i) if isinstance(row, dict) else "row %d" % i
        problems = _row_integrity(row) if isinstance(row, dict) else ["row is not a table"]
        if name in seen:
            problems.append("id is used twice")
        seen.add(name)
        out += [Finding(label, "GB17", "E", "%s: %s" % (name, m)) for m in problems]
        if not problems:
            out += [Finding(p, "GB17", "E", m) for p, m in _state(row, rec_of)]
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
        corpus = load_corpus(argv[2] if len(argv) > 2 else ROOT)
        findings = sorted(check_corpus(corpus) + check_fixtures(corpus, os.path.join(corpus.root, "qc", "fixtures")))
        for f in findings:
            print("%s: %s %s %s" % (f.path, f.rule, f.severity, f.message))
        return 1 if any(f.severity == "E" for f in findings) else 0
    print("usage: gb_audit.py sources|check [ROOT]", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
