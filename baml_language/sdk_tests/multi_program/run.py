"""Generate and exercise multiple SDKs in each host process using locally built bridges."""
import argparse
from pathlib import Path
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent
WORKSPACE = ROOT.parents[1]
NODE = WORKSPACE / "sdks/typescript/bridge_typescript"
PYTHON = WORKSPACE / "sdks/python/.venv/bin/python"
CLI = WORKSPACE / "target/debug/baml-cli"


def run(args, cwd=WORKSPACE, capture=False):
    return subprocess.run([str(arg) for arg in args], cwd=cwd, check=True, timeout=120,
                          stdout=subprocess.PIPE if capture else None, text=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--language", choices=("python", "typescript", "all"), default="all")
    args = parser.parse_args()
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
    if args.language in ("python", "all"):
        run([PYTHON, "-m", "pytest", "-q", "-n", "0", ROOT / "test_python.py"])
    if args.language == "python":
        return
    package = ROOT / "node_modules/@boundaryml/baml-bridge"
    package.parent.mkdir(parents=True, exist_ok=True)
    if not package.exists():
        package.symlink_to(NODE, target_is_directory=True)
    run(["pnpm", "exec", "tsc", "-p", ROOT / "tsconfig.json"], NODE)
    result = run(["node", ROOT / ".compiled/test_typescript.js"], capture=True)
    print(result.stdout, end="")
    assert "TypeScript multiple-program regression passed" in result.stdout, "Node exited before completing the regressions"


if __name__ == "__main__":
    main()
