"""
Tests for bridge_python: Python → PyO3 → bridge_cffi → bex_engine pipeline.

These tests embed BAML source directly and call functions through the full stack.
No LLM calls — only pure expression functions — so these run without API keys.

Run with:
    cd baml_language/crates/bridge_python
    uv run maturin develop --uv
    uv run pytest tests/ -v
"""

import contextlib
import json
import os
import tempfile

import pytest

from baml_py import BamlRuntime, FunctionResult, HostSpanManager, flush_events, get_version, call_function, call_function_sync


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
"""


# ============================================================================
# Helpers
# ============================================================================


def make_runtime(baml_source: str) -> BamlRuntime:
    """Create a BamlRuntime from a single BAML source string."""
    return BamlRuntime.from_files(".", {"main.baml": baml_source})


# ============================================================================
# TEST: Basics — initialization and version
# ============================================================================


class TestBasics:
    def test_get_version(self):
        """get_version() returns a non-empty string."""
        v = get_version()
        assert isinstance(v, str)
        assert len(v) > 0

    def test_from_files_valid(self):
        """from_files succeeds with valid BAML source."""
        rt = make_runtime(EXPR_FUNCS_BAML)
        assert rt is not None

    @pytest.mark.xfail(
        reason="bex_engine does not validate BAML at initialization time"
    )
    def test_from_files_invalid_baml(self):
        """from_files raises on invalid BAML source (type error)."""
        bad_baml = 'function Bad() -> int { "not an int" }'
        with pytest.raises(Exception):
            BamlRuntime.from_files(".", {"bad.baml": bad_baml})

    def test_from_files_empty(self):
        """from_files succeeds with empty source (no functions)."""
        rt = BamlRuntime.from_files(".", {"empty.baml": ""})
        assert rt is not None


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

    def test_missing_argument_raises(self):
        """Missing required argument raises an error."""
        rt = make_runtime(EXPR_FUNCS_BAML)
        with pytest.raises(Exception, match="Missing argument"):
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


# ============================================================================
# TEST: Tracing — event publishing via global EventStore
# ============================================================================


class TestTracing:
    """
    Tracing tests. These verify that the global EventStore + publisher
    thread properly records events from both host-language @trace decorators
    and engine function calls.
    """

    @staticmethod
    @contextlib.contextmanager
    def _trace_file():
        """Context manager that sets BAML_TRACE_FILE to a temp file.

        Saves and restores the original BAML_TRACE_FILE so that an
        externally-set value (e.g. ``BAML_TRACE_FILE=debug.jsonl pytest``)
        is never lost.
        """
        orig = os.environ.get("BAML_TRACE_FILE")
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".jsonl", delete=False
        ) as f:
            trace_file = f.name
        try:
            os.environ["BAML_TRACE_FILE"] = trace_file
            yield trace_file
        finally:
            if orig is not None:
                os.environ["BAML_TRACE_FILE"] = orig
            else:
                os.environ.pop("BAML_TRACE_FILE", None)
            try:
                os.unlink(trace_file)
            except OSError:
                pass

    def test_trace_decorator_sync(self):
        """@trace decorator records function start/end events."""
        from baml_py import BamlCtxManager

        rt = make_runtime(EXPR_FUNCS_BAML)
        ctx = BamlCtxManager(rt)

        @ctx.trace_fn
        def traced_function(x: int) -> int:
            return x * 2

        with self._trace_file() as trace_file:
            result = traced_function(21)
            assert result == 42

            ctx.flush()

            with open(trace_file) as f:
                lines = [line.strip() for line in f if line.strip()]

            assert len(lines) >= 2, f"Expected at least 2 events, got {len(lines)}"
            events = [json.loads(line) for line in lines]

            types = [e["content"]["type"] for e in events]
            assert "function_start" in types
            assert "function_end" in types

    @pytest.mark.asyncio
    async def test_trace_decorator_async(self):
        """@trace decorator works with async functions."""
        from baml_py import BamlCtxManager

        rt = make_runtime(EXPR_FUNCS_BAML)
        ctx = BamlCtxManager(rt)

        @ctx.trace_fn
        async def traced_async_fn(s: str) -> str:
            return f"traced: {s}"

        with self._trace_file() as trace_file:
            result = await traced_async_fn("hello")
            assert result == "traced: hello"

            ctx.flush()

            with open(trace_file) as f:
                lines = [line.strip() for line in f if line.strip()]

            assert len(lines) >= 2, f"Expected at least 2 events, got {len(lines)}"
            events = [json.loads(line) for line in lines]
            types = [e["content"]["type"] for e in events]
            assert "function_start" in types
            assert "function_end" in types

    def test_nested_trace_callstack(self):
        """Nested @trace calls build a proper call stack."""
        from baml_py import BamlCtxManager

        rt = make_runtime(EXPR_FUNCS_BAML)
        ctx = BamlCtxManager(rt)

        call_stack_depths = []

        @ctx.trace_fn
        def outer():
            call_stack_depths.append(ctx.get().context_depth())
            inner()
            return "outer"

        @ctx.trace_fn
        def inner():
            call_stack_depths.append(ctx.get().context_depth())
            return "inner"

        outer()

        # outer should be at depth 1, inner at depth 2
        assert call_stack_depths == [
            1,
            2,
        ], f"Expected [1, 2] but got {call_stack_depths}"

    def test_flush_trace_events(self):
        """Flushing writes trace events to the JSONL file."""
        from baml_py import BamlCtxManager

        rt = make_runtime(EXPR_FUNCS_BAML)
        ctx = BamlCtxManager(rt)

        with self._trace_file() as trace_file:
            @ctx.trace_fn
            def traced_fn():
                return 42

            traced_fn()
            flush_events()

            with open(trace_file) as f:
                content = f.read()

            assert len(content) > 0, "Trace file should not be empty after flush"

    def test_tag_propagation(self):
        """Tags set on the current span are emitted as SetTags events."""
        from baml_py import BamlCtxManager

        rt = make_runtime(EXPR_FUNCS_BAML)
        ctx = BamlCtxManager(rt)

        with self._trace_file() as trace_file:
            @ctx.trace_fn
            def tagged_fn():
                ctx.upsert_tags(env="test", version="1.0")
                return "done"

            tagged_fn()
            ctx.flush()

            with open(trace_file) as f:
                lines = [line.strip() for line in f if line.strip()]

            events = [json.loads(line) for line in lines]

            intermediate_events = [
                e for e in events if e["content"]["type"] == "intermediate"
            ]
            assert len(intermediate_events) >= 1, (
                f"Expected at least 1 SetTags event, got {len(intermediate_events)}"
            )

            set_tags = intermediate_events[0]["content"]["data"]["SetTags"]
            assert set_tags["env"] == "test"
            assert set_tags["version"] == "1.0"
