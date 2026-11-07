"""E3: lines of instrumentation code each stack needs for the same four apps.

Counts the non-blank lines between the `# setup:<stack>` and `# end:<stack>`
markers in apps/py_apps.py, plus one decorator per traced Python function.
BAML needs no setup code: telemetry is on by default and BOUNDARY_API_KEY
selects the destination. Its one explicit capture is the `$trace` argument
on the Stock tool call.
"""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
src = (ROOT / "apps" / "py_apps.py").read_text()
baml = (ROOT / "apps" / "baml" / "baml_src" / "apps.baml").read_text()
rows = {}
for stack in ("otel", "logfire", "datadog"):
    block = re.search(rf"# setup:{stack}\n(.*?)# end:{stack}", src, re.S).group(1)
    lines = [l for l in block.splitlines() if l.strip() and not l.strip().startswith("#")]
    decorated = len(re.findall(r"^@trace_(app|tool)", src, re.M))
    rows[stack] = {"setup_lines": len(lines), "decorators": decorated, "total": len(lines) + decorated,
                   "snippet": "\n".join(l[4:] if l.startswith("    ") else l for l in block.splitlines())}
rows["baml"] = {"setup_lines": 0, "decorators": 0, "explicit_captures": baml.count("$trace"),
                "total": baml.count("$trace"), "snippet": "export BOUNDARY_API_KEY=bdry_secret_..."}
(ROOT / "results" / "loc.json").write_text(json.dumps(rows, indent=1) + "\n")
for k, v in rows.items():
    print(k, {x: y for x, y in v.items() if x != "snippet"})
