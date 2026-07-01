#!/usr/bin/env python3
"""Unwrap hard-wrapped prose in Markdown files.

Joins consecutive prose lines (and wrapped list-item continuations) into a single
line per paragraph / list item, so paragraphs soft-wrap in the editor/renderer
instead of being hard-broken at ~80 columns. Leaves intact:

  - fenced code blocks (``` / ~~~)        - tables (lines starting with |)
  - headings (#...)                       - blockquotes (>...)
  - horizontal rules (---, ***, ___)      - HTML blocks (<...)
  - blank lines (paragraph boundaries)    - indented code (after a blank line)

Idempotent: files already on one line per paragraph are left unchanged.

Usage:
    python reflow_md.py [PATH ...]          # rewrite in place
    python reflow_md.py --dry-run [PATH ...]  # report what would change
PATH may be a file or a directory (recursed for *.md). Defaults to the
scenarios folder next to this script.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

FENCE = re.compile(r"^\s*(```|~~~)")
HEADING = re.compile(r"^\s{0,3}#{1,6}\s")
TABLE = re.compile(r"^\s*\|")
HR = re.compile(r"^\s{0,3}([-*_])\s*(\1\s*){2,}$")
LIST = re.compile(r"^(\s*)([-*+]|\d+[.)])\s+\S")
BLOCKQUOTE = re.compile(r"^\s{0,3}>")
HTML = re.compile(r"^\s*</?[a-zA-Z!]")
INDENT = re.compile(r"^( {4,}|\t)")
HARD_BREAK = re.compile(r"(  |\\)$")  # trailing two spaces or backslash = intentional break


def reflow(text: str) -> str:
    lines = text.split("\n")
    out: list[str] = []
    para: str | None = None          # the line being accumulated (already joined)
    para_break_after = False         # current para ended on an explicit hard break
    in_code = False

    def flush():
        nonlocal para, para_break_after
        if para is not None:
            out.append(para)
            para = None
            para_break_after = False

    for raw in lines:
        line = raw.rstrip("\n")

        if in_code:
            out.append(line)
            if FENCE.match(line):
                in_code = False
            continue

        if FENCE.match(line):
            flush()
            out.append(line)
            in_code = True
            continue

        # Structural lines: never merged into a prose paragraph; pass through as-is.
        if (
            line.strip() == ""
            or HEADING.match(line)
            or TABLE.match(line)
            or HR.match(line)
            or BLOCKQUOTE.match(line)
            or HTML.match(line)
        ):
            flush()
            out.append(line)
            continue

        # List item start: flush the previous block, begin a new joinable unit
        # seeded with this line so wrapped continuations fold into it.
        if LIST.match(line):
            flush()
            para = line.rstrip()
            para_break_after = bool(HARD_BREAK.search(line))
            continue

        # Indented line with no active paragraph, right after a blank => treat as
        # an indented code block; emit verbatim (don't reflow).
        if para is None and INDENT.match(line):
            out.append(line)
            continue

        # Otherwise: prose (or a wrapped continuation of the current para/list item).
        if para is None:
            para = line.rstrip()
            para_break_after = bool(HARD_BREAK.search(line))
        elif para_break_after:
            # previous line forced a break: keep them on separate lines.
            out.append(para)
            para = line.rstrip()
            para_break_after = bool(HARD_BREAK.search(line))
        else:
            para = para.rstrip() + " " + line.strip()
            para_break_after = bool(HARD_BREAK.search(line))

    flush()
    result = "\n".join(out)
    if text.endswith("\n") and not result.endswith("\n"):
        result += "\n"
    return result


def iter_md(paths: list[Path]):
    for p in paths:
        if p.is_dir():
            yield from sorted(p.rglob("*.md"))
        elif p.suffix == ".md":
            yield p


def main(argv: list[str]) -> int:
    dry = False
    args = []
    for a in argv:
        if a in ("--dry-run", "-n"):
            dry = True
        else:
            args.append(a)

    if args:
        paths = [Path(a) for a in args]
    else:
        paths = [Path(__file__).resolve().parent / "ideas" / "scenarios"]

    changed = 0
    for f in iter_md(paths):
        original = f.read_text()
        new = reflow(original)
        if new != original:
            changed += 1
            if dry:
                before = sum(1 for _ in original.splitlines())
                after = sum(1 for _ in new.splitlines())
                print(f"WOULD REFLOW  {f}  ({before} -> {after} lines)")
            else:
                f.write_text(new)
                print(f"reflowed      {f}")
    verb = "would change" if dry else "changed"
    print(f"\n{changed} file(s) {verb}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
