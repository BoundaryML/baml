from __future__ import annotations

from pathlib import Path

from baml_sdk.baml.fs import exists
from baml_sdk.baml.sys import now_ms


def _generated_sdk_file(rel_path: str) -> str | None:
    # Intrinsic-only modules are not emitted at all, so a missing file is fine;
    # callers only need to confirm the symbol is absent when the file exists.
    path = Path.cwd() / "baml_sdk" / rel_path
    if not path.exists():
        return None
    return path.read_text(encoding="utf-8")


# `baml.sys.now_ms() -> int` is a `$rust_function` → `FunctionKind::Native`.
# Calling it as an entry point should run the native and return a positive
# millisecond timestamp, not reject with `NotInvokableAsEntry`.
def test_native_now_ms_callable_as_entry_point():
    assert now_ms() > 0


# `baml.fs.exists(path: string) -> bool` is a `$rust_io_function` →
# `FunctionKind::SysOp`. Calling it as an entry point should run the
# filesystem sysop and return a bool. `.` exists in the generated fixture
# directory on the test host.
def test_sysop_fs_exists_callable_as_entry_point():
    assert exists(".") is True


def test_compiler_intrinsics_are_not_emitted_as_entry_points():
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
