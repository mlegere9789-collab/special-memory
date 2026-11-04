"""Example validator plugin. It checks nothing and always passes; copy it to start a real one.

How plugins work (tools/validate_data.py, rule R8):
  - Every tools/validators/*.py whose name does not start with an underscore is loaded, in name order, after the
    built-in checks. Put the plugin for one data family in one file named after it (for example `scenario.py`).
  - The module exposes `validate(root) -> list[str]`. `root` is the repository root being validated (an absolute
    path). Return one short text per problem and an empty list when the data are sound. Each text is printed as
    `<plugin name>: <text>` and counted in the problem total, so write it to name the file and the fault.
  - A plugin must not print, read the clock or the network, or change files. Use only the standard library and the
    packages in tools/requirements.txt.
  - A plugin that fails to import, has no `validate`, raises, or returns anything but a list of strings fails the
    whole run. It is never skipped, so a broken plugin cannot hide the data it was meant to guard.
  - A plugin that has nothing to check under `root` (its data directory is absent) returns an empty list.
"""


def validate(root):
    return []
