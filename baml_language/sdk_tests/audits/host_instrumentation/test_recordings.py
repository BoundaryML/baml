"""End-to-end host instrumentation: generated SDK -> native bridge -> baml query.

Prerequisites are deliberately explicit: build the CLI and both bridges and run
sdk_test_codegen for python_pydantic2 and typescript from this same checkout.
See README.md. No credentials or network-dependent BAML functions are used.
"""

import json
import os
from pathlib import Path
import subprocess
import sys

import pytest


ROOT = Path(__file__).resolve().parents[3]
CLI = Path(os.environ.get("BAML_AUDIT_CLI", ROOT / "target/debug/baml-cli"))
PYTHON_SDK = ROOT / "sdk_tests/crates/python_pydantic2/function_calls/generated"
TYPESCRIPT_SDK = ROOT / "sdk_tests/crates/typescript/function_calls/generated"


def run(command, cwd, env, evidence, name):
    result = subprocess.run(
        command, cwd=cwd, env=env, text=True, capture_output=True, timeout=90
    )
    (evidence / f"{name}.stdout").write_text(result.stdout)
    (evidence / f"{name}.stderr").write_text(result.stderr)
    assert result.returncode == 0, f"{command}\n{result.stdout}\n{result.stderr}"
    return result.stdout


@pytest.fixture(scope="module", params=["python", "typescript"])
def recording(request, tmp_path_factory):
    language = request.param
    tmp_path = tmp_path_factory.mktemp(language)
    assert CLI.is_file(), "Build cargo build -p baml_cli in this checkout first"
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith("BOUNDARY_")
    }
    env.update(
        HOME=str(tmp_path),
        BAML_HOME=str(tmp_path / "config"),
        BAML_TELEMETRY="low",
        BOUNDARY_API_KEY="local",
        DO_NOT_TRACK="1",
        BAML_HOST_RECORDING_AUDIT="1",
    )
    env["PYTHONPATH"] = str(PYTHON_SDK)
    if language == "python":
        command = [sys.executable, str(Path(__file__).with_name("python_cases.py"))]
        cwd = tmp_path
    else:
        command = [
            "pnpm",
            "exec",
            "vitest",
            "run",
            "--config",
            "vitest.node.config.ts",
            "node/host_recording_audit.test.ts",
            "--maxWorkers",
            "1",
        ]
        cwd = TYPESCRIPT_SDK
    run(command, cwd, env, tmp_path, "execution")
    assert list((tmp_path / ".baml/btel/recordings").glob("*")), (
        "native runtime wrote no recording"
    )

    def query(sql, name):
        output = run(
            [
                str(CLI),
                "query",
                "--local",
                "--from",
                str(tmp_path),
                "--format",
                "json",
                sql,
            ],
            tmp_path,
            env,
            tmp_path,
            name,
        )
        result = json.loads(output)
        assert result["outcome"]["status"] == "complete", result["outcome"]
        assert not result["outcome"]["diagnostics"], result["outcome"]
        assert result["outcome"]["unsealed"] is False, result["outcome"]
        return result["rows"]

    return language, query


def test_recorded_values_and_context(recording):
    language, query = recording
    rows = query(
        """SELECT context_metadata['case'] AS case_name, status, input_args,
        output_value, error_value, context_distinct_id, context_metadata, span_name,
        (SELECT p.span_type FROM spans p WHERE p.span_id = s.parent_span_id) AS parent_type,
        (SELECT g.span_name FROM spans p JOIN spans g ON g.span_id = p.parent_span_id
            WHERE p.span_id = s.parent_span_id) AS grandparent_name
        FROM spans s WHERE span_type = 'function' AND context_metadata['audit'] = '"""
        + language
        + """'
        ORDER BY start_time""",
        "spans",
    )
    # Host definition keys are language-specific. The original host function's
    # name occurs in Python's FQN; Node deliberately uses a stable source hash.
    hosts = [row for row in rows if row[7].startswith(language + ":")]
    assert len(hosts) == (11 if language == "python" else 12), hosts
    by_case = {row[0]: row for row in hosts}
    assert set(by_case) == (
        {
            "sync",
            "async",
            "method",
            "iterator",
            "error",
            "async_error",
            "call-0",
            "call-1",
            "marker",
            "classmethod",
            "staticmethod",
        }
        if language == "python"
        else {
            "sync",
            "async",
            "method",
            "iterator",
            "error",
            "async_error",
            "call-0",
            "call-1",
            "marker",
            "native_error",
            "staticmethod",
            "bound",
        }
    )
    assert by_case["sync"][1] == "return"
    assert by_case["sync"][3] == {"value": 8, "flag": True}
    assert by_case["async"][3] == 9
    assert by_case["method"][3] == 17
    assert by_case["staticmethod"][3] == 10
    if language == "python":
        assert by_case["sync"][2] == {
            "value": 7,
            "flag": True,
            "_baml": "application-data",
        }
        assert by_case["method"][2] == {"value": 7}
        assert by_case["classmethod"][2] == {"value": 7}
        assert by_case["classmethod"][3] == 9
    else:
        assert by_case["sync"][2] == [7, True]
        assert by_case["method"][2] == [7]
        assert by_case["bound"][2] == [7]
        assert by_case["bound"][3] == 17
    for case in ("error", "async_error"):
        assert by_case[case][1] == "user_error"
        assert by_case[case][3] is None
        expected = (
            {"type": "ValueError", "args": ["audit application failure"]}
            if language == "python"
            else {"message": "audit application failure", "code": 7}
        )
        assert by_case[case][4] == expected
    for index in range(2):
        row = by_case[f"call-{index}"]
        assert row[3] == index + 10
        assert row[5] == f"request-{index}"
        assert row[6] == {
            "audit": language,
            "case": f"call-{index}",
            "keep": 1,
            "caller": True,
        }
        assert row[8] == "future"
        assert row[9] == "user.host_callable_tests.call_configured_callback"
    assert by_case["marker"][5] == f"audit-{language}"
    assert by_case["marker"][6] == {
        "audit": language,
        "case": "marker",
        "keep": 1,
        "drop": 2,
    }
    assert by_case["marker"][8] == "future"
    assert by_case["marker"][9] is None
    # Current public SQL contract is FQN; custom labels remain definition metadata.
    assert all(row[7] != "audit custom sync" for row in hosts)


def test_error_and_iterator_capture_states(recording):
    language, query = recording
    rows = query(
        """SELECT context_metadata['case'], status, baml_value_state(output_value),
        baml_value_state(error_value) FROM spans WHERE span_type = 'function'
        AND span_name LIKE '"""
        + language
        + """:%' AND context_metadata['audit'] = '"""
        + language
        + """'
        AND context_metadata['case'] IN ('iterator', 'native_error')
        ORDER BY context_metadata['case']""",
        "capture_states",
    )
    assert rows == (
        [["iterator", "return", "present", "no_value"]]
        if language == "python"
        else [
            ["iterator", "return", "present", "no_value"],
            ["native_error", "user_error", "no_value", "present"],
        ]
    )
    opaque = query(
        """SELECT context_metadata['case'], output_value, error_value
        FROM spans WHERE span_type = 'function' AND span_name LIKE '"""
        + language
        + """:%'
        AND context_metadata['audit'] = '"""
        + language
        + """'
        AND context_metadata['case'] IN ('iterator', 'native_error')
        ORDER BY context_metadata['case']""",
        "opaque_values",
    )
    assert opaque == (
        [["iterator", {"$opaque": "host_value"}, None]]
        if language == "python"
        else [
            ["iterator", {"$opaque": "host_value"}, None],
            ["native_error", None, {"$opaque": "host_value"}],
        ]
    )


def test_host_input_navigation_and_announcements(recording):
    language, query = recording
    key = "['value']" if language == "python" else "[0]"
    for relation in ("spans", "span_announcements"):
        assert query(
            "SELECT input_args"
            + key
            + ", baml_value_state(input_args"
            + key
            + ") FROM "
            + relation
            + " WHERE span_type = 'function' AND span_name LIKE '"
            + language
            + ":%'"
            " AND context_metadata['case'] = 'sync'",
            relation + "_navigation",
        ) == [[7, "present"]]
