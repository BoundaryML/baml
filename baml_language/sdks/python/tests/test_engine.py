"""
Tests for bridge_python: Python → PyO3 → bridge_cffi → bex_engine pipeline.

These tests embed BAML source directly and call functions through the full stack.
No LLM calls — only pure expression functions — so these run without API keys.

Run with:
    cd baml_language/crates/bridge_python
    uv run maturin develop --uv
    uv run pytest tests/ -v
"""

import os
import signal
import subprocess
import sys

import pytest

from baml_bridge import (
    BamlRuntime,
    FunctionResult,
    HostSpanManager,
    get_bridge_runtime_version,
    get_toolchain_version,
    get_version,
    call_function,
    call_function_sync,
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
            BamlRuntime.initialize_runtime_from_bytecode(
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


# ============================================================================
# TEST: HostSpanManager (stub — all should pass since they're no-ops)
# ============================================================================


class TestHostSpanManager:
    def test_create_host_span_manager(self):
        hsm = HostSpanManager()
        assert isinstance(hsm, HostSpanManager)

    def test_deep_clone(self):
        hsm = HostSpanManager()
        cloned = hsm.deep_clone()
        assert isinstance(cloned, HostSpanManager)

    def test_context_depth_is_zero(self):
        hsm = HostSpanManager()
        assert hsm.context_depth() == 0
