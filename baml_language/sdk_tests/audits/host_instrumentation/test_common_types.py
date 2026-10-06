"""Exact recorded host projections, including parity with native BAML classes."""

import json
import os
from pathlib import Path
import sys

import pytest

from test_recordings import CLI, PYTHON_SDK, TYPESCRIPT_SDK, run


@pytest.fixture(scope="module")
def common_python(tmp_path_factory):
    home = tmp_path_factory.mktemp("common-python")
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith("BOUNDARY_")
    }
    env.update(
        HOME=str(home),
        BAML_HOME=str(home / "config"),
        BAML_TELEMETRY="low",
        BOUNDARY_API_KEY="local",
        DO_NOT_TRACK="1",
        PYTHONPATH=str(PYTHON_SDK),
    )
    run(
        [sys.executable, str(Path(__file__).with_name("python_common_cases.py"))],
        home,
        env,
        home,
        "execution",
    )

    def query(sql, label, truncated=False):
        output = run(
            [
                str(CLI),
                "query",
                "--local",
                "--from",
                str(home),
                "--format",
                "json",
                sql,
            ],
            home,
            env,
            home,
            label,
            success_codes=(1,) if truncated else (0,),
        )
        result = json.loads(output)
        assert result["outcome"]["status"] == (
            "incomplete" if truncated else "complete"
        ), result["outcome"]
        if truncated:
            assert {d["code"] for d in result["outcome"]["diagnostics"]} == {
                "value_truncated"
            }
        else:
            assert not result["outcome"]["diagnostics"]
        assert not result["outcome"]["unsealed"]
        return result["rows"]

    return query


def test_common_python_capture_values(common_python):
    rows = common_python(
        """SELECT context_metadata['case'], input_args['value'], output_value,
        error_value FROM spans WHERE span_type='function' AND span_name LIKE 'python:%'
        AND context_metadata['probe']='common_types'
        AND context_metadata['case'] NOT IN ('value_budget', 'byte_budget')
        ORDER BY context_metadata['case']""",
        "values",
    )
    by_case = {row[0]: row for row in rows}
    native = {"$class": "user.methods_on_classes.Greeter", "name": "hello"}
    model = {"value": 7, "when": "2026-10-06T12:30:45+00:00"}
    exception = {"type": "ValueError", "args": ["problem"]}
    expected = {
        "native": native,
        "native_enum": "HAPPY",
        "async_native": native,
        "native_precedence": native,
        "generic_native": {"$class": "user.generic_tests.GenericBox", "value": 7},
        "pydantic": model,
        "hostile_model": model,
        "date": "2026-10-06",
        "datetime": "2026-10-06T12:30:45+00:00",
        "naive_datetime": "2026-10-06T12:30:45",
        "exception_value": exception,
        "nested": {"models": [native, model], "error": exception},
        "custom": {"custom": 7},
        "custom_subclass": {"custom": 7},
        "builtin_precedence": {"safe": 7},
        "custom_failure": {"$opaque": "host_value"},
        "custom_cycle": {"safe": 7, "cycle": {"$opaque": "host_value"}},
    }
    assert set(by_case) == set(expected) | {
        "raised",
        "callback_native",
        "callback_enum",
        "custom_exception",
    }
    for case, value in expected.items():
        assert by_case[case][1] == value, case
        assert by_case[case][2] == value, case
    assert by_case["raised"][3] == exception
    assert by_case["custom_exception"][3] == {"code": 7}
    assert by_case["callback_enum"][2] == expected["native_enum"]
    assert by_case["callback_native"][2] == {
        "$class": "user.host_callable_tests.Person",
        "name": "hello",
        "age": 7,
    }


def test_host_native_class_equals_baml_native_capture(common_python):
    assert common_python(
        """SELECT host.output_value = native.input_args['mood']
        FROM spans host JOIN spans native ON native.context_metadata['case']='baml_enum'
        WHERE host.context_metadata['case']='native_enum'
        AND host.span_type='function'
        AND native.span_type='function'
        AND native.span_name='user.host_callable_tests.call_enum_roundtrip_callback'""",
        "enum_equality",
    ) == [[1]]
    assert common_python(
        """SELECT host.output_value = native.input_args['self']
        FROM spans host JOIN spans native
        ON native.context_metadata['case']='baml_native'
        WHERE host.context_metadata['case']='native'
        AND host.span_type='function' AND native.span_type='function'""",
        "native_equality",
    ) == [[1]]
    assert common_python(
        """SELECT host.output_value = native.input_args['self']
        FROM spans host JOIN spans native
        ON native.context_metadata['case']='baml_generic'
        WHERE host.context_metadata['case']='generic_native'
        AND host.span_type='function' AND native.span_type='function'""",
        "generic_equality",
    ) == [[1]]
    assert common_python(
        """SELECT host.input_args[0][0] = native.input_args['p'],
        host.output_value = native.output_value
        FROM spans host JOIN spans native ON native.context_metadata['case']='baml_callback'
        WHERE host.context_metadata['case']='callback_native'
        AND host.span_type='function' AND native.span_type='function'""",
        "callback_equality",
    ) == [[1, 1]]


@pytest.fixture(scope="module")
def common_typescript(tmp_path_factory):
    home = tmp_path_factory.mktemp("common-typescript")
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith("BOUNDARY_")
    }
    env.update(
        HOME=str(home),
        BAML_HOME=str(home / "config"),
        BAML_TELEMETRY="low",
        BOUNDARY_API_KEY="local",
        DO_NOT_TRACK="1",
        BAML_HOST_RECORDING_AUDIT="1",
    )
    run(
        [
            "pnpm",
            "exec",
            "vitest",
            "run",
            "--config",
            "vitest.node.config.ts",
            "node/host_common_types.test.ts",
            "--maxWorkers",
            "1",
        ],
        TYPESCRIPT_SDK,
        env,
        home,
        "execution",
    )

    def query(sql, label, truncated=False):
        output = run(
            [
                str(CLI),
                "query",
                "--local",
                "--from",
                str(home),
                "--format",
                "json",
                sql,
            ],
            home,
            env,
            home,
            label,
            success_codes=(1,) if truncated else (0,),
        )
        result = json.loads(output)
        assert result["outcome"]["status"] == (
            "incomplete" if truncated else "complete"
        ), result["outcome"]
        if truncated:
            assert {d["code"] for d in result["outcome"]["diagnostics"]} == {
                "value_truncated"
            }
        else:
            assert not result["outcome"]["diagnostics"]
        assert not result["outcome"]["unsealed"]
        return result["rows"]

    return query


def test_common_typescript_capture_values(common_typescript):
    rows = common_typescript(
        """SELECT context_metadata['case'], input_args[0],
        output_value, error_value FROM spans WHERE span_type='function'
        AND span_name LIKE 'typescript:%' AND context_metadata['probe']='common_types'
        AND context_metadata['case'] NOT IN ('value_budget', 'byte_budget')
        ORDER BY context_metadata['case']""",
        "values",
    )
    by_case = {row[0]: row for row in rows}
    native = {"$class": "user.methods_on_classes.Greeter", "name": "hello"}
    exception = {"type": "TypeError", "message": "problem"}
    expected = {
        "native": native,
        "native_precedence": native,
        "async_native": native,
        "generic_native": {"$class": "user.generic_tests.GenericBox", "value": 7},
        "date": "2026-10-06T12:30:45.000Z",
        "hostile_date": "2026-10-06T12:30:45.000Z",
        "exception_value": exception,
        "exception_subclass": {"type": "ValidationError", "message": "invalid"},
        "nested": {"models": [native], "error": exception},
        "getter": {"safe": 7, "danger": {"$opaque": "host_value"}},
        "proxy": {"$opaque": "host_value"},
        "invalid_date": {"$opaque": "host_value"},
        "custom": {"custom": 7},
        "custom_subclass": {"custom": 7},
        "custom_failure": {"$opaque": "host_value"},
        "custom_cycle": {"safe": 7, "cycle": {"$opaque": "host_value"}},
        "builtin_precedence": {"safe": 7},
    }
    assert set(by_case) == set(expected) | {"raised", "callback_native"}
    for case, value in expected.items():
        assert by_case[case][1] == value, case
        assert by_case[case][2] == value, case
    assert by_case["raised"][3] == exception
    assert by_case["callback_native"][2] == {
        "$class": "user.host_callable_tests.Person",
        "name": "hello",
        "age": 7,
    }


def test_typescript_native_identity(common_typescript):
    for host, native in (("native", "baml_native"), ("generic_native", "baml_generic")):
        assert common_typescript(
            """SELECT host.output_value = native.input_args['self']
            FROM spans host JOIN spans native ON native.context_metadata['case']='"""
            + native
            + "' WHERE host.context_metadata['case']='"
            + host
            + "'"
            + " AND host.span_type='function' AND native.span_type='function'",
            host + "_equality",
        ) == [[1]]
    assert common_typescript(
        """SELECT host.input_args[0][0] = native.input_args['p'],
        host.output_value = native.output_value FROM spans host JOIN spans native
        ON native.context_metadata['case']='baml_callback'
        WHERE host.context_metadata['case']='callback_native'
        AND host.span_type='function' AND native.span_type='function'""",
        "callback_equality",
    ) == [[1, 1]]


@pytest.mark.parametrize("fixture", ["common_python", "common_typescript"])
def test_capture_budgets_are_explicit(request, fixture):
    query = request.getfixturevalue(fixture)
    assert query(
        """SELECT context_metadata['case'], output_value, baml_value_state(output_value)
        FROM spans WHERE span_type='function'
        AND context_metadata['probe']='common_types'
        AND context_metadata['case'] IN ('value_budget', 'byte_budget')
        ORDER BY context_metadata['case']""",
        "budgets",
        truncated=True,
    ) == [
        ["byte_budget", None, "value_truncated"],
        ["value_budget", None, "value_truncated"],
    ]
