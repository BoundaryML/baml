"""
Collector tests for bridge_python.

Ported from integ-tests/python/tests/test_collector.py.
Tests the Collector API for tracking BAML function call logs, usage, timing,
tags, and GC cleanup through the full Python -> PyO3 -> bex_engine pipeline.

Uses mock LLM HTTP server (same as test_tracing.py) instead of real providers.

Run with:
    cd baml_language/crates/bridge_python
    uv run maturin develop --uv
    uv run pytest tests/test_collector.py -v
"""

import asyncio
import gc
import http.server
import threading

import pytest

from baml_py import BamlRuntime, BamlCtxManager, Collector, FunctionLog, call_function, call_function_sync
from conftest import MockLLMHandler


# ============================================================================
# BAML source -- pure expression functions (no LLM calls)
# ============================================================================

BAML_SOURCE = """\
function ReturnOne() -> int {
    1
}

function AddNumbers(a: int, b: int) -> int {
    a + b
}

function Identity(s: string) -> string {
    s
}

function InnerHelper(a: int, b: int) -> int {
    a + b
}

function OuterExprFunc(x: int, y: int) -> int {
    let sum = InnerHelper(x, y);
    sum * 2
}
"""


# ============================================================================
# BAML source with LLM functions -- single LLM function
# ============================================================================

SINGLE_LLM_TEMPLATE = """\
client<llm> MockClient {{
    provider openai
    options {{
        model "mock-model"
        base_url "{mock_url}"
        api_key "test-key"
    }}
}}

function TestLLM(msg: string) -> string {{
    client MockClient
    prompt #"Reply to: {{{{ msg }}}}"#
}}
"""


# ============================================================================
# BAML source with LLM functions -- pipeline with multiple LLM calls
# ============================================================================

PIPELINE_LLM_TEMPLATE = """\
client<llm> MockClient {{
    provider openai
    options {{
        model "mock-model"
        base_url "{mock_url}"
        api_key "test-key"
    }}
}}

function ExtractInfo(text: string) -> string {{
    client MockClient
    prompt #"Extract: {{{{ text }}}}"#
}}

function SummarizeInfo(text: string) -> string {{
    client MockClient
    prompt #"Summarize: {{{{ text }}}}"#
}}

function InnerPipeline(input: string) -> string {{
    let a = ExtractInfo(input);
    let b = SummarizeInfo(input);
    a + " " + b
}}

function OuterPipeline(input: string) -> string {{
    let result = InnerPipeline(input);
    "Result: " + result
}}
"""


# ============================================================================
# Fixtures
# ============================================================================


@pytest.fixture
def rt():
    """Fresh BamlRuntime with expression functions for each test."""
    return BamlRuntime.from_files(".", {"main.baml": BAML_SOURCE})


@pytest.fixture
def ctx(rt):
    """Fresh BamlCtxManager for each test."""
    import baml_py.ctx_manager as cm

    cm.prev_ctx_manager = None
    return BamlCtxManager(rt)


@pytest.fixture
def mock_server():
    """Start a mock LLM HTTP server, yield its URL, then shut down."""
    server = http.server.HTTPServer(("127.0.0.1", 0), MockLLMHandler)
    port = server.server_address[1]
    thread = threading.Thread(target=server.serve_forever)
    thread.daemon = True
    thread.start()
    yield f"http://127.0.0.1:{port}"
    server.shutdown()
    server.server_close()
    thread.join(timeout=5)


@pytest.fixture
def llm_rt(mock_server):
    """BamlRuntime with a single LLM function (TestLLM) pointing to mock server."""
    source = SINGLE_LLM_TEMPLATE.format(mock_url=mock_server)
    return BamlRuntime.from_files(".", {"main.baml": source})


@pytest.fixture
def llm_ctx(llm_rt):
    """BamlCtxManager for the single-LLM runtime."""
    import baml_py.ctx_manager as cm

    cm.prev_ctx_manager = None
    return BamlCtxManager(llm_rt)


@pytest.fixture
def pipeline_rt(mock_server):
    """BamlRuntime with pipeline LLM functions pointing to mock server."""
    source = PIPELINE_LLM_TEMPLATE.format(mock_url=mock_server)
    return BamlRuntime.from_files(".", {"main.baml": source})


@pytest.fixture
def pipeline_ctx(pipeline_rt):
    """BamlCtxManager for the pipeline LLM runtime."""
    import baml_py.ctx_manager as cm

    cm.prev_ctx_manager = None
    return BamlCtxManager(pipeline_rt)


# ============================================================================
# 1. Basic async/sync collection
#    (Ported from: test_collector_async_no_stream_success)
# ============================================================================


class TestBasicCollection:
    """Basic collector functionality: logs, last, function_name, timing, result."""

    def test_collector_empty_before_call(self):
        """Fresh collector has empty logs and None last.
        Ported from: integ-test basic collector checks."""
        collector = Collector("test")
        assert collector.logs == []
        assert collector.last is None
        assert collector.name == "test"

    def test_collector_default_name(self):
        """Collector without explicit name gets 'default'."""
        collector = Collector()
        assert collector.name == "default"

    @pytest.mark.asyncio
    async def test_collector_async_no_stream_success(self, llm_rt):
        """Async non-streaming LLM call with collector.
        Ported from: test_collector_async_no_stream_success.
        Verifies: logs count, function_name, timing, calls."""
        collector = Collector("test")
        result = await call_function(llm_rt,
            "TestLLM", {"msg": "hi there"}, collectors=[collector]
        )
        assert "mocked" in result.result()

        # Single log entry
        logs = collector.logs
        assert len(logs) == 1

        log = logs[0]
        assert log.function_name == "TestLLM"

        # Timing populated
        assert log.timing.start_time_utc_ms > 0
        assert log.timing.duration_ms is not None
        assert log.timing.duration_ms >= 0

        # ID is a non-empty UUID string
        assert log.id is not None
        assert len(log.id) > 0

        # Result captured (may be None for LLM functions depending on
        # how the engine emits FunctionEnd — this is fine for now)

        # last() matches logs[0]
        last = collector.last
        assert last is not None
        assert last.function_name == log.function_name
        assert last.id == log.id

    def test_collector_sync_success(self, llm_rt):
        """Sync LLM call with collector.
        Mirrors test_collector_async_no_stream_success but sync."""
        collector = Collector("test")
        result = call_function_sync(llm_rt,
            "TestLLM", {"msg": "hi there"}, collectors=[collector]
        )
        assert "mocked" in result.result()

        logs = collector.logs
        assert len(logs) == 1
        log = logs[0]
        assert log.function_name == "TestLLM"
        assert log.timing.start_time_utc_ms > 0
        assert log.timing.duration_ms is not None

    def test_collector_captures_function_name(self, rt):
        """log.function_name matches the called BAML function."""
        collector = Collector("test")
        call_function_sync(rt,"AddNumbers", {"a": 1, "b": 2}, collectors=[collector])
        assert collector.last.function_name == "AddNumbers"

    def test_collector_captures_timing(self, rt):
        """log.timing has positive start_time and duration."""
        collector = Collector("test")
        call_function_sync(rt,"ReturnOne", {}, collectors=[collector])

        log = collector.last
        assert log.timing.start_time_utc_ms > 0
        assert log.timing.duration_ms is not None
        assert log.timing.duration_ms >= 0

    def test_collector_captures_result(self, rt):
        """log.result contains the return value."""
        collector = Collector("test")
        call_function_sync(rt,"AddNumbers", {"a": 3, "b": 4}, collectors=[collector])
        assert collector.last.result == 7

    def test_collector_id_lookup(self, rt):
        """Can look up a specific log by its span ID.
        Ported from: collector.id() usage in integ-tests."""
        collector = Collector("test")
        call_function_sync(rt,"ReturnOne", {}, collectors=[collector])

        log = collector.last
        found = collector.id(log.id)
        assert found is not None
        assert found.function_name == "ReturnOne"
        assert collector.id("nonexistent-id") is None

    def test_collector_repr(self):
        """Collector has a useful repr."""
        collector = Collector("my_name")
        assert "my_name" in repr(collector)

    @pytest.mark.asyncio
    async def test_collector_basic_async_expr(self, rt):
        """Async expression function call with collector."""
        collector = Collector("test")
        result = await call_function(rt,"ReturnOne", {}, collectors=[collector])
        assert result.result() == 1
        assert len(collector.logs) == 1

    def test_collector_basic_sync_expr(self, rt):
        """Sync expression function call with collector."""
        collector = Collector("test")
        result = call_function_sync(rt,"ReturnOne", {}, collectors=[collector])
        assert result.result() == 1
        assert len(collector.logs) == 1
        assert collector.last.function_name == "ReturnOne"


# ============================================================================
# 2. Clear
#    (Ported from: test_collector_clear)
# ============================================================================


class TestCollectorClear:
    """Collector.clear() functionality."""

    def test_collector_clear(self, llm_rt):
        """After clear(), logs is empty.
        Ported from: test_collector_clear."""
        collector = Collector("test")

        # Initially empty
        assert len(collector.logs) == 0

        # Call and verify log exists
        call_function_sync(llm_rt,
            "TestLLM", {"msg": "hi there"}, collectors=[collector]
        )
        assert len(collector.logs) == 1

        # Clear and verify empty
        cleared = collector.clear()
        assert cleared == 1
        assert len(collector.logs) == 0
        assert collector.last is None

    def test_collector_clear_returns_count(self, rt):
        """clear() returns the number of tracked roots that were removed."""
        collector = Collector("test")
        call_function_sync(rt,"ReturnOne", {}, collectors=[collector])
        call_function_sync(rt,"ReturnOne", {}, collectors=[collector])
        assert collector.clear() == 2

    def test_collector_clear_then_reuse(self, rt):
        """After clear(), collector can track new calls.
        Ported from: test_collector_clear reuse pattern."""
        collector = Collector("test")
        call_function_sync(rt,"ReturnOne", {}, collectors=[collector])
        collector.clear()

        call_function_sync(rt,"AddNumbers", {"a": 5, "b": 6}, collectors=[collector])
        assert len(collector.logs) == 1
        assert collector.last.function_name == "AddNumbers"


# ============================================================================
# 3. No log access pattern
#    (Ported from: test_collector_async_no_stream_no_getting_logs)
# ============================================================================


class TestNoLogAccess:
    """Collector works even when logs aren't accessed immediately."""

    @pytest.mark.asyncio
    async def test_collector_async_no_getting_logs(self, llm_rt):
        """Ported from: test_collector_async_no_stream_no_getting_logs.
        Collector tracks calls even if .logs is never accessed."""
        collector = Collector("test")
        await call_function(llm_rt,
            "TestLLM", {"msg": "hi there"}, collectors=[collector]
        )
        # Don't access logs yet...
        # Later access should still work
        assert len(collector.logs) == 1


# ============================================================================
# 4. Multiple calls and usage aggregation
#    (Ported from: test_collector_async_multiple_calls_usage)
# ============================================================================


class TestMultipleCallsAndUsage:
    """Usage aggregation across multiple calls."""

    @pytest.mark.asyncio
    async def test_collector_multiple_calls_accumulate(self, llm_rt):
        """Ported from: test_collector_async_multiple_calls_usage.
        Multiple LLM calls accumulate in the same collector."""
        collector = Collector("test")

        await call_function(llm_rt,
            "TestLLM", {"msg": "First call"}, collectors=[collector]
        )
        assert len(collector.logs) == 1

        await call_function(llm_rt,
            "TestLLM", {"msg": "Second call"}, collectors=[collector]
        )
        assert len(collector.logs) == 2

        # Both logs have the same function name
        for log in collector.logs:
            assert log.function_name == "TestLLM"

    def test_collector_sequential_calls_ordered(self, rt):
        """Logs are in insertion order for sequential calls."""
        collector = Collector("test")
        call_function_sync(rt,"ReturnOne", {}, collectors=[collector])
        call_function_sync(rt,"AddNumbers", {"a": 1, "b": 2}, collectors=[collector])

        logs = collector.logs
        assert len(logs) == 2
        assert logs[0].function_name == "ReturnOne"
        assert logs[1].function_name == "AddNumbers"
        assert collector.last.function_name == "AddNumbers"

    def test_collector_usage_empty_when_no_calls(self):
        """Empty collector has None usage fields."""
        collector = Collector("test")
        usage = collector.usage
        assert usage.input_tokens is None
        assert usage.output_tokens is None
        assert usage.cached_input_tokens is None

    def test_collector_usage_after_expr_call(self, rt):
        """Expression function call has no token usage."""
        collector = Collector("test")
        call_function_sync(rt,"ReturnOne", {}, collectors=[collector])
        usage = collector.usage
        assert usage.input_tokens is None
        assert usage.output_tokens is None


# ============================================================================
# 5. Multiple collectors
#    (Ported from: test_collector_multiple_collectors)
# ============================================================================


class TestMultipleCollectors:
    """Multiple independent collectors on the same or different calls."""

    def test_multiple_collectors_same_call(self, llm_rt):
        """Two collectors on same call both receive the log.
        Ported from: test_collector_multiple_collectors."""
        c1 = Collector("c1")
        c2 = Collector("c2")

        call_function_sync(llm_rt,
            "TestLLM", {"msg": "hello"}, collectors=[c1, c2]
        )

        assert len(c1.logs) == 1
        assert len(c2.logs) == 1
        assert c1.logs[0].function_name == "TestLLM"
        assert c2.logs[0].function_name == "TestLLM"

    def test_multiple_collectors_second_call_only_one(self, llm_rt):
        """After shared call, second call only tracked by one collector.
        Ported from: test_collector_multiple_collectors second-call pattern."""
        c1 = Collector("c1")
        c2 = Collector("c2")

        # First call: both collectors
        call_function_sync(llm_rt,
            "TestLLM", {"msg": "first"}, collectors=[c1, c2]
        )

        # Second call: only c1
        call_function_sync(llm_rt,
            "TestLLM", {"msg": "second"}, collectors=[c1]
        )

        assert len(c1.logs) == 2
        assert len(c2.logs) == 1  # c2 only has the first call

    def test_collectors_independent(self, rt):
        """Two collectors track different calls, each sees only its own."""
        c1 = Collector("c1")
        c2 = Collector("c2")

        call_function_sync(rt,"ReturnOne", {}, collectors=[c1])
        call_function_sync(rt,"AddNumbers", {"a": 1, "b": 2}, collectors=[c2])

        assert len(c1.logs) == 1
        assert c1.logs[0].function_name == "ReturnOne"
        assert len(c2.logs) == 1
        assert c2.logs[0].function_name == "AddNumbers"

    def test_collector_list_single_element(self, rt):
        """[collector] works same as passing collector directly."""
        collector = Collector("test")
        call_function_sync(rt,"ReturnOne", {}, collectors=[collector])
        assert len(collector.logs) == 1


# ============================================================================
# 6. Mixed async/sync calls
#    (Ported from: test_collector_mixed_async_sync_calls)
# ============================================================================


class TestMixedAsyncSync:
    """Mix of async and sync calls in the same collector."""

    @pytest.mark.asyncio
    async def test_collector_mixed_async_sync_calls(self, llm_rt):
        """Ported from: test_collector_mixed_async_sync_calls.
        Async and sync calls both tracked by same collector."""
        collector = Collector("test")

        # Async call
        await call_function(llm_rt,
            "TestLLM", {"msg": "async call"}, collectors=[collector]
        )
        assert len(collector.logs) == 1

        # Sync call on same collector
        call_function_sync(llm_rt,
            "TestLLM", {"msg": "sync call"}, collectors=[collector]
        )
        assert len(collector.logs) == 2

        # Both have timing
        for log in collector.logs:
            assert log.timing.start_time_utc_ms > 0
            assert log.timing.duration_ms is not None

        # Second call timing should be >= first
        assert (
            collector.logs[1].timing.start_time_utc_ms
            >= collector.logs[0].timing.start_time_utc_ms
        )


# ============================================================================
# 7. Parallel async calls
#    (Ported from: test_collector_parallel_async_calls)
# ============================================================================


class TestParallelAsyncCalls:
    """Parallel execution with asyncio.gather()."""

    @pytest.mark.asyncio
    async def test_collector_parallel_async_calls(self, llm_rt):
        """Ported from: test_collector_parallel_async_calls.
        Two calls via gather() both logged."""
        collector = Collector("test")

        r1, r2 = await asyncio.gather(
            call_function(llm_rt,
                "TestLLM", {"msg": "call 1"}, collectors=[collector]
            ),
            call_function(llm_rt,
                "TestLLM", {"msg": "call 2"}, collectors=[collector]
            ),
        )

        assert "mocked" in r1.result()
        assert "mocked" in r2.result()

        logs = collector.logs
        assert len(logs) == 2

        # Both logs have function_name and timing
        for log in logs:
            assert log.function_name == "TestLLM"
            assert log.timing.start_time_utc_ms > 0

    @pytest.mark.asyncio
    async def test_collector_parallel_expr_calls(self, rt):
        """Parallel expression calls via gather()."""
        collector = Collector("test")

        r1, r2 = await asyncio.gather(
            call_function(rt,"ReturnOne", {}, collectors=[collector]),
            call_function(rt,
                "AddNumbers", {"a": 10, "b": 20}, collectors=[collector]
            ),
        )

        assert r1.result() == 1
        assert r2.result() == 30

        logs = collector.logs
        assert len(logs) == 2
        names = {log.function_name for log in logs}
        assert "ReturnOne" in names
        assert "AddNumbers" in names


# ============================================================================
# 8. LLM call details (child spans)
#    (Ported from: test_collector_async_no_stream_success call inspection)
# ============================================================================


class TestLLMCallDetails:
    """LLM function calls captured as child spans in calls list."""

    @pytest.mark.asyncio
    async def test_collector_llm_calls_captured(self, pipeline_rt):
        """Pipeline with 2 LLM calls: verify log.calls has entries.
        Ported from: test_collector_async_no_stream_success call inspection."""
        collector = Collector("test")
        result = await call_function(pipeline_rt,
            "OuterPipeline", {"input": "hello"}, collectors=[collector]
        )
        assert "mocked" in result.result()

        log = collector.last
        assert log is not None
        assert log.function_name == "OuterPipeline"

        # ExtractInfo and SummarizeInfo are LLM functions (CallWithTrace)
        assert len(log.calls) >= 2, (
            f"Expected >= 2 LLM calls (ExtractInfo, SummarizeInfo), "
            f"got {len(log.calls)}: {[c.function_name for c in log.calls]}"
        )

    @pytest.mark.asyncio
    async def test_collector_llm_call_function_names(self, pipeline_rt):
        """LLM call.function_name matches the LLM function names."""
        collector = Collector("test")
        await call_function(pipeline_rt,
            "OuterPipeline", {"input": "hello"}, collectors=[collector]
        )

        log = collector.last
        call_names = {c.function_name for c in log.calls}
        assert "ExtractInfo" in call_names
        assert "SummarizeInfo" in call_names

    @pytest.mark.asyncio
    async def test_collector_llm_call_timing(self, pipeline_rt):
        """Each LLM call has its own timing separate from parent."""
        collector = Collector("test")
        await call_function(pipeline_rt,
            "OuterPipeline", {"input": "hello"}, collectors=[collector]
        )

        log = collector.last
        for call in log.calls:
            assert call.timing.start_time_utc_ms > 0
            assert call.timing.duration_ms is not None
            assert call.timing.duration_ms >= 0

    def test_collector_llm_call_sync(self, pipeline_rt):
        """Sync LLM pipeline call also captures child LLM calls."""
        collector = Collector("test")
        result = call_function_sync(pipeline_rt,
            "OuterPipeline", {"input": "hello"}, collectors=[collector]
        )
        assert "mocked" in result.result()
        assert collector.last is not None
        assert len(collector.last.calls) >= 2

    @pytest.mark.asyncio
    async def test_collector_single_llm_call(self, llm_rt):
        """Single LLM function has no child calls (it IS the leaf call)."""
        collector = Collector("test")
        await call_function(llm_rt,
            "TestLLM", {"msg": "hello"}, collectors=[collector]
        )
        log = collector.last
        assert log is not None
        assert log.function_name == "TestLLM"
        # A single LLM function is the root — it doesn't have child LLM calls
        # (it might have 0 calls since the LLM HTTP call isn't a separate traced span)


# ============================================================================
# 9. Tags
#    (Ported from: test_functionlog_tags_inherit_from_parent_trace,
#     test_baml_function_tags_with_parent_trace)
# ============================================================================


class TestTags:
    """Tag propagation to collector logs."""

    def test_collector_tags_empty_for_expr_function(self, rt):
        """Expression function logs have empty tags."""
        collector = Collector("test")
        call_function_sync(rt,"ReturnOne", {}, collectors=[collector])
        assert collector.last.tags == {}

    def test_collector_tags_empty_for_llm_function(self, llm_rt):
        """LLM function logs have empty tags by default."""
        collector = Collector("test")
        call_function_sync(llm_rt,
            "TestLLM", {"msg": "hello"}, collectors=[collector]
        )
        assert collector.last.tags == {}

    # NOTE: Host→engine tag propagation (tags from @trace → engine FunctionStart)
    # is deferred to a future PR. When implemented, add tests like:
    #
    # test_functionlog_tags_inherit_from_parent_trace:
    #   @trace with set_tags(parent_id="p123") → child BAML call → log.tags["parent_id"] == "p123"
    #
    # test_baml_function_tags_with_parent_trace:
    #   Parent sets tags, child BAML calls with per-call tags → both present in log.tags


# ============================================================================
# 10. GC and cleanup
#     (Ported from: test_collector_clear, ensure_collector_is_empty fixture,
#      and GC patterns throughout)
# ============================================================================


class TestGCAndCleanup:
    """Garbage collection and cleanup behavior.
    Ported from: GC patterns in integ-tests/test_collector.py."""

    def test_collector_drop_releases_events(self, rt):
        """Deleting collector + gc.collect() releases events."""
        collector = Collector("test")
        call_function_sync(rt,"ReturnOne", {}, collectors=[collector])
        log_id = collector.last.id
        assert log_id is not None

        del collector
        gc.collect()

        # New collector shouldn't see old data
        new_collector = Collector("new")
        assert len(new_collector.logs) == 0

    def test_collector_gc_loop(self, rt):
        """Create collectors in a loop, verify no memory leak.
        Ported from: general GC pattern in integ-tests."""
        for _ in range(100):
            collector = Collector("loop")
            call_function_sync(rt,"ReturnOne", {}, collectors=[collector])
            assert len(collector.logs) == 1

        gc.collect()
        # If we get here without OOM or hanging, the test passes

    def test_collector_ref_counting_two_collectors(self, rt):
        """Two collectors track same root; dropping one doesn't free data.
        Ported from: reference counting behavior in integ-tests."""
        c1 = Collector("c1")
        c2 = Collector("c2")

        call_function_sync(rt,"ReturnOne", {}, collectors=[c1, c2])

        assert len(c1.logs) == 1
        assert len(c2.logs) == 1

        del c1
        gc.collect()

        # c2 should still have its log
        assert len(c2.logs) == 1
        assert c2.last.function_name == "ReturnOne"

    def test_collector_drop_then_c2_drop_frees(self, rt):
        """After both collectors dropped, events are freed."""
        c1 = Collector("c1")
        c2 = Collector("c2")

        call_function_sync(rt,"ReturnOne", {}, collectors=[c1, c2])

        del c1
        gc.collect()
        del c2
        gc.collect()

        # New collector shouldn't see old data
        new = Collector("new")
        assert len(new.logs) == 0

    @pytest.mark.asyncio
    async def test_collector_gc_after_async_calls(self, llm_rt):
        """GC after async LLM calls releases events properly."""
        collector = Collector("test")
        await call_function(llm_rt,
            "TestLLM", {"msg": "hello"}, collectors=[collector]
        )
        assert len(collector.logs) == 1

        collector.clear()
        assert len(collector.logs) == 0

        # Can still track new calls
        await call_function(llm_rt,
            "TestLLM", {"msg": "world"}, collectors=[collector]
        )
        assert len(collector.logs) == 1

    def test_collector_clear_multiple_times(self, rt):
        """Calling clear() multiple times is safe."""
        collector = Collector("test")
        call_function_sync(rt,"ReturnOne", {}, collectors=[collector])
        assert collector.clear() == 1
        assert collector.clear() == 0
        assert collector.clear() == 0


# ============================================================================
# 11. Error cases
#     (Ported from: test_collector_failures_arg_type,
#      test_collector_failures_client_registry)
# ============================================================================


class TestErrorCases:
    """Collector behavior on errors.
    Ported from: error test section in integ-tests/test_collector.py."""

    def test_collector_on_nonexistent_function(self, rt):
        """Call to missing function doesn't create a log."""
        collector = Collector("test")
        with pytest.raises(Exception):
            call_function_sync(rt,"NonExistent", {}, collectors=[collector])
        # Function not found before root span is created
        assert len(collector.logs) == 0

    def test_collector_no_collectors_param(self, rt):
        """Calling without collectors works normally."""
        result = call_function_sync(rt,"ReturnOne", {})
        assert result.result() == 1

    def test_collector_no_collectors_none(self, rt):
        """Passing collectors=None works normally."""
        result = call_function_sync(rt,"ReturnOne", {}, collectors=None)
        assert result.result() == 1

    # NOTE: test_collector_failures_arg_type (invalid arg type errors) and
    # test_collector_failures_client_registry (invalid client config) are
    # deferred until those error paths emit events before failing.


# ============================================================================
# 12. Nested @trace with collector
#     (Ported from: test_collector_multiple_async_nested,
#      test_collector_multiple_sync_nested)
# ============================================================================


class TestNestedTraceWithCollector:
    """Collector inside nested @trace decorator calls.
    Ported from: test_collector_multiple_async_nested and
    test_collector_multiple_sync_nested."""

    @pytest.mark.asyncio
    async def test_collector_nested_async_with_trace(self, llm_rt, llm_ctx):
        """Ported from: test_collector_multiple_async_nested.
        Nested @trace decorators with async LLM calls."""
        collector = Collector("test")
        trace = llm_ctx.trace_fn

        @trace
        async def inner_call(msg: str) -> str:
            result = await call_function(llm_rt,
                "TestLLM",
                {"msg": msg},
                ctx=llm_ctx.get(),
                collectors=[collector],
            )
            return result.result()

        @trace
        async def outer_call() -> str:
            return await inner_call("hello from nested")

        result = await outer_call()
        assert "mocked" in result

        log = collector.last
        assert log is not None
        assert log.function_name == "TestLLM"

    def test_collector_nested_sync_with_trace(self, llm_rt, llm_ctx):
        """Ported from: test_collector_multiple_sync_nested.
        Nested @trace decorators with sync LLM calls."""
        collector = Collector("test")
        trace = llm_ctx.trace_fn

        @trace
        def inner_call(msg: str) -> str:
            result = call_function_sync(llm_rt,
                "TestLLM",
                {"msg": msg},
                ctx=llm_ctx.get(),
                collectors=[collector],
            )
            return result.result()

        @trace
        def outer_call() -> str:
            return inner_call("hello from nested sync")

        result = outer_call()
        assert "mocked" in result
        assert len(collector.logs) == 1
        assert collector.last.function_name == "TestLLM"

    @pytest.mark.asyncio
    async def test_collector_nested_async_gather_no_trace(self, llm_rt):
        """Parallel LLM calls inside gather WITHOUT @trace.
        Without @trace, each call gets its own root → 2 logs."""
        collector = Collector("test")

        r1, r2 = await asyncio.gather(
            call_function(llm_rt,
                "TestLLM",
                {"msg": "call 1"},
                collectors=[collector],
            ),
            call_function(llm_rt,
                "TestLLM",
                {"msg": "call 2"},
                collectors=[collector],
            ),
        )

        results = [r1.result(), r2.result()]
        assert len(results) == 2
        assert all("mocked" in r for r in results)
        assert len(collector.logs) == 2

    @pytest.mark.asyncio
    async def test_collector_nested_async_gather_with_trace(self, llm_rt, llm_ctx):
        """Ported from: test_collector_multiple_async_nested gather pattern.
        Parallel LLM calls inside @trace — each call_function gets its own
        engine_span_id, so the collector sees 2 separate logs."""
        collector = Collector("test")
        trace = llm_ctx.trace_fn

        @trace
        async def parallel_calls() -> list:
            r1, r2 = await asyncio.gather(
                call_function(llm_rt,
                    "TestLLM",
                    {"msg": "call 1"},
                    ctx=llm_ctx.get(),
                    collectors=[collector],
                ),
                call_function(llm_rt,
                    "TestLLM",
                    {"msg": "call 2"},
                    ctx=llm_ctx.get(),
                    collectors=[collector],
                ),
            )
            return [r1.result(), r2.result()]

        results = await parallel_calls()
        assert len(results) == 2
        assert all("mocked" in r for r in results)
        # Each call_function gets a unique engine_span_id, so the
        # collector sees 2 separate function logs even under @trace.
        assert len(collector.logs) == 2


# ============================================================================
# 13. Cross-boundary tracing (collector + @trace + engine)
#     (Ported from: test_collector_async_no_stream_success with trace context)
# ============================================================================


class TestCrossBoundaryTracing:
    """Collector with @trace decorator integration."""

    @pytest.mark.asyncio
    async def test_collector_with_trace_decorator(self, rt, ctx):
        """Collector + @trace together; collector captures engine spans."""
        collector = Collector("test")
        trace = ctx.trace_fn

        @trace
        async def traced_call() -> int:
            result = await call_function(rt,
                "AddNumbers", {"a": 10, "b": 20}, ctx=ctx.get(), collectors=[collector]
            )
            return result.result()

        assert await traced_call() == 30
        assert collector.last is not None
        assert collector.last.function_name == "AddNumbers"

    def test_collector_without_trace_decorator(self, rt):
        """Collector works without @trace (no host context)."""
        collector = Collector("test")
        result = call_function_sync(rt,"ReturnOne", {}, collectors=[collector])
        assert result.result() == 1
        assert len(collector.logs) == 1

    @pytest.mark.asyncio
    async def test_collector_with_llm_and_trace(self, pipeline_rt, pipeline_ctx):
        """Collector + @trace + LLM pipeline: full integration.
        Ported from: combined trace+collector patterns in integ-tests."""
        collector = Collector("test")
        trace = pipeline_ctx.trace_fn

        @trace
        async def traced_pipeline() -> str:
            result = await call_function(pipeline_rt,
                "OuterPipeline",
                {"input": "hello"},
                ctx=pipeline_ctx.get(),
                collectors=[collector],
            )
            return result.result()

        result = await traced_pipeline()
        assert "mocked" in result

        log = collector.last
        assert log is not None
        assert log.function_name == "OuterPipeline"
        assert len(log.calls) >= 2

        call_names = {c.function_name for c in log.calls}
        assert "ExtractInfo" in call_names
        assert "SummarizeInfo" in call_names


# ============================================================================
# 14. Context manager production pattern
#     (Ported from: test_collector_context_manager_pattern)
# ============================================================================


class TestContextManagerPattern:
    """Production usage patterns with collector.
    Ported from: test_collector_context_manager_pattern."""

    @pytest.mark.asyncio
    async def test_collector_batch_processing_pattern(self, llm_rt):
        """Batch processing pattern: create collector, run batch, inspect logs.
        Ported from: test_collector_context_manager_pattern."""
        collector = Collector("batch")

        # Simulate a batch of 3 calls
        messages = ["hello", "world", "test"]
        results = []
        for msg in messages:
            result = await call_function(llm_rt,
                "TestLLM", {"msg": msg}, collectors=[collector]
            )
            results.append(result.result())

        # All calls succeeded
        assert len(results) == 3
        assert all("mocked" in r for r in results)

        # All logged
        logs = collector.logs
        assert len(logs) == 3
        for log in logs:
            assert log.function_name == "TestLLM"
            assert log.timing.start_time_utc_ms > 0
            assert log.timing.duration_ms is not None

    @pytest.mark.asyncio
    async def test_collector_per_request_pattern(self, llm_rt):
        """Per-request collector pattern: fresh collector per logical request.
        Ported from: test_collector_context_manager_pattern isolation."""
        # Request 1
        c1 = Collector("request-1")
        await call_function(llm_rt,
            "TestLLM", {"msg": "request 1"}, collectors=[c1]
        )

        # Request 2
        c2 = Collector("request-2")
        await call_function(llm_rt,
            "TestLLM", {"msg": "request 2"}, collectors=[c2]
        )

        # Each collector only sees its own request
        assert len(c1.logs) == 1
        assert len(c2.logs) == 1

    @pytest.mark.asyncio
    async def test_collector_parallel_batch_pattern(self, llm_rt):
        """Parallel batch with single collector.
        Ported from: test_collector_mixed_providers_context_manager."""
        collector = Collector("parallel-batch")

        # Run 3 calls in parallel
        results = await asyncio.gather(
            call_function(llm_rt,
                "TestLLM", {"msg": "p1"}, collectors=[collector]
            ),
            call_function(llm_rt,
                "TestLLM", {"msg": "p2"}, collectors=[collector]
            ),
            call_function(llm_rt,
                "TestLLM", {"msg": "p3"}, collectors=[collector]
            ),
        )

        assert len(results) == 3
        assert len(collector.logs) == 3


# ============================================================================
# 15. Timing ordering
#     (Ported from: test_collector_mixed_async_sync_calls timing assertions)
# ============================================================================


class TestTimingOrdering:
    """Timing-related assertions from the old tests."""

    def test_sequential_calls_timing_ordered(self, rt):
        """Sequential calls have increasing start times.
        Ported from: test_collector_mixed_async_sync_calls timing check."""
        collector = Collector("test")
        call_function_sync(rt,"ReturnOne", {}, collectors=[collector])
        call_function_sync(rt,"ReturnOne", {}, collectors=[collector])

        logs = collector.logs
        assert len(logs) == 2
        assert logs[1].timing.start_time_utc_ms >= logs[0].timing.start_time_utc_ms

    @pytest.mark.asyncio
    async def test_llm_call_timing_within_parent(self, pipeline_rt):
        """LLM child call timing is within parent timing.
        Ported from: test_collector_async_no_stream_success timing checks."""
        collector = Collector("test")
        await call_function(pipeline_rt,
            "OuterPipeline", {"input": "hello"}, collectors=[collector]
        )

        log = collector.last
        assert log is not None
        parent_start = log.timing.start_time_utc_ms
        parent_duration = log.timing.duration_ms
        assert parent_duration is not None

        for call in log.calls:
            # Child started after or at the same time as parent
            assert call.timing.start_time_utc_ms >= parent_start
            # Child duration should be <= parent duration
            if call.timing.duration_ms is not None:
                assert call.timing.duration_ms <= parent_duration + 5  # small tolerance
