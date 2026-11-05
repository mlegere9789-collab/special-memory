"""Validator plugin for the SPEC-015 data (see example.py for how plugins work).

The checks live in tools/validate_ui.py, which also runs on its own; this file only registers them
with tools/validate_data.py (rule R8).
"""
import importlib.util
import os


def validate(root):
    path = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "validate_ui.py")
    spec = importlib.util.spec_from_file_location("hd_validate_ui", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.validate(root)
