"""Actual SDK process exits must finalize queryable profiler recordings."""

import json
import os
import subprocess
import sys

import pytest

from test_recordings import CLI, ROOT, run


@pytest.mark.parametrize("language", ["python", "typescript"])
@pytest.mark.parametrize("mode", ["normal", "explicit", "nonzero"])
def test_sdk_shutdown_finalizes_the_profiler(language, mode, tmp_path):
    assert CLI.is_file(), "Build cargo build -p baml_cli in this checkout first"
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith("BOUNDARY_")
    }
    env.update(
        HOME=str(tmp_path),
        BAML_HOME=str(tmp_path / "config" / ".baml"),
        BAML_TELEMETRY="high",
        BOUNDARY_API_KEY="local",
        DO_NOT_TRACK="1",
    )
    source = "function SDKLifecycleProbe() -> int { 7 }"
    if language == "python":
        script = f"""
from baml_bridge import BamlRuntime, call_function_sync, shutdown_runtime
runtime = BamlRuntime.initialize_runtime('.', {{'main.baml': {source!r}}})
for _ in range(3):
    assert call_function_sync(runtime, 'SDKLifecycleProbe', {{}}).result() == 7
if {mode!r} == 'explicit':
    shutdown_runtime()
    shutdown_runtime()
if {mode!r} == 'nonzero':
    raise SystemExit(7)
"""
        command = [sys.executable, "-c", script]
    else:
        bridge = ROOT / "sdks/typescript/bridge_typescript/dist"
        script = f"""
import {{ BamlRuntime, callFunctionSync }} from {json.dumps((bridge / "index.js").as_uri())};
import {{ shutdownRuntime }} from {json.dumps((bridge / "native.js").as_uri())};
const runtime = BamlRuntime.initializeRuntime('.', {{'main.baml': {json.dumps(source)}}});
for (let i = 0; i < 3; i++) {{
    if (callFunctionSync(runtime, 'SDKLifecycleProbe', {{}}).result() !== 7) {{
        throw new Error('unexpected result');
    }}
}}
if ({json.dumps(mode)} === 'explicit') {{
    await shutdownRuntime();
    await shutdownRuntime();
}}
if ({json.dumps(mode)} === 'nonzero') process.exitCode = 7;
"""
        command = ["node", "--input-type=module", "--eval", script]
    execution = subprocess.run(
        command, cwd=tmp_path, env=env, capture_output=True, text=True, timeout=90
    )
    (tmp_path / "execution.stdout").write_text(execution.stdout)
    (tmp_path / "execution.stderr").write_text(execution.stderr)
    assert execution.returncode == (7 if mode == "nonzero" else 0), execution.stderr
    sql = """SELECT p.status, p.status_history[1]['status'],
        (SELECT SUM(invocation_count) FROM profiler f
         WHERE f.process_id = p.process_id AND f.function_name = 'user.SDKLifecycleProbe')
        FROM processes p"""
    result = json.loads(
        run(
            [
                str(CLI),
                "query",
                "--local",
                "--from",
                str(tmp_path / "config"),
                "--format",
                "json",
                sql,
            ],
            tmp_path,
            env,
            tmp_path,
            "query",
        )
    )
    assert result["outcome"]["status"] == "complete", result["outcome"]
    assert not result["outcome"]["unsealed"], result["outcome"]
    assert not result["outcome"]["diagnostics"], result["outcome"]
    assert result["rows"] == [["unknown", "unknown", 3]], result["rows"]
