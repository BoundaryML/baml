"""Compile and run the public-header smoke consumer as strict C and C++."""

from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys


CRATE = Path(__file__).resolve().parent.parent
SOURCE = CRATE / "tests" / "abi_smoke.c"
INCLUDE = CRATE / "include"
TESTS = CRATE / "tests"


def compile_consumer(*, cpp: bool, output: Path) -> None:
    if os.name == "nt":
        compiler = os.environ.get("CXX" if cpp else "CC", "cl.exe")
        command = [
            compiler,
            "/nologo",
            "/W4",
            "/WX",
            "/std:c++17" if cpp else "/std:c11",
            "/TP" if cpp else "/TC",
            f"/I{INCLUDE}",
            f"/I{TESTS}",
            str(SOURCE),
            f"/Fe:{output}",
        ]
        if cpp:
            command.append("/EHsc")
    else:
        compiler = os.environ.get("CXX" if cpp else "CC", "c++" if cpp else "cc")
        command = [
            compiler,
            "-x",
            "c++" if cpp else "c",
            "-std=c++17" if cpp else "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
            "-I",
            str(INCLUDE),
            "-I",
            str(TESTS),
            str(SOURCE),
            "-o",
            str(output),
        ]
        if sys.platform.startswith("linux"):
            command.append("-ldl")
    subprocess.run(command, check=True)


def main() -> int:
    if len(sys.argv) not in (2, 3):
        raise SystemExit("usage: run_abi_smoke.py <bridge_cffi library> [expected version]")
    library = Path(sys.argv[1]).resolve()
    if not library.is_file():
        raise SystemExit(f"bridge_cffi library does not exist: {library}")

    output_dir = Path(os.environ.get("BAML_CFFI_SMOKE_OUT", CRATE / "target" / "abi-smoke"))
    output_dir.mkdir(parents=True, exist_ok=True)
    suffix = ".exe" if os.name == "nt" else ""
    observed_versions: list[str] = []
    for language, cpp in (("c", False), ("cpp", True)):
        executable = output_dir / f"abi_smoke_{language}{suffix}"
        compile_consumer(cpp=cpp, output=executable)
        completed = subprocess.run(
            [str(executable), str(library)], check=True, text=True, capture_output=True
        )
        print(completed.stdout, end="")
        observed_versions.append(completed.stdout.strip())
    if len(sys.argv) == 3 and any(version != sys.argv[2] for version in observed_versions):
        raise SystemExit(
            f"ABI smoke returned versions {observed_versions!r}; expected {sys.argv[2]!r}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
