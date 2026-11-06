"""Run from worktree root after building strings-v2 and the matching CLI."""
import json
import os
from pathlib import Path
import subprocess

source = "baml_language/architecture/runtime-v2/phase1/strings.baml"
evidence = []
for stress in ["0", "1"]:
    env = dict(os.environ, BAML_TELEMETRY="off", BAML_GC_STRESS=stress, BAML_CLI_ALLOW_DIRECT="1")
    commands = {
        "v2": ["target/phase1/strings-v2"],
        "bytecode": ["target/debug/baml-cli", "--agent-skill-check", "off", "--no-progress", "--color", "never", "run", "--file", source, "main"],
    }
    results = {}
    for backend, command in commands.items():
        process = subprocess.run(command, env=env, capture_output=True, text=True)
        results[backend] = dict(command=command, status=process.returncode, stdout=process.stdout,
            stderr=process.stderr.replace("warning: code is unformatted; run `baml fmt`\n", ""))
    for key in ["status", "stdout", "stderr"]:
        assert results["v2"][key] == results["bytecode"][key], (stress, key, results)
    assert results["v2"]["status"] == 0 and results["v2"]["stdout"] == "340\n", results
    evidence.append(dict(gc_stress=stress, results=results))
Path("baml_language/architecture/runtime-v2/phase1/strings-correctness.json").write_text(json.dumps(evidence, indent=2) + "\n")
print("String fixture matches bytecode normally and under v2 GC stress: 340")
