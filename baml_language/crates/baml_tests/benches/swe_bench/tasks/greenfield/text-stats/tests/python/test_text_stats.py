"""Python grader for the text-stats task.

Discovers the candidate program in the staging dir (text_stats.py at the
root, by convention) and runs it as a subprocess against each fixture,
asserting the JSON it prints matches the expected stats."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

# Staging layout (relative to this test file):
#   <staging>/inputs/...
#   <staging>/tests/python/test_text_stats.py   ← __file__
#   <staging>/text_stats.py                      ← candidate
# Use __file__ rather than Path.cwd(): pytest may change cwd to its
# discovered rootdir during collection, so cwd is unreliable.
ROOT = Path(__file__).resolve().parent.parent.parent
INPUTS = ROOT / "inputs"
CANDIDATE = ROOT / "text_stats.py"


EXPECTED: dict[str, dict[str, int]] = {
    "empty.txt":      {"bytes":  0, "chars":  0, "words": 0, "lines": 0},
    "hello.txt":      {"bytes": 12, "chars": 12, "words": 2, "lines": 1},
    "multi_line.txt": {"bytes": 34, "chars": 34, "words": 6, "lines": 3},
    "unicode.txt":    {"bytes": 31, "chars": 24, "words": 5, "lines": 2},
}


def run_candidate(fixture: Path) -> dict:
    assert CANDIDATE.is_file(), f"missing candidate: {CANDIDATE}"
    proc = subprocess.run(
        [sys.executable, str(CANDIDATE), str(fixture)],
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert proc.returncode == 0, (
        f"candidate exited {proc.returncode}\n"
        f"stdout: {proc.stdout!r}\nstderr: {proc.stderr!r}"
    )
    line = proc.stdout.strip().splitlines()[-1]
    return json.loads(line)


def _assert_fixture(name: str) -> None:
    fixture = INPUTS / name
    assert fixture.is_file(), f"missing fixture: {fixture}"
    got = run_candidate(fixture)
    expected = EXPECTED[name]
    for key in ("bytes", "chars", "words", "lines"):
        assert got.get(key) == expected[key], (
            f"{name}: {key} mismatch — got {got.get(key)!r} expected {expected[key]!r}"
        )


def test_empty():
    _assert_fixture("empty.txt")


def test_hello():
    _assert_fixture("hello.txt")


def test_multi_line():
    _assert_fixture("multi_line.txt")


def test_unicode():
    _assert_fixture("unicode.txt")
