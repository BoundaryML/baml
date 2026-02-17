"""
Tracing tests for bridge_python.

Tests the @trace decorator, context isolation (async/threads), tag propagation,
flush, and log event callbacks through the full Python → PyO3 → bridge_cffi →
bex_engine pipeline.

Modeled after integ-tests/python/tests/test_tracing.py.  We instantiate
BamlRuntime + BamlCtxManager directly (the same way the codegen would).
Most tests use pure expression BAML functions; TEST 15 uses LLM functions
backed by a local mock HTTP server.

Trace events (function_start / function_end) are expected to be written to disk
as JSONL in the same format as the integ-tests.  Tests verify these events via
a TraceFileReader helper class.

Event-recording tests verify that trace events (function_start / function_end)
are properly written to disk as JSONL via the global EventStore publisher.

Run with:
    cd baml_language/crates/bridge_python
    uv run maturin develop --uv
    uv run pytest tests/test_tracing.py -v
"""

import asyncio
import concurrent.futures
import http.server
import json
import os
import random
import tempfile
import threading
import time
import typing
from typing import Any, Dict, List, Optional

import pytest

from baml_py import BamlRuntime, BamlCtxManager, FunctionResult, HostSpanManager, call_function, call_function_sync
from conftest import MockLLMHandler


# ============================================================================
# BAML source — pure expression functions (no LLM calls)
# ============================================================================

BAML_SOURCE = """\
function ReturnOne() -> int {
    1
}

function ReturnNumber(n: int) -> int {
    n
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
# BAML source with LLM functions (used by TEST 15)
# ============================================================================

LLM_BAML_TEMPLATE = """\
client<llm> MockClient {
    provider openai
    options {
        model "mock-model"
        base_url "__MOCK_URL__"
        api_key "test-key"
    }
}

function ExtractInfo(text: string) -> string {
    client MockClient
    prompt #"Extract: {{ text }}"#
}

function SummarizeInfo(text: string) -> string {
    client MockClient
    prompt #"Summarize: {{ text }}"#
}

function InnerPipeline(input: string) -> string {
    let a = ExtractInfo(input);
    let b = SummarizeInfo(input);
    a + " " + b
}

function OuterPipeline(input: string) -> string {
    let result = InnerPipeline(input);
    "Result: " + result
}
"""


# ============================================================================
# Trace Event Models (same format as integ-tests)
# ============================================================================


class TraceEvent:
    """Represents a single trace event from the log file.

    JSONL format per line:
    {
        "call_id": "...",
        "function_event_id": "...",
        "call_stack": ["id1", "id2"],
        "timestamp_epoch_ms": 1234567890,
        "content": {
            "type": "function_start" | "function_end",
            "data": {
                "function_display_name": "my_function",
                "args": [...],
                "tags": {...},
                ...
            }
        }
    }
    """

    def __init__(self, raw: Dict[str, Any]):
        self.call_id: str = raw["call_id"]
        self.function_event_id: str = raw["function_event_id"]
        self.call_stack: List[str] = raw["call_stack"]
        self.timestamp_epoch_ms: int = raw["timestamp_epoch_ms"]
        self.content: Dict[str, Any] = raw["content"]

    @property
    def is_function_start(self) -> bool:
        return self.content["type"] == "function_start"

    @property
    def is_function_end(self) -> bool:
        return self.content["type"] == "function_end"

    @property
    def is_set_tags(self) -> bool:
        return self.content["type"] == "intermediate" and "SetTags" in self.content.get(
            "data", {}
        )

    @property
    def function_name(self) -> str:
        return self.content.get("data", {}).get("function_display_name", "")

    @property
    def tags(self) -> Dict[str, str]:
        if self.is_function_start:
            return self.content.get("data", {}).get("tags", {})
        return {}

    @property
    def set_tags(self) -> Dict[str, str]:
        if self.is_set_tags:
            return self.content.get("data", {}).get("SetTags", {})
        return {}

    @property
    def parent_id(self) -> Optional[str]:
        if len(self.call_stack) >= 2:
            return self.call_stack[-2]
        return None

    @property
    def root_id(self) -> str:
        return self.call_stack[0]

    @property
    def depth(self) -> int:
        return len(self.call_stack)

    def is_root(self) -> bool:
        return len(self.call_stack) == 1 and self.call_stack[0] == self.call_id

    def is_child_of(self, parent_id: str) -> bool:
        return self.parent_id == parent_id

    def is_descendant_of(self, ancestor_id: str) -> bool:
        return ancestor_id in self.call_stack[:-1]


# ============================================================================
# Trace File Reader
# ============================================================================


class TraceFileReader:
    """Helper to read and parse trace JSONL files.

    Same interface as integ-tests/python/tests/test_tracing.py::TraceFileReader.
    """

    def __init__(self, trace_file_path: str):
        self.trace_file_path = trace_file_path
        self._events: Optional[List[TraceEvent]] = None

    def load_events(self) -> List[TraceEvent]:
        if self._events is not None:
            return self._events

        events = []
        if not os.path.exists(self.trace_file_path):
            return events

        with open(self.trace_file_path, "r") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    raw = json.loads(line)
                    events.append(TraceEvent(raw))
                except (json.JSONDecodeError, KeyError) as e:
                    print(f"Warning: Failed to parse trace event: {e}")
                    continue

        self._events = events
        return events

    def get_function_starts(self) -> List[TraceEvent]:
        return [e for e in self.load_events() if e.is_function_start]

    def get_function_ends(self) -> List[TraceEvent]:
        return [e for e in self.load_events() if e.is_function_end]

    def count_events(self) -> Dict[str, int]:
        events = self.load_events()
        return {
            "function_start": sum(1 for e in events if e.is_function_start),
            "function_end": sum(1 for e in events if e.is_function_end),
        }

    def find_by_function_name(
        self, name: str, event_type: str = "function_start"
    ) -> List[TraceEvent]:
        events = (
            self.get_function_starts()
            if event_type == "function_start"
            else self.get_function_ends()
        )
        return [e for e in events if e.function_name == name]

    def find_root(self, function_name: Optional[str] = None) -> Optional[TraceEvent]:
        for event in self.get_function_starts():
            if event.is_root():
                if function_name is None or event.function_name == function_name:
                    return event
        return None

    def find_children(
        self, parent_id: str, function_name: Optional[str] = None
    ) -> List[TraceEvent]:
        children = [e for e in self.get_function_starts() if e.is_child_of(parent_id)]
        if function_name:
            children = [c for c in children if c.function_name == function_name]
        return children

    def find_descendants(self, ancestor_id: str) -> List[TraceEvent]:
        return [
            e for e in self.get_function_starts() if e.is_descendant_of(ancestor_id)
        ]

    def verify_parent_child(self, parent: TraceEvent, child: TraceEvent) -> None:
        assert child.call_stack == [*parent.call_stack, child.call_id], (
            f"Expected child call_stack = {[*parent.call_stack, child.call_id]}, "
            f"got {child.call_stack}"
        )

    def get_set_tags_events(self, call_id: str) -> List[TraceEvent]:
        return [e for e in self.load_events() if e.call_id == call_id and e.is_set_tags]

    def get_merged_tags(self, event: TraceEvent) -> Dict[str, str]:
        merged = event.tags.copy()
        for st in self.get_set_tags_events(event.call_id):
            merged.update(st.set_tags)
        return merged

    def verify_tags(self, event: TraceEvent, expected_tags: Dict[str, str]) -> None:
        merged = self.get_merged_tags(event)
        for key, value in expected_tags.items():
            assert key in merged and merged[key] == value, (
                f"Expected tag {key}={value} on {event.function_name}, "
                f"got tags={merged}"
            )

    def print_trace_hierarchy(
        self,
        show_ids: bool = False,
        show_tags: bool = False,
        show_depth: bool = False,
        indent_str: str = "  ",
    ) -> None:
        events = self.get_function_starts()
        events_sorted = sorted(events, key=lambda e: e.timestamp_epoch_ms)

        tree: Dict[str, List[TraceEvent]] = {}
        roots: List[TraceEvent] = []

        for event in events_sorted:
            if event.is_root():
                roots.append(event)
            else:
                pid = event.parent_id
                if pid not in tree:
                    tree[pid] = []
                tree[pid].append(event)

        def print_event(event: TraceEvent, indent_level: int = 0) -> None:
            indent = indent_str * indent_level
            parts = [indent, event.function_name]
            if show_ids:
                parts.append(f" [{event.call_id}]")
            if show_depth:
                parts.append(f" (depth: {event.depth})")
            print("".join(parts))

            if show_tags:
                merged = self.get_merged_tags(event)
                if merged:
                    tag_indent = indent_str * (indent_level + 1)
                    for key, value in merged.items():
                        print(f"{tag_indent}@{key}={value}")

            if event.call_id in tree:
                for child in tree[event.call_id]:
                    print_event(child, indent_level + 1)

        for root in roots:
            print_event(root)


# ============================================================================
# Helpers
# ============================================================================


def make_runtime() -> BamlRuntime:
    """Create a BamlRuntime from test BAML source."""
    return BamlRuntime.from_files(".", {"main.baml": BAML_SOURCE})


def make_ctx(rt: BamlRuntime) -> BamlCtxManager:
    """Create a BamlCtxManager wrapping a runtime.

    This is the same pattern the codegen uses:
        rt = BamlRuntime.from_files(...)
        ctx = BamlCtxManager(rt)
        trace = ctx.trace_fn
        set_tags = ctx.upsert_tags
        flush = ctx.flush
    """
    return BamlCtxManager(rt)


# ============================================================================
# Fixtures
# ============================================================================


@pytest.fixture
def trace_file(tmp_path):
    """Trace file path for event verification.

    Uses ``BAML_TRACE_FILE`` when set externally so you can inspect events
    after the test run (e.g. ``BAML_TRACE_FILE=debug.jsonl pytest -k test_name``).
    Falls back to a temp file when the env var is not set.
    """
    already_set = "BAML_TRACE_FILE" in os.environ
    if not already_set:
        os.environ["BAML_TRACE_FILE"] = str(tmp_path / "trace.jsonl")
    path = os.environ["BAML_TRACE_FILE"]
    if os.path.exists(path):
        os.remove(path)
    yield path
    if not already_set:
        os.environ.pop("BAML_TRACE_FILE", None)


@pytest.fixture
def rt():
    """Fresh BamlRuntime for each test."""
    return make_runtime()


@pytest.fixture
def ctx(rt):
    """Fresh BamlCtxManager for each test.

    NOTE: BamlCtxManager uses a module-level singleton (prev_ctx_manager).
    We need to reset it between tests to avoid cross-test contamination.
    """
    import baml_py.ctx_manager as cm

    cm.prev_ctx_manager = None
    return make_ctx(rt)


@pytest.fixture
def trace(ctx):
    """The @trace decorator from the CtxManager."""
    return ctx.trace_fn


@pytest.fixture
def set_tags(ctx):
    """The set_tags function from the CtxManager."""
    return ctx.upsert_tags


# ============================================================================
# TEST: @trace decorator basics — these should PASS even without event
# publishing, because the decorator wraps functions correctly.
# ============================================================================


class TestTraceDecoratorBasics:
    """Test that @trace wraps functions correctly (return values, exceptions, etc.)."""

    def test_trace_sync_returns_value(self, trace):
        """@trace on a sync function preserves the return value."""

        @trace
        def add(a: int, b: int) -> int:
            return a + b

        assert add(10, 32) == 42

    def test_trace_sync_preserves_name(self, trace):
        """@trace preserves __name__ via functools.wraps."""

        @trace
        def my_named_function():
            return 1

        assert my_named_function.__name__ == "my_named_function"

    def test_trace_sync_propagates_exception(self, trace):
        """@trace on a sync function propagates exceptions."""

        @trace
        def raises():
            raise ValueError("test error")

        with pytest.raises(ValueError, match="test error"):
            raises()

    @pytest.mark.asyncio
    async def test_trace_async_returns_value(self, trace):
        """@trace on an async function preserves the return value."""

        @trace
        async def add_async(a: int, b: int) -> int:
            return a + b

        assert await add_async(10, 32) == 42

    @pytest.mark.asyncio
    async def test_trace_async_preserves_name(self, trace):
        """@trace preserves __name__ for async functions."""

        @trace
        async def my_async_function():
            return 1

        assert my_async_function.__name__ == "my_async_function"

    @pytest.mark.asyncio
    async def test_trace_async_propagates_exception(self, trace):
        """@trace on an async function propagates exceptions."""

        @trace
        async def raises_async():
            raise ValueError("async error")

        with pytest.raises(ValueError, match="async error"):
            await raises_async()

    def test_trace_with_kwargs(self, trace):
        """@trace handles keyword arguments correctly."""

        @trace
        def greet(name: str, greeting: str = "hello") -> str:
            return f"{greeting} {name}"

        assert greet("world") == "hello world"
        assert greet("world", greeting="hi") == "hi world"
        assert greet(name="world", greeting="hey") == "hey world"

    @pytest.mark.asyncio
    async def test_trace_async_with_kwargs(self, trace):
        """@trace handles keyword arguments on async functions."""

        @trace
        async def greet_async(name: str, greeting: str = "hello") -> str:
            return f"{greeting} {name}"

        assert await greet_async("world") == "hello world"
        assert await greet_async("world", greeting="hi") == "hi world"

    def test_trace_no_args(self, trace):
        """@trace works with zero-argument functions."""

        @trace
        def no_args() -> int:
            return 42

        assert no_args() == 42

    def test_trace_with_baml_function(self, rt, trace):
        """@trace wrapping a function that calls a BAML function."""

        @trace
        def call_baml() -> int:
            result = call_function_sync(rt,"AddNumbers", {"a": 10, "b": 32})
            return result.result()

        assert call_baml() == 42

    @pytest.mark.asyncio
    async def test_trace_async_with_baml_function(self, rt, trace):
        """@trace wrapping an async function that calls a BAML function."""

        @trace
        async def call_baml_async() -> int:
            result = await call_function(rt,"AddNumbers", {"a": 100, "b": 200})
            return result.result()

        assert await call_baml_async() == 300


# ============================================================================
# TEST: Nested @trace calls — decorator should work for nested sync/async
# ============================================================================


class TestTraceNesting:
    """Test nested @trace decorator calls."""

    def test_nested_sync(self, trace):
        """Nested sync @trace calls work correctly."""

        @trace
        def inner(x: int) -> int:
            return x * 2

        @trace
        def outer(x: int) -> int:
            return inner(x) + 1

        assert outer(5) == 11

    @pytest.mark.asyncio
    async def test_nested_async(self, trace):
        """Nested async @trace calls work correctly."""

        @trace
        async def inner_async(x: int) -> int:
            return x * 2

        @trace
        async def outer_async(x: int) -> int:
            return await inner_async(x) + 1

        assert await outer_async(5) == 11

    def test_three_level_sync(self, trace):
        """Three-level sync nesting: root → parent → child."""

        @trace
        def grandchild(x: int) -> int:
            return x

        @trace
        def parent(x: int) -> int:
            return grandchild(x) + 1

        @trace
        def root(x: int) -> int:
            return parent(x) + 1

        assert root(10) == 12

    @pytest.mark.asyncio
    async def test_three_level_async(self, trace):
        """Three-level async nesting: root → parent → child."""

        @trace
        async def grandchild_async(x: int) -> int:
            await asyncio.sleep(0.01)
            return x

        @trace
        async def parent_async(x: int) -> int:
            return await grandchild_async(x) + 1

        @trace
        async def root_async(x: int) -> int:
            return await parent_async(x) + 1

        assert await root_async(10) == 12

    @pytest.mark.asyncio
    async def test_parallel_children_async(self, trace):
        """Root with parallel async children via asyncio.gather."""

        @trace
        async def child(task_id: int) -> int:
            await asyncio.sleep(0.01 + random.random() * 0.01)
            return task_id

        @trace
        async def root() -> typing.List[int]:
            results = await asyncio.gather(
                child(1),
                child(2),
                child(3),
            )
            return list(results)

        result = await root()
        assert sorted(result) == [1, 2, 3]

    @pytest.mark.asyncio
    async def test_sequential_children_async(self, trace):
        """Root with sequential async children."""

        @trace
        async def child_a(x: str) -> str:
            await asyncio.sleep(0.01)
            return f"a:{x}"

        @trace
        async def child_b(x: str) -> str:
            await asyncio.sleep(0.01)
            return f"b:{x}"

        @trace
        async def child_c(x: str) -> str:
            await asyncio.sleep(0.01)
            return f"c:{x}"

        @trace
        async def root(x: str) -> str:
            a = await child_a(x)
            b = await child_b(x)
            c = await child_c(x)
            return f"{a},{b},{c}"

        assert await root("test") == "a:test,b:test,c:test"

    def test_mixed_traced_and_untraced(self, trace):
        """Mixing @trace and non-traced functions."""

        @trace
        def traced_inner(x: int) -> int:
            return x * 2

        def untraced_middle(x: int) -> int:
            return traced_inner(x) + 1

        @trace
        def traced_outer(x: int) -> int:
            return untraced_middle(x) + 1

        assert traced_outer(5) == 12


# ============================================================================
# TEST: Thread pool with @trace
# ============================================================================


class TestTraceThreadPool:
    """Test @trace behavior with ThreadPoolExecutor."""

    def test_thread_pool_workers(self, trace):
        """Workers in a thread pool execute correctly."""

        @trace
        def worker(task_id: int) -> int:
            time.sleep(0.01 + random.random() * 0.01)
            return task_id

        with concurrent.futures.ThreadPoolExecutor(max_workers=3) as executor:
            futures = [executor.submit(worker, i) for i in range(5)]
            results = [f.result() for f in concurrent.futures.as_completed(futures)]

        assert sorted(results) == [0, 1, 2, 3, 4]

    def test_thread_pool_nested(self, trace):
        """Nested @trace calls within thread pool workers."""

        @trace
        def inner(task_id: int) -> str:
            return f"inner:{task_id}"

        @trace
        def worker(task_id: int) -> str:
            time.sleep(0.01 + random.random() * 0.01)
            return inner(task_id)

        @trace
        def root() -> typing.List[str]:
            with concurrent.futures.ThreadPoolExecutor(max_workers=3) as executor:
                futures = [executor.submit(worker, i) for i in range(3)]
                return [f.result() for f in concurrent.futures.as_completed(futures)]

        results = root()
        assert sorted(results) == ["inner:0", "inner:1", "inner:2"]


# ============================================================================
# TEST: Complex async patterns
# ============================================================================


class TestTraceComplexAsync:
    """Test @trace with complex async patterns."""

    @pytest.mark.asyncio
    async def test_gather_with_nested_calls(self, rt, trace):
        """Root → parallel children → each child calls a BAML function."""

        @trace
        async def child_with_baml(n: int) -> int:
            result = await call_function(rt,"ReturnNumber", {"n": n})
            return result.result()

        @trace
        async def root() -> typing.List[int]:
            return list(
                await asyncio.gather(
                    child_with_baml(1),
                    child_with_baml(2),
                    child_with_baml(3),
                )
            )

        results = await root()
        assert sorted(results) == [1, 2, 3]

    @pytest.mark.asyncio
    async def test_nested_gather(self, trace):
        """Two levels of asyncio.gather."""

        @trace
        async def leaf(x: int) -> int:
            await asyncio.sleep(0.01)
            return x

        @trace
        async def branch(x: int) -> typing.List[int]:
            return list(
                await asyncio.gather(
                    leaf(x * 10 + 1),
                    leaf(x * 10 + 2),
                )
            )

        @trace
        async def root() -> typing.List[typing.List[int]]:
            return list(
                await asyncio.gather(
                    branch(1),
                    branch(2),
                    branch(3),
                )
            )

        results = await root()
        assert len(results) == 3
        assert sorted(results[0]) == [11, 12]
        assert sorted(results[1]) == [21, 22]
        assert sorted(results[2]) == [31, 32]

    @pytest.mark.asyncio
    async def test_exception_in_parallel_child(self, trace):
        """Exception in one parallel child doesn't prevent others from tracing."""

        @trace
        async def good_child(x: int) -> int:
            await asyncio.sleep(0.01)
            return x

        @trace
        async def bad_child() -> int:
            await asyncio.sleep(0.01)
            raise ValueError("bad child")

        @trace
        async def root() -> int:
            try:
                await asyncio.gather(
                    good_child(1),
                    bad_child(),
                    good_child(3),
                )
            except ValueError:
                return -1
            return 0

        assert await root() == -1


# ============================================================================
# TEST: CtxManager API — upsert_tags, flush, on_log_event
# These are stubs so the tests verify the API exists and is callable.
# ============================================================================


class TestCtxManagerAPI:
    """Test that CtxManager exposes the expected API surface."""

    def test_trace_fn_is_callable(self, ctx):
        assert callable(ctx.trace_fn)

    def test_upsert_tags_is_callable(self, ctx):
        ctx.upsert_tags(key1="val1", key2="val2")

    def test_flush_is_callable(self, ctx):
        ctx.flush()

    def test_get_returns_host_span_manager(self, ctx):
        hsm = ctx.get()
        assert isinstance(hsm, HostSpanManager)

    def test_clone_context_returns_host_span_manager(self, ctx):
        hsm = ctx.clone_context()
        assert isinstance(hsm, HostSpanManager)

    def test_allow_reset(self, ctx):
        # Must call get() first to populate the context for this thread
        ctx.get()
        assert ctx.allow_reset() is True

    def test_reset(self, ctx):
        ctx.reset()


# ============================================================================
# TEST: Trace event recording
#
# These tests verify that actual trace events are written to disk as JSONL.
# They use TraceFileReader to parse and verify the event hierarchy.
# Modeled after integ-tests/python/tests/test_tracing.py.
# ============================================================================


class TestTraceEventRecording:
    """Tests that verify actual trace events are written to disk as JSONL.

    Modeled after integ-tests/python/tests/test_tracing.py.
    Expected event format:
        {"call_id": "...", "function_event_id": "...", "call_stack": [...],
         "timestamp_epoch_ms": ..., "content": {"type": "function_start", "data": {...}}}
    """

    # ============================================================================
    # Basic start/end events
    # ============================================================================

    def test_sync_trace_records_start_and_end(self, ctx, trace_file):
        """A @trace sync function should write function_start and function_end events."""
        trace = ctx.trace_fn

        @trace
        def traced_fn(x: int) -> int:
            return x * 2

        traced_fn(21)
        ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()
        assert counts["function_start"] == 1
        assert counts["function_end"] == 1

        start_events = reader.find_by_function_name("traced_fn")
        assert len(start_events) == 1
        assert start_events[0].is_function_start
        assert start_events[0].is_root()

    @pytest.mark.asyncio
    async def test_async_trace_records_start_and_end(self, ctx, trace_file):
        """A @trace async function should write function_start and function_end events."""
        trace = ctx.trace_fn

        @trace
        async def traced_async(x: int) -> int:
            return x * 2

        await traced_async(21)
        ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()
        assert counts["function_start"] == 1
        assert counts["function_end"] == 1

    # ============================================================================
    # TEST 1: Simple case - 1 root, 1 child
    # ============================================================================

    @pytest.mark.asyncio
    async def test_simple_root_child(self, ctx, trace_file):
        """1 root → 1 child: verify call stack hierarchy."""
        trace = ctx.trace_fn

        @trace
        async def child_function(arg: str):
            await asyncio.sleep(0.01)
            return f"child: {arg}"

        @trace
        async def root_function(arg: str):
            return await child_function(arg)

        result = await root_function("test-arg")
        assert "test-arg" in result
        ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()
        assert counts["function_start"] == 2  # root + child
        assert counts["function_end"] == 2

        root = reader.find_root("root_function")
        assert root is not None
        assert root.is_root()

        children = reader.find_children(root.call_id, "child_function")
        assert len(children) == 1
        reader.verify_parent_child(root, children[0])

    # ============================================================================
    # TEST 2: Root with multiple children (sequential)
    # ============================================================================

    @pytest.mark.asyncio
    async def test_root_with_sequential_children(self, ctx, trace_file):
        """1 root → 3 sequential children."""
        trace = ctx.trace_fn

        @trace
        async def child_a(arg: str):
            await asyncio.sleep(0.01)
            return f"a:{arg}"

        @trace
        async def child_b(arg: str):
            await asyncio.sleep(0.01)
            return f"b:{arg}"

        @trace
        async def child_c(arg: str):
            await asyncio.sleep(0.01)
            return f"c:{arg}"

        @trace
        async def root(arg: str):
            a = await child_a(arg)
            b = await child_b(arg)
            c = await child_c(arg)
            return f"{a},{b},{c}"

        await root("test")
        ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()
        assert counts["function_start"] == 4  # root + 3 children
        assert counts["function_end"] == 4

        root_event = reader.find_root("root")
        assert root_event is not None

        for child_name in ["child_a", "child_b", "child_c"]:
            children = reader.find_children(root_event.call_id, child_name)
            assert len(children) == 1
            reader.verify_parent_child(root_event, children[0])

    # ============================================================================
    # TEST 3: Root with children in parallel (asyncio.gather)
    # ============================================================================

    @pytest.mark.asyncio
    async def test_root_with_parallel_children(self, ctx, trace_file):
        """1 root → 3 parallel children via asyncio.gather."""
        trace = ctx.trace_fn

        @trace
        async def child_task(task_id: int):
            await asyncio.sleep(0.01 + random.random() * 0.01)
            return task_id

        @trace
        async def root():
            return await asyncio.gather(
                child_task(1),
                child_task(2),
                child_task(3),
            )

        await root()
        ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()
        assert counts["function_start"] == 4  # root + 3 children
        assert counts["function_end"] == 4

        root_event = reader.find_root("root")
        assert root_event is not None
        assert root_event.is_root()

        children = reader.find_children(root_event.call_id, "child_task")
        assert len(children) == 3
        for child in children:
            reader.verify_parent_child(root_event, child)

    # ============================================================================
    # TEST 4: Nested hierarchy (root -> parent -> child)
    # ============================================================================

    @pytest.mark.asyncio
    async def test_nested_hierarchy_three_levels(self, ctx, trace_file):
        """3-level nesting: root → parent → grandchild."""
        trace = ctx.trace_fn

        @trace
        async def grandchild(arg: str):
            await asyncio.sleep(0.01)
            return f"grandchild:{arg}"

        @trace
        async def parent(arg: str):
            return await grandchild(arg)

        @trace
        async def root(arg: str):
            return await parent(arg)

        await root("test")
        ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()
        assert counts["function_start"] == 3
        assert counts["function_end"] == 3

        root_event = reader.find_root("root")
        assert root_event is not None
        assert root_event.depth == 1

        parents = reader.find_children(root_event.call_id, "parent")
        assert len(parents) == 1
        parent_event = parents[0]
        assert parent_event.depth == 2
        reader.verify_parent_child(root_event, parent_event)

        grandchildren = reader.find_children(parent_event.call_id, "grandchild")
        assert len(grandchildren) == 1
        gc = grandchildren[0]
        assert gc.depth == 3
        assert gc.call_stack == [root_event.call_id, parent_event.call_id, gc.call_id]

    # ============================================================================
    # TEST 5: Sync thread pool - simple case
    # ============================================================================

    def test_thread_pool_workers_are_independent_roots(self, ctx, trace_file):
        """Thread pool workers should get fresh contexts (independent roots)."""
        trace = ctx.trace_fn

        @trace
        def worker(task_id: int):
            time.sleep(0.01)
            return task_id

        @trace
        def root():
            with concurrent.futures.ThreadPoolExecutor() as executor:
                futures = [executor.submit(worker, i) for i in range(3)]
                return [f.result() for f in concurrent.futures.as_completed(futures)]

        root()
        ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()
        assert counts["function_start"] == 4  # root + 3 workers
        assert counts["function_end"] == 4

        root_event = reader.find_root("root")
        assert root_event is not None

        # Workers should be independent roots, NOT children of root
        all_starts = reader.get_function_starts()
        workers = [e for e in all_starts if e.function_name == "worker"]
        assert len(workers) == 3
        for w in workers:
            assert (
                w.is_root()
            ), f"Worker should be independent root, got call_stack={w.call_stack}"
            assert w.depth == 1

        # Verify workers are NOT children of the root function
        workers_as_children = reader.find_children(root_event.call_id, "worker")
        assert len(workers_as_children) == 0

    # ============================================================================
    # TEST 7: Complex thread pool test
    # ============================================================================

    def test_thread_pool_nested_within_workers(self, ctx, trace_file):
        """Within a single worker thread, nested calls share the same context."""
        trace = ctx.trace_fn

        @trace
        def inner(task_id: int):
            return f"inner:{task_id}"

        @trace
        def worker(task_id: int):
            time.sleep(0.01)
            return inner(task_id)

        @trace
        def root():
            with concurrent.futures.ThreadPoolExecutor() as executor:
                futures = [executor.submit(worker, i) for i in range(3)]
                return [f.result() for f in concurrent.futures.as_completed(futures)]

        root()
        ctx.flush()

        reader = TraceFileReader(trace_file)

        # Workers are independent roots
        all_starts = reader.get_function_starts()
        workers = [e for e in all_starts if e.function_name == "worker"]
        assert len(workers) == 3

        # inner calls should be children of their worker (same thread)
        inners = reader.find_by_function_name("inner")
        assert len(inners) == 3

        worker_ids = {w.call_id for w in workers}
        for inner_event in inners:
            assert inner_event.depth == 2
            assert inner_event.parent_id in worker_ids

    # ============================================================================
    # TEST 6: Complex async case - multiple levels with parallel execution
    # ============================================================================

    @pytest.mark.asyncio
    async def test_complex_async_gather(self, ctx, trace_file):
        """Complex: root → 3 parallel dummy_fn → each calls nested_fn."""
        trace = ctx.trace_fn

        @trace
        async def nested_fn(x: str):
            await asyncio.sleep(0.01 + random.random() * 0.01)
            return f"nested:{x}"

        @trace
        async def dummy_fn(x: str):
            result = await nested_fn(x)
            return f"dummy:{result}"

        @trace
        async def root():
            return await asyncio.gather(
                dummy_fn("a"),
                dummy_fn("b"),
                dummy_fn("c"),
            )

        await root()
        ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()
        # 1 root + 3 dummy_fn + 3 nested_fn = 7
        assert counts["function_start"] == 7
        assert counts["function_end"] == 7

        root_event = reader.find_root("root")
        assert root_event is not None

        dummy_fns = reader.find_children(root_event.call_id, "dummy_fn")
        assert len(dummy_fns) == 3
        dummy_fn_ids = {d.call_id for d in dummy_fns}

        for df in dummy_fns:
            reader.verify_parent_child(root_event, df)

        nested_fns = reader.find_by_function_name("nested_fn")
        assert len(nested_fns) == 3
        for nf in nested_fns:
            assert nf.depth == 3
            assert nf.root_id == root_event.call_id
            assert nf.parent_id in dummy_fn_ids

    # ============================================================================
    # TEST 8: Async gather at top level
    # ============================================================================

    @pytest.mark.asyncio
    async def test_gather_at_top_level_no_root(self, ctx, trace_file):
        """asyncio.gather at top level: each task is an independent root."""
        trace = ctx.trace_fn

        @trace
        async def task(task_id: int):
            await asyncio.sleep(0.01 + random.random() * 0.01)
            return task_id

        await asyncio.gather(*[task(i) for i in range(10)])
        ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()
        assert counts["function_start"] == 10
        assert counts["function_end"] == 10

        tasks = reader.find_by_function_name("task")
        assert len(tasks) == 10

    # ============================================================================
    # TEST 9: Async gather with root
    # ============================================================================

    @pytest.mark.asyncio
    async def test_async_gather_with_root(self, ctx, trace_file):
        """asyncio.gather under a root: all tasks are children of root."""
        trace = ctx.trace_fn

        @trace
        async def child(task_id: int):
            await asyncio.sleep(0.01 + random.random() * 0.01)
            return task_id

        @trace
        async def root():
            return await asyncio.gather(*[child(i) for i in range(10)])

        await root()
        ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()
        assert counts["function_start"] == 11  # 1 root + 10 children
        assert counts["function_end"] == 11

        root_event = reader.find_root("root")
        assert root_event is not None

        children = reader.find_children(root_event.call_id, "child")
        assert len(children) == 10
        for c in children:
            reader.verify_parent_child(root_event, c)

    # ============================================================================
    # TEST 10: Thread pool async (ThreadPoolExecutor + async functions)
    # ============================================================================

    @pytest.mark.asyncio
    async def test_thread_pool_async(self, ctx, trace_file):
        """ThreadPoolExecutor creating async coroutines awaited in main loop.

        Simplified equivalent of integ-tests TEST 10 — without LLM calls.
        """
        trace = ctx.trace_fn

        @trace
        async def async_leaf(task_id: int):
            await asyncio.sleep(0.01)
            return task_id

        @trace
        async def async_gather():
            return await asyncio.gather(*[async_leaf(i) for i in range(3)])

        # Submit async function to thread pool; worker returns coroutine, main loop awaits it
        with concurrent.futures.ThreadPoolExecutor(max_workers=3) as executor:
            futures = [executor.submit(async_gather) for _ in range(3)]
            for future in concurrent.futures.as_completed(futures):
                coro = future.result()
                await coro

        ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()
        # 3 async_gather + 9 async_leaf = 12
        assert counts["function_start"] == 12
        assert counts["function_end"] == 12

        # Each async_gather should be an independent root
        gather_fns = reader.find_by_function_name("async_gather")
        assert len(gather_fns) == 3
        for gf in gather_fns:
            assert gf.is_root()

        # Each async_gather should have 3 leaf children
        for gf in gather_fns:
            leaves = reader.find_children(gf.call_id, "async_leaf")
            assert len(leaves) == 3

    # ============================================================================
    # TEST 13: Big unions / complex nested args
    # ============================================================================

    @pytest.mark.asyncio
    async def test_complex_nested_args(self, ctx, trace_file):
        """Traced functions with complex nested arguments (dicts, lists, mixed types)."""
        trace = ctx.trace_fn

        @trace
        async def child_function(arg):
            await asyncio.sleep(0.01)
            return f"child: {arg}"

        @trace
        async def root_function(arg):
            result = await child_function(arg)
            return f"root: {result}"

        complex_arg = {
            "a": 10,
            "b": {"c": 20, "d": "hi"},
            "e": [1, 2, "hi", {"f": 30, "g": "hello"}],
        }
        result = await root_function(complex_arg)
        assert "hello" in result
        ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()
        assert counts["function_start"] == 2  # root + child
        assert counts["function_end"] == 2

        root = reader.find_root("root_function")
        assert root is not None
        assert root.is_root()

        children = reader.find_children(root.call_id, "child_function")
        assert len(children) == 1
        reader.verify_parent_child(root, children[0])


# ============================================================================
# TEST 11: Tag propagation via trace events
# ============================================================================


class TestTagPropagation:
    """Tests that verify tag propagation to child spans via trace events."""

    @pytest.mark.asyncio
    async def test_tags_propagate_to_children(self, ctx, trace_file):
        """Tags set in parent should propagate to children."""
        trace = ctx.trace_fn
        set_tags = ctx.upsert_tags

        @trace
        async def child(x: str):
            await asyncio.sleep(0.01)
            return f"child:{x}"

        @trace
        async def parent(x: str):
            set_tags(myKey="myVal")
            return await child(x)

        await parent("test")
        ctx.flush()

        reader = TraceFileReader(trace_file)
        parent_event = reader.find_root("parent")
        assert parent_event is not None
        reader.verify_tags(parent_event, {"myKey": "myVal"})

        children = reader.find_children(parent_event.call_id, "child")
        assert len(children) == 1
        reader.verify_tags(children[0], {"myKey": "myVal"})

    @pytest.mark.asyncio
    async def test_tags_propagate_through_parallel_children(self, ctx, trace_file):
        """Tags set in parent propagate to all parallel children."""
        trace = ctx.trace_fn
        set_tags = ctx.upsert_tags

        @trace
        async def child(x: int):
            await asyncio.sleep(0.01)
            return x

        @trace
        async def parent():
            set_tags(env="test", version="1.0")
            return await asyncio.gather(
                child(1),
                child(2),
                child(3),
            )

        await parent()
        ctx.flush()

        reader = TraceFileReader(trace_file)
        parent_event = reader.find_root("parent")
        assert parent_event is not None

        children = reader.find_children(parent_event.call_id, "child")
        assert len(children) == 3
        for c in children:
            reader.verify_tags(c, {"env": "test", "version": "1.0"})

    def test_tags_propagate_in_sync(self, ctx, trace_file):
        """Tags propagate in sync nested calls."""
        trace = ctx.trace_fn
        set_tags = ctx.upsert_tags

        @trace
        def inner():
            return 1

        @trace
        def outer():
            set_tags(source="outer")
            return inner()

        outer()
        ctx.flush()

        reader = TraceFileReader(trace_file)
        outer_event = reader.find_root("outer")
        assert outer_event is not None
        reader.verify_tags(outer_event, {"source": "outer"})

        inners = reader.find_children(outer_event.call_id, "inner")
        assert len(inners) == 1
        reader.verify_tags(inners[0], {"source": "outer"})

    @pytest.mark.asyncio
    async def test_child_tags_dont_leak_to_siblings(self, ctx, trace_file):
        """Tags set in a child should NOT leak to siblings or parent."""
        trace = ctx.trace_fn
        set_tags = ctx.upsert_tags

        @trace
        async def child_a():
            set_tags(child="a")
            await asyncio.sleep(0.01)
            return "a"

        @trace
        async def child_b():
            await asyncio.sleep(0.01)
            return "b"

        @trace
        async def parent():
            a = await child_a()
            b = await child_b()
            return f"{a},{b}"

        await parent()
        ctx.flush()

        reader = TraceFileReader(trace_file)
        parent_event = reader.find_root("parent")

        child_b_events = reader.find_children(parent_event.call_id, "child_b")
        assert len(child_b_events) == 1
        merged = reader.get_merged_tags(child_b_events[0])
        assert (
            "child" not in merged
        ), f"child_b should not have tag 'child', got {merged}"


# ============================================================================
# TEST: context_depth tracking
# ============================================================================


class TestContextDepth:
    """Tests that verify context_depth tracking in HostSpanManager."""

    def test_context_depth_increases_with_nesting(self, ctx):
        """context_depth should increase with each nested @trace call."""
        trace = ctx.trace_fn
        depths: typing.List[int] = []

        @trace
        def inner():
            depths.append(ctx.get().context_depth())
            return "inner"

        @trace
        def outer():
            depths.append(ctx.get().context_depth())
            inner()
            return "outer"

        outer()
        assert depths == [1, 2], f"Expected [1, 2] but got {depths}"

    def test_context_depth_returns_to_zero(self, ctx):
        """context_depth should return to 0 after traced function completes."""
        trace = ctx.trace_fn

        @trace
        def traced():
            return 1

        traced()
        assert ctx.get().context_depth() == 0


# ============================================================================
# TEST: Flush
# ============================================================================


class TestFlush:
    """Tests for flush() functionality."""

    def test_flush_writes_events(self, ctx, trace_file):
        """flush() should write trace events to disk."""
        trace = ctx.trace_fn

        @trace
        def traced():
            return 1

        traced()
        ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()
        assert counts["function_start"] >= 1
        assert counts["function_end"] >= 1


# ============================================================================
# TEST 14: Cross-boundary tracing — Python @trace → BAML expr function
#
# Verifies that:
# 1. Nested @traced Python functions produce host spans with correct hierarchy
# 2. When a @traced Python function calls a BAML function, the engine's span is
#    nested under the Python spans (via HostSpanContext → call_function_traced)
# 3. The BAML function internally calls another expression function, but that
#    inner call does NOT produce a span (uses Call, not CallWithTrace)
# 4. Only the top-level BAML function appears in the trace, not inner helpers
#
# The expected event hierarchy:
#   parent_py                    (call_stack: [parent_py_id])
#   └── child_py                 (call_stack: [parent_py_id, child_py_id])
#       └── OuterExprFunc        (call_stack: [parent_py_id, child_py_id, outer_id])
#           (InnerHelper is NOT traced — uses Call, not CallWithTrace)
# ============================================================================


class TestCrossBoundaryTracing:
    """Tests that verify Python @trace → BAML function cross-boundary tracing."""

    @pytest.mark.asyncio
    async def test_nested_python_to_baml_expr_function(self, rt, ctx, trace_file):
        """Nested Python @trace → BAML expr function with inner helper.

        The inner BAML expression function (InnerHelper) should NOT appear in
        the trace. Only the top-level BAML function (OuterExprFunc) should have
        a span, properly nested under the Python host spans.
        """
        trace = ctx.trace_fn

        @trace
        async def child_py(x: int, y: int) -> int:
            result = await call_function(rt,
                "OuterExprFunc", {"x": x, "y": y}, ctx.get()
            )
            return result.result()

        @trace
        async def parent_py() -> int:
            return await child_py(3, 4)

        result = await parent_py()
        # OuterExprFunc calls InnerHelper(3, 4) = 7, then * 2 = 14
        assert result == 14
        ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()

        # Expected: parent_py start/end + child_py start/end + OuterExprFunc start/end = 3 starts, 3 ends
        assert counts["function_start"] == 3, (
            f"Expected 3 function_start events (parent_py, child_py, OuterExprFunc), "
            f"got {counts['function_start']}"
        )
        assert counts["function_end"] == 3

        # Verify hierarchy: parent_py is root
        parent_event = reader.find_root("parent_py")
        assert parent_event is not None, "parent_py should be the root span"
        assert parent_event.is_root()
        assert parent_event.depth == 1

        # child_py should be a child of parent_py
        children = reader.find_children(parent_event.call_id, "child_py")
        assert len(children) == 1, "child_py should be a child of parent_py"
        child_event = children[0]
        assert child_event.depth == 2
        reader.verify_parent_child(parent_event, child_event)

        # OuterExprFunc should be a child of child_py (engine span nested under host span)
        baml_children = reader.find_children(child_event.call_id, "OuterExprFunc")
        assert len(baml_children) == 1, "OuterExprFunc should be a child of child_py"
        baml_event = baml_children[0]
        assert baml_event.depth == 3
        reader.verify_parent_child(child_event, baml_event)

        # InnerHelper should NOT appear in the trace
        inner_helpers = reader.find_by_function_name("InnerHelper")
        assert (
            len(inner_helpers) == 0
        ), "InnerHelper should NOT have a span (expression-to-expression call uses Call, not CallWithTrace)"

        # All events should share the same root_span_id (parent_py's span)
        all_starts = reader.get_function_starts()
        root_ids = {e.root_id for e in all_starts}
        assert (
            len(root_ids) == 1
        ), f"All events should share the same root_span_id, got {root_ids}"

    def test_nested_python_to_baml_sync(self, rt, ctx, trace_file):
        """Same as above but with sync Python functions."""
        trace = ctx.trace_fn

        @trace
        def child_py(x: int, y: int) -> int:
            result = call_function_sync(rt,"OuterExprFunc", {"x": x, "y": y}, ctx.get())
            return result.result()

        @trace
        def parent_py() -> int:
            return child_py(5, 6)

        result = parent_py()
        # OuterExprFunc calls InnerHelper(5, 6) = 11, then * 2 = 22
        assert result == 22
        ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()

        assert counts["function_start"] == 3
        assert counts["function_end"] == 3

        parent_event = reader.find_root("parent_py")
        assert parent_event is not None

        children = reader.find_children(parent_event.call_id, "child_py")
        assert len(children) == 1
        child_event = children[0]
        reader.verify_parent_child(parent_event, child_event)

        baml_children = reader.find_children(child_event.call_id, "OuterExprFunc")
        assert len(baml_children) == 1
        baml_event = baml_children[0]
        assert baml_event.depth == 3

        # InnerHelper should NOT appear
        inner_helpers = reader.find_by_function_name("InnerHelper")
        assert len(inner_helpers) == 0


# ============================================================================
# TEST 15: Deep nesting with LLM functions — Python @trace → BAML expr → LLM
#
# Verifies that:
# 1. LLM function calls produce spans via CallWithTrace
# 2. Expression function calls (InnerPipeline) do NOT produce spans (uses Call)
# 3. The trace hierarchy correctly nests:
#    parent_py → child_py → OuterPipeline → [ExtractInfo, SummarizeInfo]
# 4. InnerPipeline does NOT appear in the trace
# 5. All events share the same root_span_id
#
# Uses a local mock HTTP server so no real LLM API calls are made.
# ============================================================================


class TestCrossBoundaryLLMTracing:
    """Tests deep nesting: Python @trace → BAML expr function → LLM functions.

    Expected trace hierarchy:
        parent_py              (depth 1, host root)
        └── child_py           (depth 2, host child)
            └── OuterPipeline  (depth 3, engine root via call_function_traced)
                ├── ExtractInfo  (depth 4, engine child via CallWithTrace)
                └── SummarizeInfo (depth 4, engine child via CallWithTrace)

    InnerPipeline is NOT in the trace (expression→expression uses Call).
    """

    @pytest.fixture
    def mock_server(self):
        """Start a mock LLM HTTP server, yield its port, then shut down."""
        server = http.server.HTTPServer(("127.0.0.1", 0), MockLLMHandler)
        port = server.server_address[1]
        thread = threading.Thread(target=server.serve_forever)
        thread.daemon = True
        thread.start()
        yield port
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)

    @pytest.fixture
    def llm_rt(self, mock_server):
        """BamlRuntime with LLM functions pointing to the mock server."""
        source = LLM_BAML_TEMPLATE.replace(
            "__MOCK_URL__", f"http://127.0.0.1:{mock_server}"
        )
        return BamlRuntime.from_files(".", {"main.baml": source})

    @pytest.fixture
    def llm_ctx(self, llm_rt):
        """BamlCtxManager for the LLM runtime."""
        import baml_py.ctx_manager as cm

        cm.prev_ctx_manager = None
        return BamlCtxManager(llm_rt)

    @pytest.mark.asyncio
    async def test_deep_nesting_with_llm_functions(self, llm_rt, llm_ctx, trace_file):
        """Async: parent_py → child_py → OuterPipeline → ExtractInfo + SummarizeInfo."""
        trace = llm_ctx.trace_fn

        @trace
        async def child_py(text: str) -> str:
            result = await call_function(llm_rt,
                "OuterPipeline", {"input": text}, llm_ctx.get()
            )
            return result.result()

        @trace
        async def parent_py() -> str:
            return await child_py("hello world")

        result = await parent_py()
        assert "mocked" in result
        llm_ctx.flush()

        reader = TraceFileReader(trace_file)
        counts = reader.count_events()

        # parent_py + child_py + OuterPipeline + ExtractInfo + SummarizeInfo = 5
        assert counts["function_start"] == 5, (
            f"Expected 5 function_start events "
            f"(parent_py, child_py, OuterPipeline, ExtractInfo, SummarizeInfo), "
            f"got {counts['function_start']}. "
            f"Functions: {[e.function_name for e in reader.get_function_starts()]}"
        )
        assert counts["function_end"] == 5

        # parent_py is root
        parent_event = reader.find_root("parent_py")
        assert parent_event is not None
        assert parent_event.is_root()
        assert parent_event.depth == 1

        # child_py is child of parent_py
        children = reader.find_children(parent_event.call_id, "child_py")
        assert len(children) == 1
        child_event = children[0]
        assert child_event.depth == 2
        reader.verify_parent_child(parent_event, child_event)

        # OuterPipeline is child of child_py (engine span nested under host)
        outer_events = reader.find_children(child_event.call_id, "OuterPipeline")
        assert len(outer_events) == 1
        outer_event = outer_events[0]
        assert outer_event.depth == 3
        reader.verify_parent_child(child_event, outer_event)

        # ExtractInfo and SummarizeInfo are children of OuterPipeline
        extract_events = reader.find_children(outer_event.call_id, "ExtractInfo")
        assert len(extract_events) == 1
        assert extract_events[0].depth == 4
        reader.verify_parent_child(outer_event, extract_events[0])

        summarize_events = reader.find_children(outer_event.call_id, "SummarizeInfo")
        assert len(summarize_events) == 1
        assert summarize_events[0].depth == 4
        reader.verify_parent_child(outer_event, summarize_events[0])

        # InnerPipeline should NOT appear (expression→expression uses Call)
        inner_events = reader.find_by_function_name("InnerPipeline")
        assert len(inner_events) == 0, (
            "InnerPipeline should NOT have a span "
            "(expression→expression call uses Call, not CallWithTrace)"
        )

        # All events share the same root_span_id
        all_starts = reader.get_function_starts()
        root_ids = {e.root_id for e in all_starts}
        assert (
            len(root_ids) == 1
        ), f"All events should share the same root_span_id, got {root_ids}"

    def test_deep_nesting_with_llm_functions_sync(self, llm_rt, llm_ctx, trace_file):
        """Sync: parent_py → child_py → OuterPipeline → ExtractInfo + SummarizeInfo."""
        trace = llm_ctx.trace_fn

        @trace
        def child_py(text: str) -> str:
            result = call_function_sync(llm_rt,
                "OuterPipeline", {"input": text}, llm_ctx.get()
            )
            return result.result()

        @trace
        def parent_py() -> str:
            return child_py("hello world")

        result = parent_py()
        assert "mocked" in result
        llm_ctx.flush()

        reader = TraceFileReader(trace_file)
        print("Trace hierarchy: ===============================")
        reader.print_trace_hierarchy(show_ids=True)
        counts = reader.count_events()

        assert counts["function_start"] == 5
        assert counts["function_end"] == 5

        # Verify hierarchy
        parent_event = reader.find_root("parent_py")
        assert parent_event is not None

        children = reader.find_children(parent_event.call_id, "child_py")
        assert len(children) == 1
        child_event = children[0]
        reader.verify_parent_child(parent_event, child_event)

        outer_events = reader.find_children(child_event.call_id, "OuterPipeline")
        assert len(outer_events) == 1
        outer_event = outer_events[0]
        reader.verify_parent_child(child_event, outer_event)

        # LLM functions as children of OuterPipeline
        extract_events = reader.find_children(outer_event.call_id, "ExtractInfo")
        assert len(extract_events) == 1
        reader.verify_parent_child(outer_event, extract_events[0])

        summarize_events = reader.find_children(outer_event.call_id, "SummarizeInfo")
        assert len(summarize_events) == 1
        reader.verify_parent_child(outer_event, summarize_events[0])

        # InnerPipeline should NOT appear
        assert len(reader.find_by_function_name("InnerPipeline")) == 0

        # All events share the same root_span_id
        root_ids = {e.root_id for e in reader.get_function_starts()}
        assert len(root_ids) == 1
