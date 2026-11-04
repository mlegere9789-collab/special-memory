"""Validator plugin for the Gettysburg records under data/history/gettysburg (SPEC-013 section 10, rule R8 of
validate_data.py). It loads the corpus with tools/gb_audit.py and reports every E finding as
`<path>: GBnn <message>` (the corpus rules and, from qc/fixtures, the known-conflict rule GB17); W and I findings are printed by `python3 tools/gb_audit.py check` only. A repository
without data/history/gettysburg has nothing to check."""
import os
import sys

TOOLS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if TOOLS not in sys.path:
    sys.path.insert(0, TOOLS)

import gb_audit  # noqa: E402

GB_SCHEMAS = gb_audit.GB_SCHEMAS


def check_gettysburg(root):
    """Returns (files checked, problems) where problems are the texts of the E findings, in sorted order."""
    if not os.path.isdir(os.path.join(root, "data", "history", "gettysburg")):
        return 0, []
    corpus = gb_audit.load_corpus(root)
    found = gb_audit.check_corpus(corpus)
    fixtures = os.path.join(root, "qc", "fixtures")  # rule R6: the known-conflict fixtures (SPEC-013 section 9)
    found = sorted(found + gb_audit.check_fixtures(corpus, fixtures))
    problems = ["%s: %s %s" % (f.path, f.rule, f.message) for f in found if f.severity == "E"]
    return len(corpus.records), problems


def validate(root):
    return check_gettysburg(root)[1]
