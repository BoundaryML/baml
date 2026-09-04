"""Generate and exercise multiple SDKs in each host process using locally built bridges."""
from pathlib import Path
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent
WORKSPACE = ROOT.parents[1]
NODE = WORKSPACE / "sdks/typescript/bridge_typescript"
PYTHON = WORKSPACE / "sdks/python/.venv/bin/python"
CLI = WORKSPACE / "target/debug/baml-cli"


def run(args, cwd=WORKSPACE):
    subprocess.run([str(arg) for arg in args], cwd=cwd, check=True, timeout=120)


def main():
    copy = ROOT / "a_copy"
    copy.mkdir(exist_ok=True)
    shutil.copyfile(ROOT / "a/baml.toml", copy / "baml.toml")
    shutil.copytree(ROOT / "a/baml_src", copy / "baml_src", dirs_exist_ok=True)
    for program in ("a", "b", "a_copy"):
        run([CLI, "generate", "--project", ROOT / program, "--agent-skill-check", "off"])
    keys = []
    for program in ("a", "a_copy"):
        source = (ROOT / program / "typescript/baml_sdk/_inlinedbaml.ts").read_text()
        keys.append(re.search(r"PROGRAM_KEY\s*=\s*(\d+)n", source).group(1))
    assert keys[0] == keys[1], "relocating identical sources changed the generated program identity"
    package = ROOT / "node_modules/@boundaryml/baml-bridge"
    package.parent.mkdir(parents=True, exist_ok=True)
    if not package.exists():
        package.symlink_to(NODE, target_is_directory=True)
    run([PYTHON, "-m", "pytest", "-q", "-n", "0", ROOT / "test_python.py"])
    run(["pnpm", "exec", "tsc", "-p", ROOT / "tsconfig.json"], NODE)
    run(["node", ROOT / ".compiled/test_typescript.js"])


if __name__ == "__main__":
    main()
