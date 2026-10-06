"""What the Python bridge does to the host process: an unobserved error of a
spawned task, the wait at shutdown and at exit, and a runtime that does not
start.

The state is process-wide and several cases end the process, so those cases
run a child process with its own BAML program. The child needs only
`baml_bridge`: no generated SDK.
"""

import json
import os
import signal
import subprocess
import sys

import pytest

from baml_bridge import (
    BamlRuntime,
    default_unhandled_spawn_error_handler,
    get_bridge_runtime_version,
    get_toolchain_version,
    get_version,
    set_unhandled_spawn_error_handler,
    shutdown_runtime,
)


# A host chooses what an unobserved spawn error does to its process
# (`set_unhandled_spawn_error_handler`). The state is process-wide and the
# default ends the process, so each case runs in a child process: `main` leaves
# a failed task unobserved, and `cancelled_main` cancels a task whose cleanup
# then fails. A child that survives exits with status 42.
UNHANDLED_SPAWN_SCRIPT = """\
import json
import sys
import threading
import traceback

import baml_bridge
from baml_bridge import BamlRuntime, call_function_sync, shutdown_runtime

source = '''
function bad() -> int throws string { throw "boom" }
function main() -> int {
    spawn { bad() };
    baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
    1
}
function failing_cleanup() -> int {
    defer { throw "cleanup boom" }
    baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n));
    1
}
function cancelled_main() -> int {
    let pending = spawn { failing_cleanup() };
    baml.sys.sleep(baml.time.Duration.from_milliseconds(20n));
    pending.cancel();
    baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
    1
}
'''

seen = []


def record(error, cancelled):
    seen.append(
        {
            "type": type(error).__name__,
            "text": str(error),
            "cancelled": cancelled,
            "baml_frames": "main.baml" in "".join(traceback.format_exception(error)),
            "event_loop": _has_running_loop(),
        }
    )


def _has_running_loop():
    import asyncio

    try:
        asyncio.get_running_loop()
    except RuntimeError:
        return False
    return True


def run(entry):
    runtime = BamlRuntime.initialize_runtime(".", {"main.baml": source})
    assert call_function_sync(runtime, entry, {}).result() == 1
    shutdown_runtime()
    print(json.dumps(seen))
    raise SystemExit(42)


"""


def run_unhandled_spawn_script(body: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, "-c", UNHANDLED_SPAWN_SCRIPT + body],
        capture_output=True,
        text=True,
        check=False,
        timeout=60,
    )


# SDK_PARITY_LINT(skip): observes process-wide state of the Python bridge
def test_unhandled_spawn_error_handler_replaces_the_default():
    result = run_unhandled_spawn_script(
        """\
previous = baml_bridge.set_unhandled_spawn_error_handler(record)
assert previous is baml_bridge.default_unhandled_spawn_error_handler
run("main")
"""
    )

    # The process went on to its own exit, and the handler was the only report.
    assert result.returncode == 42, result.stderr
    assert "boom" not in result.stderr
    assert json.loads(result.stdout) == [
        {
            "type": "BamlError",
            "text": "str: 'boom'",
            "cancelled": False,
            "baml_frames": True,
            "event_loop": False,
        }
    ]


# SDK_PARITY_LINT(skip): observes process-wide state of the Python bridge
def test_unhandled_spawn_error_handler_none_restores_the_default():
    result = run_unhandled_spawn_script(
        """\
baml_bridge.set_unhandled_spawn_error_handler(record)
assert baml_bridge.set_unhandled_spawn_error_handler(None) is record
run("main")
"""
    )

    expected_returncode = signal.SIGTERM if os.name == "nt" else 1
    assert result.returncode == expected_returncode
    assert "boom" in result.stderr


# SDK_PARITY_LINT(skip): observes process-wide state of the Python bridge
def test_unhandled_spawn_error_handler_can_keep_the_default_for_some_errors():
    result = run_unhandled_spawn_script(
        """\
def fatal_unless_cancelled(error, cancelled):
    record(error, cancelled)
    baml_bridge.default_unhandled_spawn_error_handler(error, cancelled)

baml_bridge.set_unhandled_spawn_error_handler(fatal_unless_cancelled)
run("main")
"""
    )

    expected_returncode = signal.SIGTERM if os.name == "nt" else 1
    assert result.returncode == expected_returncode
    assert "boom" in result.stderr


# SDK_PARITY_LINT(skip): observes process-wide state of the Python bridge
def test_unhandled_spawn_error_handler_that_raises_is_reported_and_the_process_continues():
    result = run_unhandled_spawn_script(
        """\
def broken(error, cancelled):
    raise ValueError("the handler has a defect")

baml_bridge.set_unhandled_spawn_error_handler(broken)
run("main")
"""
    )

    assert result.returncode == 42, result.stderr
    # `sys.unraisablehook` reports the handler's exception, and with it the
    # error of the spawned task that the handler did not report.
    assert "the handler has a defect" in result.stderr
    assert "boom" in result.stderr


# SDK_PARITY_LINT(skip): observes process-wide state of the Python bridge
def test_unhandled_spawn_error_handler_is_told_when_the_task_was_cancelled():
    result = run_unhandled_spawn_script(
        """\
baml_bridge.set_unhandled_spawn_error_handler(record)
run("cancelled_main")
"""
    )

    assert result.returncode == 42, result.stderr
    [seen] = json.loads(result.stdout)
    assert seen["cancelled"] is True
    assert seen["text"] == "str: 'cleanup boom'"


# SDK_PARITY_LINT(skip): observes process-wide state of the Python bridge
def test_unhandled_spawn_error_default_prints_a_cancelled_task_and_continues():
    result = run_unhandled_spawn_script(
        """\
run("cancelled_main")
"""
    )

    assert result.returncode == 42, result.stderr
    assert "cleanup boom" in result.stderr


# SDK_PARITY_LINT(skip): observes process-wide state of the Python bridge
def test_set_unhandled_spawn_error_handler_returns_the_handler_it_replaces():
    def first(error: BaseException, cancelled: bool) -> None:
        pass

    def second(error: BaseException, cancelled: bool) -> None:
        pass

    original = set_unhandled_spawn_error_handler(first)
    try:
        assert original is default_unhandled_spawn_error_handler
        assert set_unhandled_spawn_error_handler(second) is first
        assert set_unhandled_spawn_error_handler(None) is second
        assert set_unhandled_spawn_error_handler(None) is default_unhandled_spawn_error_handler
    finally:
        set_unhandled_spawn_error_handler(original)


# A call whose cleanup outlasts it: cancelled, it unwinds into a `defer` that
# runs shielded for a minute.
SLOW_CLEANUP_SCRIPT = """\
import threading
from baml_bridge import BamlRuntime, call_function_sync

source = '''
function slow_cleanup() -> int {
    defer {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n));
    }
    baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n));
    0
}
'''
runtime = BamlRuntime.initialize_runtime(".", {"main.baml": source})
threading.Thread(
    target=lambda: call_function_sync(runtime, "slow_cleanup", {}),
    daemon=True,
).start()
"""


# SDK_PARITY_LINT(skip): observes process-wide state of the Python bridge
def test_shutdown_timeout_bounds_an_in_flight_call():
    script = (
        SLOW_CLEANUP_SCRIPT
        + """\
import time
from baml_bridge import shutdown_runtime

time.sleep(0.5)
started = time.monotonic()
shutdown_runtime(timeout=0.3)
print(f"shutdown took {time.monotonic() - started:.1f}s")
"""
    )
    result = subprocess.run(
        [sys.executable, "-c", script],
        capture_output=True,
        text=True,
        check=False,
        timeout=60,
    )

    assert result.returncode == 0, result.stderr
    took = float(result.stdout.strip().removeprefix("shutdown took ").removesuffix("s"))
    # The timeout plus one bounded settle window, not the cleanup's minute.
    assert took < 20


# SDK_PARITY_LINT(skip): observes process-wide state of the Python bridge
def test_shutdown_timeout_is_validated():
    for bad in (-1.0, float("nan"), float("inf")):
        with pytest.raises(ValueError):
            shutdown_runtime(timeout=bad)


@pytest.mark.skipif(os.name == "nt", reason="SIGINT delivery to a child is POSIX-only")
# SDK_PARITY_LINT(skip): observes process-wide state of the Python bridge
def test_exit_wait_ends_on_ctrl_c():
    # Exit waits for the call with no bound, as Python waits for non-daemon
    # threads; Ctrl+C ends that wait.
    process = subprocess.Popen(
        [sys.executable, "-c", SLOW_CLEANUP_SCRIPT + "import time\ntime.sleep(0.5)\n"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        # Past the script's own sleep, so the process is waiting at exit.
        with pytest.raises(subprocess.TimeoutExpired):
            process.wait(timeout=3)
        process.send_signal(signal.SIGINT)
        _, stderr = process.communicate(timeout=10)
    finally:
        process.kill()
    assert "KeyboardInterrupt" in stderr, stderr


# ============================================================================
# A runtime that starts, and one that does not
# ============================================================================


# SDK_PARITY_LINT(skip): observes process-wide state of the Python bridge
def test_get_version():
    version = get_version()
    assert isinstance(version, str)
    assert len(version) > 0


INITIALIZE_FROM_SOURCE_SCRIPT = """\
from baml_bridge import BamlPanic, BamlRuntime

assert BamlRuntime.initialize_runtime(".", {"empty.baml": ""}) is not None

try:
    BamlRuntime.initialize_runtime(".", {"bad.baml": 'function Bad() -> int { "not an int" }'})
except BamlPanic as panic:
    print(panic.class_name)
    print(panic)
else:
    raise SystemExit("initialize_runtime accepted a program that does not compile")
"""


# SDK_PARITY_LINT(skip): observes process-wide state of the Python bridge
def test_initialize_runtime_from_source_reports_compile_errors():
    """A program with no functions starts. A program that does not compile is
    a `baml.panics.SdkPanic` that carries the diagnostics. `initialize_runtime`
    replaces the runtime of its process, so the case runs in a child."""
    result = subprocess.run(
        [sys.executable, "-c", INITIALIZE_FROM_SOURCE_SCRIPT],
        capture_output=True,
        text=True,
        check=False,
        timeout=60,
    )

    assert result.returncode == 0, result.stderr
    assert result.stdout.startswith("baml.panics.SdkPanic\n"), result.stdout
    assert "bad.baml:1:" in result.stdout
    assert "error[E0001]" in result.stdout


# SDK_PARITY_LINT(skip): observes process-wide state of the Python bridge
def test_generated_bytecode_version_skew_fails_before_deserialization():
    """Generated SDK imports report bridge skew instead of a bytecode panic."""
    generated_toolchain = "999.0.0"
    embedded_baml_toml = f"""\
[package]
name = "version-skew-test"

[__baml_codegen]
metadata_version = 1

[__baml_codegen.toolchain]
version = "{generated_toolchain}"
"""

    with pytest.raises(RuntimeError) as exc_info:
        BamlRuntime.initialize_runtime_from_blob(
            b"\x00", embedded_baml_toml
        )

    message = str(exc_info.value)
    assert message.startswith("BAML startup failed: version skew error.")
    assert f"generated using BAML toolchain {generated_toolchain}" in message
    assert f"baml-bridge is installed at {get_bridge_runtime_version()}" in message
    assert (
        "expects baml_sdk to be generated using BAML toolchain "
        f"{get_toolchain_version()}" in message
    )
    assert f"`baml toolchain pin {get_toolchain_version()}`" in message
    assert "install `baml-bridge` (the Python package)" in message
    assert "then re-run `baml generate`" in message
    assert "Failed to deserialize BAML bytecode" not in message
