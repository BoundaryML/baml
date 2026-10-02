"""Environment variables are honored whenever the host program sets them.

A program commonly does `import baml_sdk` at module top level and only later,
inside `main()`, calls `load_dotenv()` or assigns `os.environ[...]`. BAML must
see those values:

  * `env.NAME` in client options and `baml.env.get(...)` are read at each call,
    so a value set after import, or changed between calls, is the one used.
  * `BOUNDARY_URL` / `BOUNDARY_API_KEY` are read once, when telemetry is set
    up — which is at the first BAML call, not at import.

Each scenario runs `env_timing_program.py` in a fresh interpreter (import order
cannot be replayed in-process) against a local capture server; no real keys.
"""
import json
import os
import subprocess
import sys
from pathlib import Path

import baml_sdk

# The generated tree: where `baml_sdk/` lives and this overlay is linked into.
# The program's own directory is not it — Python resolves the script symlink.
GENERATED = Path(baml_sdk.__file__).resolve().parent.parent
PROGRAM = GENERATED / "env_timing_program.py"

# Variables the scenarios set themselves, plus ones that would change what a
# scenario observes: with `OPENAI_API_KEY` set, an unset `env.NAME` api_key
# falls back to it; `BAML_TELEMETRY=off` disables trace delivery.
SCRUBBED = (
    "BAML_REPLAY_BASE_URL",
    "BAML_REPLAY_API_KEY",
    "BOUNDARY_URL",
    "BOUNDARY_API_KEY",
    "OPENAI_API_KEY",
    "BAML_TELEMETRY",
)

MISSING_KEY = "set one of the variables it consulted — BAML_REPLAY_API_KEY, OPENAI_API_KEY"


def run_program(scenario: str) -> dict:
    env = {key: value for key, value in os.environ.items() if key not in SCRUBBED}
    env["PYTHONPATH"] = os.pathsep.join(filter(None, [str(GENERATED), env.get("PYTHONPATH")]))
    result = subprocess.run(
        [sys.executable, str(PROGRAM), scenario],
        cwd=GENERATED,
        env=env,
        capture_output=True,
        text=True,
        timeout=120,
        check=False,
    )
    assert result.returncode == 0, f"{scenario} failed:\n{result.stdout}\n{result.stderr}"
    return json.loads(result.stdout.splitlines()[-1])


def llm_authorizations(report: dict) -> list[str]:
    return [r["authorization"] for r in report["requests"] if r["path"] == "/responses"]


def boundary_authorizations(report: dict) -> list[str]:
    return [r["authorization"] for r in report["requests"] if r["path"].startswith("/v1/recordings/")]


# SDK_PARITY_LINT(skip): Python import-order behavior of os.environ
def test_env_set_before_import_reaches_the_client():
    report = run_program("set_then_import")
    assert report["calls"] == ["ok"]
    assert llm_authorizations(report) == ["Bearer key-set-before-import"]


# SDK_PARITY_LINT(skip): Python import-order behavior of os.environ
def test_env_set_after_import_reaches_the_client():
    report = run_program("import_then_set")
    assert report["calls"] == ["ok"]
    assert llm_authorizations(report) == ["Bearer key-set-after-import"]


# SDK_PARITY_LINT(skip): Python import-order behavior of os.environ
def test_env_changed_between_calls_is_read_at_each_call():
    report = run_program("change_between_calls")
    unset, get_1, call_1, get_2, call_2, get_deleted, deleted = report["calls"]
    # Before anything is set, and again once the key is deleted, the call fails
    # locally naming the variable; nothing is sent.
    assert MISSING_KEY in unset
    assert MISSING_KEY in deleted
    assert (get_1, call_1) == ("env.get: key-1", "ok")
    assert (get_2, call_2) == ("env.get: key-2", "ok")
    assert get_deleted == "env.get: None"
    assert llm_authorizations(report) == ["Bearer key-1", "Bearer key-2"]


# SDK_PARITY_LINT(skip): Python import-order behavior of os.environ
def test_boundary_env_set_before_import_delivers_traces():
    report = run_program("boundary_set_then_import")
    assert report["calls"] == ["ok"]
    assert "Bearer boundary-key" in boundary_authorizations(report)


# SDK_PARITY_LINT(skip): Python import-order behavior of os.environ
def test_boundary_env_set_after_import_delivers_traces():
    report = run_program("boundary_import_then_set")
    assert report["calls"] == ["ok"]
    assert "Bearer boundary-key" in boundary_authorizations(report)
