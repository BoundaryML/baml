"""Compare a phase1 fixture's v2 pack with CLI bytecode, normally and stressed.
Usage from worktree root: uv run --no-project python <this-file> lists 6836
"""
import json
import os
from pathlib import Path
import subprocess
import sys

name, expected = sys.argv[1:]
base = Path("baml_language/architecture/runtime-v2/phase1")
rows = []
for stress in ["0", "1"]:
    env = dict(os.environ, BAML_TELEMETRY="off", BAML_GC_STRESS=stress, BAML_CLI_ALLOW_DIRECT="1")
    commands = {
        "v2": [f"target/phase1/{name}-v2"],
        "bytecode": ["target/debug/baml-cli", "--agent-skill-check", "off", "--no-progress", "--color", "never", "run", "--file", str(base / f"{name}.baml"), "main"],
    }
    results = {}
    for backend, command in commands.items():
        result = subprocess.run(command, env=env, capture_output=True, text=True)
        results[backend] = dict(status=result.returncode, stdout=result.stdout,
            stderr=result.stderr.replace("warning: code is unformatted; run `baml fmt`\n", ""))
    assert results["v2"] == results["bytecode"], results
    assert results["v2"] == dict(status=0, stdout=expected + "\n", stderr=""), results
    rows.append(dict(stress=stress, commands=commands, **results))
(base / f"{name}-correctness.json").write_text(json.dumps(rows, indent=2) + "\n")
print(f"{name}: bytecode parity normal/stressed, {expected}")
