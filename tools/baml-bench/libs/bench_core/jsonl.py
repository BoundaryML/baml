"""Balanced-brace JSON extraction (ported from claude-proxy / dedup).

Agents are asked to write a JSON file, but sometimes emit JSON inline in
the transcript. These helpers recover the first/last top-level {...}.
"""

from __future__ import annotations

import json
from typing import Any, Optional


def _scan(s: str, start: int) -> Optional[tuple[int, int]]:
    """Find the brace-balanced object that opens at a given index.

    Walks forward tracking brace depth while honoring string literals and
    escapes, so braces inside quoted strings do not affect balancing.

    Args:
        s: The text to scan.
        start: Index of the opening "{" to balance from.

    Returns:
        The (start, end) span as a half-open slice covering the balanced
        object, or None if no matching close brace is found.
    """
    depth = 0
    in_str = False
    esc = False
    for i in range(start, len(s)):
        c = s[i]
        if in_str:
            if esc:
                esc = False
            elif c == "\\":
                esc = True
            elif c == '"':
                in_str = False
            continue
        if c == '"':
            in_str = True
        elif c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return (start, i + 1)
    return None


def extract_first_json_object(s: str) -> Optional[Any]:
    """Parse and return the first top-level JSON object found in a string.

    Args:
        s: Text that may contain one or more inline JSON objects.

    Returns:
        The parsed value of the first brace-balanced object that parses as
        JSON, or None if none is found.
    """
    start = s.find("{")
    while start != -1:
        span = _scan(s, start)
        if span:
            try:
                return json.loads(s[span[0] : span[1]])
            except json.JSONDecodeError:
                pass
        start = s.find("{", start + 1)
    return None


def extract_last_json_object(s: str) -> Optional[Any]:
    """Parse and return the last top-level JSON object found in a string.

    Args:
        s: Text that may contain one or more inline JSON objects.

    Returns:
        The parsed value of the final brace-balanced object that parses as
        JSON, or None if none is found.
    """
    found: Optional[Any] = None
    start = s.find("{")
    while start != -1:
        span = _scan(s, start)
        if span:
            try:
                found = json.loads(s[span[0] : span[1]])
            except json.JSONDecodeError:
                pass
            start = s.find("{", span[1])
        else:
            break
    return found
