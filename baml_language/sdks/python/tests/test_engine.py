"""
Tests for bridge_python: Python → PyO3 → bridge_cffi → bex_engine pipeline.

These tests embed BAML source directly and call functions through the full stack.
No LLM calls — only pure expression functions — so these run without API keys.

Run with:
    cd baml_language/crates/bridge_python
    uv run maturin develop --uv
    uv run pytest tests/ -v
"""

import json
import os
import signal
import subprocess
import sys

import pytest

from baml_bridge import (
    BamlRuntime,
    FunctionResult,
    default_unhandled_spawn_error_handler,
    get_bridge_runtime_version,
    get_toolchain_version,
    get_version,
    call_function,
    call_function_sync,
    set_unhandled_spawn_error_handler,
)


# ============================================================================
# BAML source files used by tests.
# ============================================================================

EXPR_FUNCS_BAML = """\
function ReturnOne() -> int {
    1
}

function ReturnNumber(n: int) -> int {
    n
}

function CallReturnOne() -> int {
    ReturnOne()
}

function ChainedCalls() -> int {
    ReturnNumber(CallReturnOne())
}

function AddNumbers(a: int, b: int) -> int {
    a + b
}

function BoolToInt(b: bool) -> int {
    if (b) { 1 } else { 0 }
}

function Identity(s: string) -> string {
    s
}

function ReturnNull() -> null {
    null
}

function ReturnFloat(f: float) -> float {
    f
}

function ClassifyAmbiguousEmptyList(value: int[] | string[]) -> string {
    match (value) {
        let ints: int[] => "ints",
        let strings: string[] => "strings",
    }
}

function MakeAdder(offset: int) -> (value: int) -> int throws never {
    return (value: int) -> int { offset + value }
}

function MakeCounter(start: int) -> () -> int throws never {
    let current = start;
    return () -> int {
        current += 1;
        current
    }
}
"""


# ============================================================================
# Helpers
# ============================================================================


def make_runtime(baml_source: str) -> BamlRuntime:
    """Create a BamlRuntime from a single BAML source string."""
    return BamlRuntime.initialize_runtime(
        ".", {"main.baml": baml_source}
    )


def test_unhandled_spawn_error_uses_host_default():
    result = subprocess.run(
        [
            sys.executable,
            "-c",
            """\
from baml_bridge import BamlRuntime, call_function_sync, shutdown_runtime

source = '''
function bad() -> int throws string { throw "boom" }
function main() -> int {
    spawn { bad() };
    baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
    1
}
'''
runtime = BamlRuntime.initialize_runtime(".", {"main.baml": source})
assert call_function_sync(runtime, "main", {}).result() == 1
shutdown_runtime()
raise SystemExit(42)
""",
        ],
        capture_output=True,
        text=True,
        check=False,
    )

    expected_returncode = signal.SIGTERM if os.name == "nt" else 1
    assert result.returncode == expected_returncode
    assert "boom" in result.stderr


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


def test_unhandled_spawn_error_default_prints_a_cancelled_task_and_continues():
    result = run_unhandled_spawn_script(
        """\
run("cancelled_main")
"""
    )

    assert result.returncode == 42, result.stderr
    assert "cleanup boom" in result.stderr


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


def test_shutdown_timeout_is_validated():
    from baml_bridge import shutdown_runtime

    for bad in (-1.0, float("nan"), float("inf")):
        with pytest.raises(ValueError):
            shutdown_runtime(timeout=bad)


@pytest.mark.skipif(os.name == "nt", reason="SIGINT delivery to a child is POSIX-only")
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
# TEST: Basics — initialization and version
# ============================================================================


class TestBasics:
    def test_get_version(self):
        """get_version() returns a non-empty string."""
        v = get_version()
        assert isinstance(v, str)
        assert len(v) > 0

    def test_initialize_runtime_valid(self):
        """initialize_runtime succeeds with valid BAML source."""
        rt = make_runtime(EXPR_FUNCS_BAML)
        assert rt is not None

    @pytest.mark.xfail(
        reason="bex_engine does not validate BAML at initialization time"
    )
    def test_initialize_runtime_invalid_baml(self):
        """initialize_runtime raises on invalid BAML source (type error)."""
        bad_baml = 'function Bad() -> int { "not an int" }'
        with pytest.raises(Exception):
            BamlRuntime.initialize_runtime(
                ".", {"bad.baml": bad_baml}
            )

    def test_initialize_runtime_empty(self):
        """initialize_runtime succeeds with empty source (no functions)."""
        rt = BamlRuntime.initialize_runtime(
            ".", {"empty.baml": ""}
        )
        assert rt is not None

    def test_generated_bytecode_version_skew_fails_before_deserialization(self):
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


# ============================================================================
# TEST: Sync function calls through the full pipeline
# ============================================================================


class TestCallFunctionSync:
    """Test call_function_sync: Python → PyO3 → bridge_cffi → bex_engine."""

    def test_return_one(self):
        rt = make_runtime(EXPR_FUNCS_BAML)
        result = call_function_sync(rt,"ReturnOne", {})
        assert isinstance(result, FunctionResult)
        assert result.result() == 1

    def test_return_number(self):
        rt = make_runtime(EXPR_FUNCS_BAML)
        result = call_function_sync(rt,"ReturnNumber", {"n": 42})
        assert result.result() == 42

    def test_call_return_one(self):
        """Function calling another function."""
        rt = make_runtime(EXPR_FUNCS_BAML)
        result = call_function_sync(rt,"CallReturnOne", {})
        assert result.result() == 1

    @pytest.mark.xfail(
        reason="bex_engine bug: nested call expressions not yet supported"
    )
    def test_chained_calls(self):
        """Chained function calls: ReturnNumber(CallReturnOne())."""
        rt = make_runtime(EXPR_FUNCS_BAML)
        result = call_function_sync(rt,"ChainedCalls", {})
        assert result.result() == 1

    def test_add_numbers(self):
        """Multiple arguments in correct order."""
        rt = make_runtime(EXPR_FUNCS_BAML)
        result = call_function_sync(rt,"AddNumbers", {"a": 10, "b": 32})
        assert result.result() == 42

    def test_bool_to_int(self):
        """Boolean argument → int result via if/else."""
        rt = make_runtime(EXPR_FUNCS_BAML)
        assert call_function_sync(rt,"BoolToInt", {"b": True}).result() == 1
        assert call_function_sync(rt,"BoolToInt", {"b": False}).result() == 0

    def test_identity_string(self):
        """String argument round-trip."""
        rt = make_runtime(EXPR_FUNCS_BAML)
        result = call_function_sync(rt,"Identity", {"s": "hello world"})
        assert result.result() == "hello world"

    def test_return_null(self):
        """Null return type."""
        rt = make_runtime(EXPR_FUNCS_BAML)
        result = call_function_sync(rt,"ReturnNull", {})
        assert result.result() is None

    def test_return_float(self):
        """Float argument round-trip."""
        rt = make_runtime(EXPR_FUNCS_BAML)
        result = call_function_sync(rt,"ReturnFloat", {"f": 3.14})
        assert abs(result.result() - 3.14) < 0.001

    def test_raw_empty_list_uses_dynamic_union_default(self):
        """A raw Python [] selects the first matching list arm."""
        rt = make_runtime(EXPR_FUNCS_BAML)
        result = call_function_sync(rt, "ClassifyAmbiguousEmptyList", {"value": []})
        assert result.result() == "ints"

    def test_returned_closure_accepts_args_and_decodes_results(self):
        rt = make_runtime(EXPR_FUNCS_BAML)
        add_ten = call_function_sync(rt, "MakeAdder", {"offset": 10}).result()
        assert callable(add_ten)
        assert add_ten(5) == 15
        assert add_ten(value=7) == 17

    def test_returned_closure_is_reusable_and_retains_captures(self):
        rt = make_runtime(EXPR_FUNCS_BAML)
        next_value = call_function_sync(rt, "MakeCounter", {"start": 40}).result()
        assert next_value() == 41
        assert next_value() == 42

    def test_missing_argument_raises(self):
        """Missing required argument raises an error.

        The engine reports this as ``Invalid argument: <name>`` rather than
        ``Missing argument``; the test only asserts that *an* argument-
        related error surfaces so it survives that phrasing tweak.
        """
        rt = make_runtime(EXPR_FUNCS_BAML)
        with pytest.raises(Exception, match="argument"):
            call_function_sync(rt,"ReturnNumber", {})

    def test_function_not_found_raises(self):
        """Calling a nonexistent function raises an error."""
        rt = make_runtime(EXPR_FUNCS_BAML)
        with pytest.raises(Exception, match="not found"):
            call_function_sync(rt,"NoSuchFunction", {})


# ============================================================================
# TEST: A plain string where an enum is declared
# ============================================================================

ENUM_ARGS_BAML = """\
enum HostClientName {
    BedrockSonnet5
    AgentPrimary
}

function Describe(client_name: HostClientName) -> string {
    match (client_name) {
        HostClientName.BedrockSonnet5 => "bedrock",
        HostClientName.AgentPrimary => "primary",
    }
}

function Resolve(client_name: HostClientName?) -> string {
    match (client_name) {
        null => "default",
        let named: HostClientName => Describe(named),
    }
}

function ResolveAll(client_names: HostClientName[]) -> string {
    client_names.map((name) -> { Describe(name) }).join(",")
}

function NameOrText(value: string | HostClientName) -> string {
    match (value) {
        let text: string => "string",
        let named: HostClientName => "enum",
    }
}
"""


class TestStringForEnum:
    """A host reads an enum's variant from JSON, a flag or a configuration
    file, so it holds a `str`. The engine reads a plain string as the variant
    it names where an enum is declared. `Describe` matches on the variants: a
    string that only sat in the enum's place would match no arm."""

    def test_string_names_a_variant_of_an_optional_enum(self):
        rt = make_runtime(ENUM_ARGS_BAML)
        result = call_function_sync(rt, "Resolve", {"client_name": "BedrockSonnet5"})
        assert result.result() == "bedrock"
        assert call_function_sync(rt, "Resolve", {"client_name": None}).result() == "default"

    def test_string_names_a_variant_of_a_required_enum(self):
        rt = make_runtime(ENUM_ARGS_BAML)
        result = call_function_sync(rt, "Describe", {"client_name": "AgentPrimary"})
        assert result.result() == "primary"

    def test_strings_in_a_list_of_enums_name_variants(self):
        rt = make_runtime(ENUM_ARGS_BAML)
        result = call_function_sync(
            rt, "ResolveAll", {"client_names": ["AgentPrimary", "BedrockSonnet5"]}
        )
        assert result.result() == "primary,bedrock"

    @pytest.mark.parametrize("function", ["Describe", "Resolve"])
    def test_string_that_names_no_variant_raises(self, function):
        rt = make_runtime(ENUM_ARGS_BAML)
        with pytest.raises(Exception):
            call_function_sync(rt, function, {"client_name": "Gpt9"})

    def test_string_stays_a_string_beside_a_string_member(self):
        rt = make_runtime(ENUM_ARGS_BAML)
        result = call_function_sync(rt, "NameOrText", {"value": "AgentPrimary"})
        assert result.result() == "string"


# ============================================================================
# TEST: An argument must inhabit its declared type
# ============================================================================

ARGUMENT_TYPES_BAML = """
class Reading {
    label: string
    count: int
}

function Twice(n: int) -> int {
    n * 2
}

function Half(x: float) -> float {
    x / 2.0
}

function Shout(text: string) -> string {
    text.to_upper_case()
}

function Negate(flag: bool) -> bool {
    !flag
}

function Total(counts: int[]) -> int {
    counts.reduce((sum, count) -> { sum + count }, 0)
}

function Count(reading: Reading) -> int {
    reading.count
}

function OrZero(n: int?) -> int {
    n ?? 0
}
"""


class TestArgumentTypes:
    """The engine checks an argument against the declared type before the
    function runs. A value of another kind entered the function as it was
    before: `Twice("7")` ran with a string in the `int` parameter."""

    @pytest.mark.parametrize(
        ("function", "args"),
        [
            ("Twice", {"n": "7"}),
            ("Twice", {"n": 1.5}),
            ("Twice", {"n": True}),
            ("Twice", {"n": None}),
            ("Twice", {"n": [1]}),
            ("Half", {"x": "1.5"}),
            ("Shout", {"text": 7}),
            ("Negate", {"flag": 1}),
            ("Total", {"counts": [1, "two"]}),
            ("Total", {"counts": "12"}),
            ("Count", {"reading": {"label": "a", "count": "seven"}}),
            ("Count", {"reading": "a"}),
        ],
    )
    def test_value_of_another_kind_raises(self, function, args):
        rt = make_runtime(ARGUMENT_TYPES_BAML)
        with pytest.raises(Exception):
            call_function_sync(rt, function, args)

    def test_values_of_the_declared_kind_are_accepted(self):
        rt = make_runtime(ARGUMENT_TYPES_BAML)
        assert call_function_sync(rt, "Twice", {"n": 7}).result() == 14
        assert call_function_sync(rt, "Shout", {"text": "hi"}).result() == "HI"
        assert call_function_sync(rt, "Negate", {"flag": True}).result() is False
        assert call_function_sync(rt, "Total", {"counts": [1, 2]}).result() == 3
        reading = {"label": "a", "count": 7}
        assert call_function_sync(rt, "Count", {"reading": reading}).result() == 7
        assert call_function_sync(rt, "OrZero", {"n": None}).result() == 0

    def test_int_is_accepted_where_a_float_is_declared(self):
        rt = make_runtime(ARGUMENT_TYPES_BAML)
        assert call_function_sync(rt, "Half", {"x": 3}).result() == 1.5


# ============================================================================
# TEST: Async function calls
# ============================================================================


class TestCallFunctionAsync:
    """Test call_function (async): Python → PyO3 → bridge_cffi → bex_engine."""

    @pytest.mark.asyncio
    async def test_return_one_async(self):
        rt = make_runtime(EXPR_FUNCS_BAML)
        result = await call_function(rt,"ReturnOne", {})
        assert isinstance(result, FunctionResult)
        assert result.result() == 1

    @pytest.mark.asyncio
    async def test_add_numbers_async(self):
        rt = make_runtime(EXPR_FUNCS_BAML)
        result = await call_function(rt,"AddNumbers", {"a": 100, "b": 200})
        assert result.result() == 300

    @pytest.mark.asyncio
    async def test_identity_string_async(self):
        rt = make_runtime(EXPR_FUNCS_BAML)
        result = await call_function(rt,"Identity", {"s": "async hello"})
        assert result.result() == "async hello"
