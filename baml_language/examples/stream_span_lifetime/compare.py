#!/usr/bin/env python3
"""Run identical offline streaming code on installed nightly and local BAML."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
SQL = """SELECT span_name, span_reason, status, input_args['input'] AS input,
    output_value['value'] AS output, error_value
    FROM spans WHERE span_type = 'function' AND span_name LIKE '%Extract%'
    ORDER BY start_time"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--nightly", default="0.22.3-nightly.20261013.b")
    parser.add_argument("--local-cli", type=Path, default=HERE.parents[1] / "target/debug/baml-cli")
    parser.add_argument("--telemetry", choices=["medium", "high"], default="medium")
    args = parser.parse_args()
    local = str(args.local_cli.resolve())
    output = Path(tempfile.mkdtemp(prefix="baml-stream-spans-"))
    env = dict(os.environ, BOUNDARY_API_KEY="local", BAML_TELEMETRY=args.telemetry)
    for name, command in [("nightly", ["baml"]), ("local", [local])]:
        project = output / name
        project.mkdir()
        for file in ["baml.toml", "main.baml"]:
            shutil.copyfile(HERE / file, project / file)
        run_env = dict(env, BAML_VERSION=args.nightly if name == "nightly" else local)
        subprocess.run(command + ["run", "--project", str(project), "main",
            "--agent-skill-check", "off", "--no-progress"], env=run_env, check=True)
        # Read both formats with the updated local reader.
        result = subprocess.run([local, "query", "--local", "--from", str(project),
            "--agent-skill-check", "off", "--format", "json", SQL],
            env=env, check=True, capture_output=True, text=True)
        (output / f"{name}.json").write_text(result.stdout)
        evidence = json.loads(result.stdout)
        if evidence["outcome"]["unsealed"] or evidence["outcome"]["diagnostics"]:
            raise RuntimeError(f"{name}: recording did not seal cleanly: {evidence['outcome']}")
        print(f"\n{name}:")
        print(json.dumps(evidence["rows"], indent=2))
    print(f"\nRecordings and query results: {output}")


if __name__ == "__main__":
    main()
