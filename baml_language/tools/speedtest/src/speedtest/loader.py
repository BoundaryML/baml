"""Workload .md parser — loads ## eval-setup / ## BAML / ## Python / ## Typescript sections."""

import re
import sys
from pathlib import Path
from string import Template


class _DDTemplate(Template):
    """Template using $$var instead of $var, so single $ is literal."""
    delimiter = '$$'


def _workloads_dir():
    """Return the workloads/ directory (sibling of the package)."""
    return Path(__file__).resolve().parent.parent.parent / "workloads"


def parse_workload_md(path):
    """Parse a workload .md file.

    Returns dict with keys: name, category, baml, python, js (expanded source code).
    """
    text = path.read_text()

    title_match = re.match(r'^#\s+(.+)', text)
    if not title_match:
        return None
    full_name = title_match.group(1).strip()

    sections = {}
    for m in re.finditer(
        r'^##\s+([\w-]+)\s*\n```\w*\n(.*?)```',
        text,
        re.MULTILINE | re.DOTALL,
    ):
        sections[m.group(1).lower()] = m.group(2)

    namespace = {}
    setup = sections.get("eval-setup")
    if setup:
        exec(setup, {"__builtins__": __builtins__}, namespace)

    def resolve(code):
        if code is None:
            return None
        if namespace:
            return _DDTemplate(code).safe_substitute(namespace)
        return code

    baml = resolve(sections.get("baml"))
    python = resolve(sections.get("python"))
    js = resolve(sections.get("typescript"))

    if not baml or not python or not js:
        return None

    return {
        "name": full_name,
        "category": path.parent.name,
        "baml": baml,
        "python": python,
        "js": js,
    }


def load_workloads(workloads_dir=None):
    """Scan for .md workload files. Returns ordered list of workload dicts."""
    wdir = Path(workloads_dir) if workloads_dir else _workloads_dir()
    if not wdir.is_dir():
        print(f"ERROR: workloads directory not found: {wdir}", file=sys.stderr)
        sys.exit(1)

    workloads = []
    for md in sorted(wdir.rglob("*.md")):
        w = parse_workload_md(md)
        if w:
            workloads.append(w)
        else:
            print(f"  WARN: skipping {md.relative_to(wdir)} (parse failed)", file=sys.stderr)
    return workloads


def cmd_list():
    """Print all workload names and exit."""
    for w in load_workloads():
        print(w["name"])
