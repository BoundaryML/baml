"""Generate .pyi stubs from PyO3 code.

Usage (from sdks/python/):
    uv run python generate_stubs.py
"""

import json
import os
import subprocess
import sys
import sysconfig
from pathlib import Path

# This script lives in sdks/python/.
PYTHON_DIR = Path(__file__).resolve().parent
SRC_DIR = PYTHON_DIR / "src"
WORKSPACE_ROOT = PYTHON_DIR.parent.parent


def main() -> int:
    # Build the stub_gen binary.
    result = subprocess.run(
        ["cargo", "build", "-p", "bridge_python", "--bin", "stub_gen"],
        cwd=WORKSPACE_ROOT,
    )
    if result.returncode != 0:
        return result.returncode

    # Find the built binary.
    meta = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        capture_output=True, text=True, cwd=WORKSPACE_ROOT,
    )
    target_dir = json.loads(meta.stdout)["target_directory"]
    binary = Path(target_dir) / "debug" / "stub_gen"

    # Python info — use base_prefix (actual install, not the venv).
    python_home = sys.base_prefix
    python_libdir = sysconfig.get_config_var("LIBDIR")

    # On macOS, fix broken install_name in the binary if needed.
    if sys.platform == "darwin":
        otool = subprocess.run(
            ["otool", "-L", str(binary)], capture_output=True, text=True
        )
        for line in otool.stdout.splitlines():
            line = line.strip()
            if "libpython" in line:
                dylib_ref = line.split()[0]
                if not os.path.exists(dylib_ref):
                    real_lib = os.path.join(python_libdir, os.path.basename(dylib_ref))
                    if os.path.exists(real_lib):
                        subprocess.run(
                            ["install_name_tool", "-change", dylib_ref, real_lib, str(binary)]
                        )

    # pyo3-stub-gen reads pyproject.toml from CARGO_MANIFEST_DIR at both
    # compile time (for module name) and runtime (for output path).
    # A permanent symlink at rust/bridge_python/pyproject.toml → ../../pyproject.toml.
    crate_dir = PYTHON_DIR / "rust" / "bridge_python"
    env = os.environ.copy()
    env["CARGO_MANIFEST_DIR"] = str(crate_dir)
    env["PYTHONHOME"] = python_home
    env["DYLD_LIBRARY_PATH"] = python_libdir

    result = subprocess.run([str(binary)], env=env)
    if result.returncode != 0:
        print("stub_gen failed", file=sys.stderr)
        return result.returncode

    # pyo3-stub-gen writes relative to CARGO_MANIFEST_DIR + python-source.
    # Since the symlinked pyproject.toml is at rust/bridge_python/, and
    # python-source = "src", stubs land at rust/bridge_python/src/baml_core/...
    # Move them to the real src/ location.
    crate_src = PYTHON_DIR / "rust" / "bridge_python" / "src" / "baml_core"
    real_src = SRC_DIR / "baml_core"

    # pyo3-stub-gen 0.22 generates baml_py/__init__.pyi (package layout),
    # but baml_py is a single extension module (.so), not a package.
    # Flatten to baml_py.pyi.
    stub_dir = crate_src / "baml_py"
    stub_flat = real_src / "baml_py.pyi"

    if not stub_dir.exists():
        # Maybe it wrote directly to real src/ (if paths aligned)
        stub_dir = real_src / "baml_py"
    if stub_dir.is_dir() and (stub_dir / "__init__.pyi").exists():
        import shutil
        shutil.move(str(stub_dir / "__init__.pyi"), str(stub_flat))
        shutil.rmtree(str(stub_dir))

    # pyo3-stub-gen sorts classes alphabetically, which puts
    # BamlCancelledError before BamlError (its base). Reorder so
    # base classes are defined before subclasses.
    _fix_exception_ordering(stub_flat)

    print(f"Generated {stub_flat}")
    return 0


def _fix_exception_ordering(pyi_path: Path) -> None:
    """Move BamlError(Exception) before its subclasses in the .pyi file."""
    import re

    content = pyi_path.read_text()

    base_pattern = re.compile(
        r"^class BamlError\(Exception\):.*?(?=\nclass |\n[^\s]|\Z)",
        re.MULTILINE | re.DOTALL,
    )
    base_match = base_pattern.search(content)
    if not base_match:
        return

    base_block = base_match.group(0)
    content = content[: base_match.start()] + content[base_match.end() :]

    sub_match = re.search(r"^class Baml\w+Error\(BamlError\):", content, re.MULTILINE)
    if sub_match:
        content = content[: sub_match.start()] + base_block + "\n" + content[sub_match.start() :]

    pyi_path.write_text(content)


if __name__ == "__main__":
    raise SystemExit(main())
