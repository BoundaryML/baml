#!/usr/bin/env python3
"""Pack this wrapper and query isolated, local-only recordings of its version tags."""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

from environment import test_environment


REPO = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--cli", type=Path, required=True)
parser.add_argument("--pack-host", type=Path, required=True)
parser.add_argument(
    "--evidence-dir", type=Path, help="retain private logs and recordings here"
)
args = parser.parse_args()
if os.name == "nt":
    parser.error("the delegated CLI fixtures require a POSIX host")
root = (args.evidence_dir or Path(tempfile.mkdtemp(prefix="wrapper-version-tags-"))).resolve()
root.mkdir(mode=0o700, parents=True, exist_ok=True)
cli = args.cli.resolve()
host = args.pack_host.resolve()
base = test_environment(os.environ)
for key in ("BAML_CONTEXT", "BAML_TOOLCHAIN", "BAML_HOME"):
    base.pop(key, None)
base.update(
    BAML_CLI_ALLOW_DIRECT="1",
    DEV_BAML_CLI_DISABLE_AGENT_DETECTION="1",
    BAML_HOME=str(root / "build-home"),
    BOUNDARY_API_URL="http://127.0.0.1:9",
)


def run(name, command, env=base, cwd=root, expected=0):
    result = subprocess.run(
        [str(arg) for arg in command],
        env=env,
        cwd=cwd,
        capture_output=True,
        text=True,
        timeout=90,
    )
    (root / f"{name}.stdout").write_text(result.stdout)
    (root / f"{name}.stderr").write_text(result.stderr)
    assert result.returncode == expected, (name, result.returncode, result.stderr)
    return result


compiler = (
    run("compiler-version", [cli, "--version"]).stdout.strip().removeprefix("baml-cli ")
)
default_version = re.search(
    r'^version = "([^"]+)"',
    (REPO / "baml_language/crates/baml/Cargo.toml").read_text(),
    re.M,
)[1]
override = "9.8.7-version-test"
wrapper = root / "wrapper"
build = [
    sys.executable,
    REPO / "baml_language/wrapper/build",
    "--cli",
    cli,
    "--pack-host",
    host,
]
run("pack-override", [*build, "--version", override, "--output", wrapper])
default_wrapper = root / "wrapper-default"
run("pack-default", [*build, "--output", default_wrapper])

delegates = []
for version in ("0.1.0", "99.0.0"):
    delegate = root / f"cli-{version}"
    delegate.write_text(
        f"#!{sys.executable}\n"
        "import json, pathlib, sys\n"
        f"with pathlib.Path({str(root / 'delegate-calls.jsonl')!r}).open('a') as log:\n"
        f"    log.write(json.dumps([{version!r}, sys.argv[1:]]) + '\\n')\n"
        f"print('baml-cli {version}')\n"
    )
    delegate.chmod(0o755)
    delegates.append(delegate)

context = {
    "distinct_id": "version-test-caller",
    "metadata": {
        "caller": "preserved",
        "command": "caller-command",
        "baml.ctx.is_ci": True,
        "baml.ctx.app_version": "launch-app",
        "baml.ctx.baml_version": "launch-baml",
    },
}
scenarios = [
    ("help", wrapper, ["--help"], override, None, 0),
    ("error", wrapper, ["--project"], override, context, 1),
    ("older-cli", wrapper, ["--version"], override, context, 0),
    ("newer-cli", wrapper, ["--version"], override, context, 0),
    ("helper", wrapper, ["__flush-wrapper-telemetry"], override, None, 0),
    ("default", default_wrapper, ["--help"], default_version, None, 0),
]
summary = []
for name, binary, argv, app_version, caller, exit_code in scenarios:
    home = root / f"home-{name}"
    env = dict(base, BAML_HOME=str(home), BAML_TELEMETRY="medium")
    if caller:
        env["BAML_CONTEXT"] = json.dumps(caller)
    if name in ("older-cli", "newer-cli"):
        env["BAML_TOOLCHAIN"] = str(delegates[0 if name == "older-cli" else 1])
    output = run(name, [binary, *argv], env=env, expected=exit_code)
    if name in ("older-cli", "newer-cli"):
        selected = "0.1.0" if name == "older-cli" else "99.0.0"
        assert f"baml wrapper {override}" in output.stdout
        assert f"baml-cli {selected}" in output.stdout
    if name == "error":
        assert "error:" in output.stderr
    assert not (home / "wrapper-telemetry").exists(), (
        "PostHog collected despite opt-out"
    )
    project = root / f"query-{name}"
    project.mkdir()
    (project / "baml.toml").write_text('[package]\nname = "version-query"\n')
    shutil.copytree(home / "btel", project / ".baml/btel")

    def query(label, sql):
        (root / f"{name}-{label}.sql").write_text(sql + "\n")
        result = run(
            f"{name}-{label}",
            [cli, "query", "--local", "--format", "json", sql],
            cwd=project,
        )
        data = json.loads(result.stdout)
        assert data["outcome"]["status"] == "complete", data
        assert data["outcome"]["unsealed"] is False, data
        return data["rows"]

    spans = query(
        "spans",
        """SELECT span_name, status,
        context_metadata['baml.ctx.app_version'], context_metadata['baml.ctx.baml_version'],
        context_distinct_id, context_metadata['caller'], context_metadata['command'],
        context_metadata['baml.ctx.is_ci'] FROM spans ORDER BY span_name""",
    )
    operational = [
        row
        for row in spans
        if row[0]
        in (
            "user.wrapper_entry",
            "user.dispatch",
            "user.WrapperInvocation.parse",
        )
    ]
    assert {row[0] for row in operational} >= {"user.wrapper_entry", "user.dispatch"}, (
        spans
    )
    for row in operational:
        assert row[2:4] == [app_version, compiler], row
        if caller:
            assert row[4:6] == ["version-test-caller", "preserved"], row
            assert row[7] is True, row  # Local native is_ci semantics are unchanged.
    for row in spans:
        if row[0].startswith("user.") and row[0] != "user.main":
            assert row[2:4] == [app_version, compiler], row
    capture = query(
        "capture",
        """SELECT span_name, baml_value_state(input_args),
        baml_value_state(output_value), baml_value_state(error_value) FROM spans
        WHERE span_name = 'user.wrapper_entry'""",
    )
    assert capture == [["user.wrapper_entry", "no_value", "no_value", "no_value"]], (
        capture
    )
    # Metadata-only patch preserves the caller's command label on entry. Dispatch
    # may attach its own canonical command context independently (PR #5621).
    if caller:
        assert (
            next(row for row in spans if row[0] == "user.wrapper_entry")[6]
            == "caller-command"
        )
    main = next(row for row in spans if row[0] == "user.main")
    expected_launch = ["launch-app", "launch-baml"] if caller else [None, None]
    assert main[2:4] == expected_launch, main
    processes = query(
        "processes",
        """SELECT baml_version,
        context_metadata['baml.ctx.app_version'], context_metadata['baml.ctx.baml_version'],
        context_distinct_id FROM processes""",
    )
    assert len(processes) == 1, processes
    assert processes[0][:3] == [compiler, *expected_launch], processes
    if name == "error":
        assert any(
            row[0] == "user.dispatch" and row[1] == "user_error" for row in spans
        ), spans
        assert any(
            row[0] == "user.WrapperInvocation.parse" and row[1] == "user_error"
            for row in spans
        ), spans
    summary.append(
        {
            "scenario": name,
            "app_version": app_version,
            "baml_version": compiler,
            "spans": operational,
        }
    )

calls = [
    json.loads(line)
    for line in (root / "delegate-calls.jsonl").read_text().splitlines()
]
assert calls == [["0.1.0", ["--version"]], ["99.0.0", ["--version"]]], calls
off_home = root / "home-off"
run("recording-off", [wrapper, "--help"], env=dict(base, BAML_HOME=str(off_home)))
assert not list(off_home.rglob("*.btel")), "recording-off created recordings"
(root / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(
    f"PASS: version generation, root operational scope, inheritance, errors, caller context, independent CLI versions, PostHog off, recording off; evidence: {root}"
)
