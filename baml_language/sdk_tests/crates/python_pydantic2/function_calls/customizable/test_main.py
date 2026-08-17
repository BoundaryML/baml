"""Smoke tests for plain (non-LLM) expression functions."""

import asyncio
import time

import pytest

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk import (
    hello_world,
    sdk_bridge_cancelled_log_async,
    sdk_bridge_detached_log,
    sdk_bridge_logs,
    single_required_arg,
)


def test_main_hello_world_returns_literal():
    assert hello_world() == "hello world"


def test_main_single_required_arg_round_trips():
    # The next step up from the nullary case: one required positional
    # argument round-trips through the engine unchanged.
    assert single_required_arg("hi") == "hi"


# SDK_PARITY_LINT(skip): native SDK stderr delivery is covered by the Python CFFI integration
def test_baml_logs_are_off_by_default(monkeypatch, capfd):
    monkeypatch.delenv("BAML_LOG", raising=False)
    assert sdk_bridge_logs() == 42

    stderr = capfd.readouterr().err
    assert "sdk bridge" not in stderr


# SDK_PARITY_LINT(skip): native SDK stderr delivery is covered by the Python CFFI integration
def test_baml_logs_reach_sdk_stderr_and_respect_level(monkeypatch, capfd):
    monkeypatch.setenv("BAML_LOG", "DEBUG")
    assert sdk_bridge_logs() == 42

    stderr = capfd.readouterr().err
    assert "[BAML DEBUG] sdk bridge debug" in stderr
    assert "[BAML INFO] sdk bridge info" in stderr
    assert '[BAML WARN] {"kind": "sdk bridge warning"}' in stderr
    assert "[BAML ERROR] sdk bridge error" in stderr

    monkeypatch.setenv("BAML_LOG", "ERROR")
    assert sdk_bridge_logs() == 42

    stderr = capfd.readouterr().err
    assert "sdk bridge debug" not in stderr
    assert "sdk bridge info" not in stderr
    assert "sdk bridge warning" not in stderr
    assert "[BAML ERROR] sdk bridge error" in stderr


# SDK_PARITY_LINT(skip): native SDK stderr delivery is covered by the Python CFFI integration
def test_baml_logs_from_detached_tasks_reach_sdk_stderr(monkeypatch, capfd):
    monkeypatch.setenv("BAML_LOG", "INFO")
    assert sdk_bridge_detached_log() == 42

    deadline = time.monotonic() + 2
    stderr = ""
    while "sdk bridge detached info" not in stderr and time.monotonic() < deadline:
        time.sleep(0.05)
        stderr += capfd.readouterr().err

    assert "[BAML INFO] sdk bridge detached info" in stderr


# SDK_PARITY_LINT(skip): native SDK stderr delivery is covered by the Python CFFI integration
async def test_baml_logs_survive_async_call_cancellation(monkeypatch, capfd):
    monkeypatch.setenv("BAML_LOG", "INFO")
    task = asyncio.create_task(sdk_bridge_cancelled_log_async())

    deadline = time.monotonic() + 2
    stderr = ""
    while "sdk bridge before cancel" not in stderr and time.monotonic() < deadline:
        await asyncio.sleep(0.05)
        stderr += capfd.readouterr().err
    assert "[BAML INFO] sdk bridge before cancel" in stderr

    task.cancel()

    with pytest.raises(asyncio.CancelledError):
        await task

    deadline = time.monotonic() + 2
    while "sdk bridge detached after cancel" not in stderr and time.monotonic() < deadline:
        await asyncio.sleep(0.05)
        stderr += capfd.readouterr().err

    assert "[BAML INFO] sdk bridge detached after cancel" in stderr
