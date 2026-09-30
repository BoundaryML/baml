from __future__ import annotations

from pathlib import Path


def _generated_sdk_file(rel_path: str) -> str | None:
    # Intrinsic-only modules are not emitted at all, so a missing file is fine;
    # callers only need to confirm the symbol is absent when the file exists.
    path = Path.cwd() / "baml_sdk" / rel_path
    if not path.exists():
        return None
    return path.read_text(encoding="utf-8")


def test_stdlib_entrypoints_compiler_intrinsics_are_not_emitted_as_entry_points():
    forbidden = [
        ("vendor/log/__init__.py", '"log.info"'),
        ("vendor/log/__init__.py", '"log.debug"'),
        ("vendor/log/__init__.py", '"log.warn"'),
        ("vendor/log/__init__.py", '"log.error"'),
        ("vendor/log/__init__.pyi", "def info("),
        ("vendor/log/__init__.pyi", "def debug("),
        ("vendor/log/__init__.pyi", "def warn("),
        ("vendor/log/__init__.pyi", "def error("),
        ("baml/events/__init__.py", '"baml.events.send"'),
        ("baml/events/__init__.pyi", "def send("),
    ]

    for rel_path, snippet in forbidden:
        contents = _generated_sdk_file(rel_path)
        assert contents is None or snippet not in contents
