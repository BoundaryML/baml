"""Run from the v2-codegen worktree root after building the four executables."""
import json
import os
from pathlib import Path
import statistics
import subprocess

out = Path("target/phase1")
jobs = int(os.environ.get("PHASE1_JOBS", "1"))
commands = {
    "node": ["node", "baml_language/tools/speedtest/src/speedtest/drivers/driver.js", str(out / "trees-checked.js"), str(jobs), "1"],
    "native": [str(out / "trees-native"), "--jobs", str(jobs), "--warmup", "1"],
    "v2": [str(out / "trees-v2"), "--jobs", str(jobs), "--warmup", "1"],
    "spike": [str(out / "spike-target/release/v2-spike"), "binary-trees-14", str(jobs), "1", "3222190"],
}
if before := os.environ.get("PHASE1_TREE_BEFORE"):
    commands["v2-before"] = [before, "--jobs", str(jobs), "--warmup", "1"]
env = dict(os.environ, BAML_TELEMETRY="off", BAML_GC_STRESS="0")
env.pop("BAML_COMPILED_BODIES", None)
rows = []
for round_no in range(5):
    for backend, command in commands.items():
        command = ["taskset", "-c", "12"] + command
        load_before = os.getloadavg()
        process = subprocess.run(command, env=env, capture_output=True, text=True)
        load_after = os.getloadavg()
        assert process.returncode == 0, (command, process.stderr)
        assert process.stderr == "", (command, process.stderr)
        data = json.loads(process.stdout)
        assert str(data["result"]) == "3222190", data
        assert data["jobs"] == jobs, data
        row = dict(round=round_no + 1, load_before=load_before, load_after=load_after, backend=backend, command=command, status=process.returncode, stderr=process.stderr, **data)
        rows.append(row)
        print(json.dumps(row), flush=True)
        (out / "trees-benchmark-raw.json").write_text(json.dumps(rows, indent=2) + "\n")
medians = {backend: statistics.median(r["elapsed_ns"] / r["jobs"] / 1e6 for r in rows if r["backend"] == backend) for backend in commands}
summary = dict(ms_per_job=medians, generated_over_handwritten=medians["v2"] / medians["spike"])
(out / "trees-benchmark-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary), flush=True)
