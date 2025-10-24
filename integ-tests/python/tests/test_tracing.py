# import uuid
import json
import os
import time
from typing import Optional, List, Dict, Any
from pydantic import BaseModel, Field
import pytest
from assertpy import assert_that
import asyncio
import random
import concurrent.futures

from ..baml_client import b
from ..baml_client.globals import (
    DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME,
)
from ..baml_client.tracing import trace, set_tags, flush, on_log_event


# ============================================================================
# Trace Event Models
# ============================================================================
class FunctionStartData(BaseModel):
    """Data for function_start events"""

    function_display_name: str
    args: List[Any] = Field(default_factory=list)
    tags: Dict[str, str] = Field(default_factory=dict)
    function_type: str
    is_stream: bool
    baml_function_content: Optional[str] = None


class FunctionEndData(BaseModel):
    """Data for function_end events"""

    function_display_name: str
    # Add other fields as needed


class EventContent(BaseModel):
    """Content of a trace event"""

    type: str  # "function_start" or "function_end"
    data: Dict[str, Any]  # Will be parsed into FunctionStartData or FunctionEndData


class TraceEvent(BaseModel):
    """Represents a single trace event from the log file"""

    call_id: str
    function_event_id: str
    call_stack: List[str]
    timestamp_epoch_ms: int
    content: EventContent

    @property
    def is_function_start(self) -> bool:
        return self.content.type == "function_start"

    @property
    def is_function_end(self) -> bool:
        return self.content.type == "function_end"

    @property
    def function_name(self) -> str:
        """Get the function display name"""
        return self.content.data.get("function_display_name", "")

    @property
    def tags(self) -> Dict[str, str]:
        """Get tags from function_start events"""
        if self.is_function_start:
            return self.content.data.get("tags", {})
        return {}

    @property
    def parent_id(self) -> Optional[str]:
        """Get the parent call_id (second to last in call_stack)"""
        if len(self.call_stack) >= 2:
            return self.call_stack[-2]
        return None

    @property
    def root_id(self) -> str:
        """Get the root call_id (first in call_stack)"""
        return self.call_stack[0]

    @property
    def depth(self) -> int:
        """Get the depth of this call in the stack"""
        return len(self.call_stack)

    def is_root(self) -> bool:
        """Check if this is a root call"""
        return len(self.call_stack) == 1 and self.call_stack[0] == self.call_id

    def is_child_of(self, parent_id: str) -> bool:
        """Check if this event is a direct child of the given parent_id"""
        return self.parent_id == parent_id

    def is_descendant_of(self, ancestor_id: str) -> bool:
        """Check if this event is a descendant of the given ancestor_id"""
        return ancestor_id in self.call_stack[:-1]  # Exclude self

    def has_tag(self, key: str, value: Optional[str] = None) -> bool:
        """Check if this event has a specific tag, optionally with a specific value"""
        if key not in self.tags:
            return False
        if value is not None:
            return self.tags[key] == value
        return True


# ============================================================================
# Trace File Helpers
# ============================================================================
class TraceFileReader:
    """
    Helper class to read and parse trace files.

    Example usage:
        reader = TraceFileReader(trace_file)

        # Basic usage - print hierarchy
        reader.print_trace_hierarchy()

        # Output:
        # root_function
        #   child_a
        #   child_b
        #   child_c

        # With call IDs
        reader.print_trace_hierarchy(show_ids=True)

        # Output:
        # root_function [bfcall_01abc123]
        #   child_a [bfcall_01def456]
        #   child_b [bfcall_01ghi789]
        #   child_c [bfcall_01jkl012]

        # With depth information
        reader.print_trace_hierarchy(show_depth=True)

        # Output:
        # root_function (depth: 1)
        #   child_a (depth: 2)
        #   child_b (depth: 2)
        #   child_c (depth: 2)

        # With tags (shows propagated tags)
        reader.print_trace_hierarchy(show_tags=True)

        # Output:
        # parent_async
        #   @myKey=myVal
        #   async_dummy_func
        #     @myKey=myVal
        #   FnOutputClass
        #     @myKey=myVal

        # All options together
        reader.print_trace_hierarchy(
            show_ids=True,
            show_tags=True,
            show_depth=True,
            indent_str="    "  # Custom indentation (4 spaces)
        )

        # Get as string instead of printing
        hierarchy_str = reader.get_trace_hierarchy_string(show_ids=True)
    """

    def __init__(self, trace_file_path: str):
        self.trace_file_path = trace_file_path
        self._events: Optional[List[TraceEvent]] = None

    def load_events(self) -> List[TraceEvent]:
        """Load all events from the trace file"""
        if self._events is not None:
            return self._events

        events = []
        if not os.path.exists(self.trace_file_path):
            os.makedirs(os.path.dirname(self.trace_file_path), exist_ok=True)
            with open(self.trace_file_path, "w") as f:
                pass
            return events

        with open(self.trace_file_path, "r") as f:
            for line in f:
                try:
                    event_dict = json.loads(line.strip())
                    event = TraceEvent(**event_dict)
                    events.append(event)
                except (json.JSONDecodeError, Exception) as e:
                    print(f"Warning: Failed to parse event: {e}")
                    continue

        self._events = events
        return events

    def get_function_starts(self) -> List[TraceEvent]:
        """Get all function_start events"""
        return [e for e in self.load_events() if e.is_function_start]

    def get_function_ends(self) -> List[TraceEvent]:
        """Get all function_end events"""
        return [e for e in self.load_events() if e.is_function_end]

    def count_events(self) -> Dict[str, int]:
        """Count function_start and function_end events"""
        events = self.load_events()
        return {
            "function_start": sum(1 for e in events if e.is_function_start),
            "function_end": sum(1 for e in events if e.is_function_end),
        }

    def find_by_function_name(
        self, name: str, event_type: str = "function_start"
    ) -> List[TraceEvent]:
        """Find all events with a specific function name"""
        events = (
            self.get_function_starts()
            if event_type == "function_start"
            else self.get_function_ends()
        )
        return [e for e in events if e.function_name == name]

    def find_root(self, function_name: Optional[str] = None) -> Optional[TraceEvent]:
        """Find the root event, optionally filtered by function name"""
        for event in self.get_function_starts():
            if event.is_root():
                if function_name is None or event.function_name == function_name:
                    return event
        return None

    def find_children(
        self, parent_id: str, function_name: Optional[str] = None
    ) -> List[TraceEvent]:
        """Find all direct children of a given parent"""
        children = [e for e in self.get_function_starts() if e.is_child_of(parent_id)]
        if function_name:
            children = [c for c in children if c.function_name == function_name]
        return children

    def find_descendants(self, ancestor_id: str) -> List[TraceEvent]:
        """Find all descendants of a given ancestor"""
        return [
            e for e in self.get_function_starts() if e.is_descendant_of(ancestor_id)
        ]

    def verify_callstack(self, event: TraceEvent, expected_stack: List[str]) -> bool:
        """Verify that an event's callstack matches the expected stack"""
        return event.call_stack == expected_stack

    def verify_parent_child(self, parent: TraceEvent, child: TraceEvent) -> None:
        """Verify that child is a direct child of parent"""
        assert_that(child.call_stack).is_equal_to([*parent.call_stack, child.call_id])
        print(
            f"✓ {child.function_name}: call_stack = [{parent.function_name}, {child.call_id}]"
        )

    def verify_tags(self, event: TraceEvent, expected_tags: Dict[str, str]) -> None:
        """Verify that an event has the expected tags"""
        for key, value in expected_tags.items():
            assert_that(event.tags).contains_entry({key: value})
        print(f"✓ {event.function_name} has tags: {expected_tags}")

    def print_trace_hierarchy(
        self,
        show_ids: bool = False,
        show_tags: bool = False,
        show_depth: bool = False,
        indent_str: str = "    ",
    ) -> None:
        """
        Print the trace event hierarchy in a nested, tabbed format.

        Args:
            show_ids: If True, show call_ids next to function names
            show_tags: If True, show tags for each function
            show_depth: If True, show depth level in brackets
            indent_str: String to use for indentation (default: two spaces)

        Example output (with show_ids=True):
            root_function [bfcall_123]
              child_a [bfcall_456]
              child_b [bfcall_789]
        """
        events = self.get_function_starts()

        # Sort events by timestamp to maintain chronological order
        events_sorted = sorted(events, key=lambda e: e.timestamp_epoch_ms)

        # Build a tree structure: parent_id -> list of children
        tree: Dict[str, List[TraceEvent]] = {}
        roots: List[TraceEvent] = []

        for event in events_sorted:
            if event.is_root():
                roots.append(event)
            else:
                parent_id = event.parent_id
                if parent_id not in tree:
                    tree[parent_id] = []
                tree[parent_id].append(event)

        def print_event(event: TraceEvent, indent_level: int = 0) -> None:
            """Recursively print an event and its children"""
            indent = indent_str * indent_level

            # Build the output line
            parts = [indent, event.function_name]

            if show_ids:
                parts.append(f" [{event.call_id}]")

            if show_depth:
                parts.append(f" (depth: {event.depth})")

            print("".join(parts))

            if show_tags and event.tags:
                tag_indent = indent_str * (indent_level + 1)
                for key, value in event.tags.items():
                    print(f"{tag_indent}@{key}={value}")

            # Print children
            if event.call_id in tree:
                for child in tree[event.call_id]:
                    print_event(child, indent_level + 1)

        # Print all root events and their hierarchies
        for root in roots:
            print_event(root)
        print("\n ------ \n")

    def get_trace_hierarchy_string(
        self,
        show_ids: bool = False,
        show_tags: bool = False,
        show_depth: bool = False,
        indent_str: str = "  ",
    ) -> str:
        """
        Return the trace event hierarchy as a string (same format as print_trace_hierarchy).

        Args:
            show_ids: If True, show call_ids next to function names
            show_tags: If True, show tags for each function
            show_depth: If True, show depth level in brackets
            indent_str: String to use for indentation (default: two spaces)

        Returns:
            String representation of the trace hierarchy
        """
        import io
        import sys

        # Capture print output
        old_stdout = sys.stdout
        sys.stdout = buffer = io.StringIO()

        try:
            self.print_trace_hierarchy(show_ids, show_tags, show_depth, indent_str)
            return buffer.getvalue()
        finally:
            sys.stdout = old_stdout


def count_trace_events_from_file(trace_file_path: str) -> dict:
    """
    Count function_start and function_end events from a trace file.
    DEPRECATED: Use TraceFileReader.count_events() instead
    """
    reader = TraceFileReader(trace_file_path)
    return reader.count_events()


# ============================================================================
# TEST 1: Simple case - 1 root, 1 child
# ============================================================================
@pytest.mark.asyncio
async def test_tracing_simple_root_child():
    """Test simple tracing: 1 root function calling 1 child function"""

    @trace
    async def child_function(arg: str):
        await asyncio.sleep(0.1)
        return f"child: {arg}"

    @trace
    async def root_function(arg: str):
        result = await child_function(arg)
        return f"root: {result}"

    # Set up trace file for verification
    trace_file = os.environ["BAML_TRACE_FILE"]
    if os.path.exists(trace_file):
        os.remove(trace_file)
    print(f"Trace file: {trace_file}")

    try:
        # Clear any existing traces
        flush()
        _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

        res = await root_function("test-arg")
        assert_that(res).contains("test-arg")

        flush()

        # Verify trace events using helper
        reader = TraceFileReader(trace_file)
        print("\n=== Trace Hierarchy ===")
        reader.print_trace_hierarchy(show_ids=True, show_depth=True)
        event_counts = reader.count_events()
        print(f"Trace event counts: {event_counts}")
        assert_that(event_counts["function_start"]).is_equal_to(2)  # root + child
        assert_that(event_counts["function_end"]).is_equal_to(2)

        # Find root and verify
        root = reader.find_root("root_function")
        assert_that(root).is_not_none()
        assert_that(root.is_root()).is_true()
        print(f"✓ root_function: call_stack = [{root.call_id}]")

        # Find child and verify parent-child relationship
        children = reader.find_children(root.call_id, "child_function")
        assert_that(len(children)).is_equal_to(1)
        child = children[0]
        reader.verify_parent_child(root, child)

        print("✓ Callstack verification complete!")
    finally:
        pass


# ============================================================================
# TEST 2: Root with multiple children (sequential)
# ============================================================================
@pytest.mark.asyncio
async def test_tracing_root_with_multiple_children_sequential():
    """Test tracing: 1 root function calling 3 children sequentially"""

    @trace
    async def child_a(arg: str):
        await asyncio.sleep(0.1)
        return f"child_a: {arg}"

    @trace
    async def child_b(arg: str):
        await asyncio.sleep(0.1)
        return f"child_b: {arg}"

    @trace
    async def child_c(arg: str):
        await asyncio.sleep(0.1)
        return f"child_c: {arg}"

    @trace
    async def root_function(arg: str):
        a = await child_a(arg)
        b = await child_b(arg)
        c = await child_c(arg)
        return f"root: {a}, {b}, {c}"

    # Set up trace file
    trace_file = os.environ["BAML_TRACE_FILE"]
    if os.path.exists(trace_file):
        os.remove(trace_file)

    try:
        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
        _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

        res = await root_function("test")
        assert_that(res).contains("test")

        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()

        # Verify counts: 1 root + 3 children = 4
        reader = TraceFileReader(trace_file)
        event_counts = reader.count_events()
        assert_that(event_counts["function_start"]).is_equal_to(4)
        assert_that(event_counts["function_end"]).is_equal_to(4)

        # Find root and verify
        root = reader.find_root("root_function")
        assert_that(root).is_not_none()
        assert_that(root.is_root()).is_true()
        print(f"✓ root_function: call_stack = [{root.call_id}]")

        # Verify all children have correct parent
        for child_name in ["child_a", "child_b", "child_c"]:
            children = reader.find_children(root.call_id, child_name)
            assert_that(len(children)).is_equal_to(1)
            reader.verify_parent_child(root, children[0])

        # Print trace hierarchy for visualization
        print("\n=== Trace Hierarchy ===")
        reader.print_trace_hierarchy(show_ids=True, show_depth=True)

        print("✓ Callstack verification complete!")
    finally:
        pass


# ============================================================================
# TEST 3: Root with children in parallel (asyncio.gather)
# ============================================================================
@pytest.mark.asyncio
async def test_tracing_root_with_children_parallel():
    """Test tracing: 1 root function calling 3 children in parallel using asyncio.gather"""

    @trace
    async def child_task(task_id: int):
        await asyncio.sleep(0.1 + random.random() * 0.1)
        return f"child_{task_id}"

    @trace
    async def root_function():
        results = await asyncio.gather(
            child_task(1),
            child_task(2),
            child_task(3),
        )
        return results

    # Set up trace file
    trace_file = os.environ["BAML_TRACE_FILE"]
    if os.path.exists(trace_file):
        os.remove(trace_file)

    try:
        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
        _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

        res = await root_function()
        assert_that(len(res)).is_equal_to(3)

        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()

        # Verify counts: 1 root + 3 children = 4
        reader = TraceFileReader(trace_file)
        event_counts = reader.count_events()
        assert_that(event_counts["function_start"]).is_equal_to(4)
        assert_that(event_counts["function_end"]).is_equal_to(4)

        # Find root and verify
        root = reader.find_root("root_function")
        assert_that(root).is_not_none()
        assert_that(root.is_root()).is_true()
        print(f"✓ root_function: call_stack = [{root.call_id}]")

        # Verify all child_task calls have correct parent
        children = reader.find_children(root.call_id, "child_task")
        assert_that(len(children)).is_equal_to(3)
        for child in children:
            reader.verify_parent_child(root, child)

        print("✓ Callstack verification complete!")
    finally:
        pass


# ============================================================================
# TEST 4: Nested hierarchy (root -> parent -> child)
# ============================================================================
@pytest.mark.asyncio
async def test_tracing_nested_hierarchy():
    """Test tracing: 3-level nesting (root -> parent -> child)"""

    @trace
    async def grandchild_function(arg: str):
        await asyncio.sleep(0.1)
        return f"grandchild: {arg}"

    @trace
    async def parent_function(arg: str):
        result = await grandchild_function(arg)
        return f"parent: {result}"

    @trace
    async def root_function(arg: str):
        result = await parent_function(arg)
        return f"root: {result}"

    # Set up trace file
    trace_file = os.environ["BAML_TRACE_FILE"]
    if os.path.exists(trace_file):
        os.remove(trace_file)

    try:
        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
        _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

        res = await root_function("test")
        assert_that(res).contains("test")

        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()

        # Verify counts: 3 functions
        reader = TraceFileReader(trace_file)
        event_counts = reader.count_events()
        assert_that(event_counts["function_start"]).is_equal_to(3)
        assert_that(event_counts["function_end"]).is_equal_to(3)

        # Find root (depth 1)
        root = reader.find_root("root_function")
        assert_that(root).is_not_none()
        assert_that(root.depth).is_equal_to(1)
        print(f"✓ root_function: call_stack = [{root.call_id}]")

        # Find parent (depth 2)
        parents = reader.find_children(root.call_id, "parent_function")
        assert_that(len(parents)).is_equal_to(1)
        parent = parents[0]
        assert_that(parent.depth).is_equal_to(2)
        reader.verify_parent_child(root, parent)

        # Find grandchild (depth 3)
        grandchildren = reader.find_children(parent.call_id, "grandchild_function")
        assert_that(len(grandchildren)).is_equal_to(1)
        grandchild = grandchildren[0]
        assert_that(grandchild.depth).is_equal_to(3)
        assert_that(grandchild.call_stack).is_equal_to(
            [root.call_id, parent.call_id, grandchild.call_id]
        )
        print(
            f"✓ grandchild_function: call_stack = [root_function, parent_function, {grandchild.call_id}]"
        )

        print("✓ Callstack verification complete!")
    finally:
        pass


# ============================================================================
# TEST 5: Sync thread pool - simple case
# ============================================================================
def test_tracing_thread_pool_simple():
    """Test tracing with thread pool: workers start with fresh context (no parent relationship)"""

    @trace
    def worker_task(task_id: int):
        time.sleep(0.1 + random.random() * 0.1)
        return f"worker_{task_id}"

    @trace
    def root_thread_pool():
        with concurrent.futures.ThreadPoolExecutor() as executor:
            # Submit workers directly - they will get fresh contexts
            futures = [executor.submit(worker_task, i) for i in range(3)]
            for future in concurrent.futures.as_completed(futures):
                future.result()

    # Set up trace file
    trace_file = os.environ["BAML_TRACE_FILE"]
    if os.path.exists(trace_file):
        os.remove(trace_file)

    try:
        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
        _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

        root_thread_pool()

        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()

        # Verify counts: 1 root + 3 workers = 4
        reader = TraceFileReader(trace_file)

        print("\n=== Trace Hierarchy ===")
        reader.print_trace_hierarchy(show_ids=True, show_depth=True)
        event_counts = reader.count_events()
        assert_that(event_counts["function_start"]).is_equal_to(4)
        assert_that(event_counts["function_end"]).is_equal_to(4)

        # Find root thread pool function
        root = reader.find_root("root_thread_pool")
        assert_that(root).is_not_none()
        assert_that(root.is_root()).is_true()
        print(f"✓ root_thread_pool: call_stack = [{root.call_id}]")

        # Workers should be independent roots (not children of root_thread_pool)
        # This is expected behavior: ThreadPoolExecutor workers get fresh contexts
        all_events = reader.get_function_starts()
        worker_events = [e for e in all_events if e.function_name == "worker_task"]
        assert_that(len(worker_events)).is_equal_to(3)

        # Verify workers are independent roots (depth 1, not children)
        for worker in worker_events:
            assert_that(worker.is_root()).is_true()
            assert_that(worker.depth).is_equal_to(1)
            print(f"✓ worker_task: independent root with call_stack = [{worker.call_id}]")

        # Verify workers are NOT children of root_thread_pool
        workers_as_children = reader.find_children(root.call_id, "worker_task")
        assert_that(len(workers_as_children)).is_equal_to(0)

        print("✓ Thread pool test: workers correctly have independent contexts!")
    finally:
        pass


# ============================================================================
# TEST 6: Complex async case - multiple levels with parallel execution
# ============================================================================
@pytest.mark.asyncio
async def test_tracing_complex_async():
    """
    Complex tracing test: root -> multiple parents (parallel) -> multiple children (parallel)
    Replicates the test_tracing_async_only pattern from test_functions.py
    """

    @trace
    async def nested_dummy_fn(_foo: str):
        time.sleep(0.5 + random.random())
        return "nested dummy fn"

    async def failsafe_baml_fn(foo: str):
        try:
            await b.FnOutputClass(foo)
        except Exception as e:
            print("ERROR", e)
            return "failsafe baml fn"

    @trace
    async def dummy_fn(foo: str):
        await asyncio.gather(
            failsafe_baml_fn(foo),
            nested_dummy_fn(foo),
        )
        return "dummy fn"

    @trace
    async def top_level_async_tracing():
        await asyncio.gather(
            dummy_fn("dummy arg 1"),
            dummy_fn("dummy arg 2"),
            dummy_fn("dummy arg 3"),
        )
        return 1

    # Set up trace file
    trace_file = os.environ["BAML_TRACE_FILE"]
    if os.path.exists(trace_file):
        os.remove(trace_file)

    try:
        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
        _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

        try:
            res = await top_level_async_tracing()
            assert_that(res).is_equal_to(1)
        except Exception as e:
            print("ERROR", e)

        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()

        # Verify trace events using helper
        reader = TraceFileReader(trace_file)
        event_counts = reader.count_events()
        print(f"Trace event counts: {event_counts}")
        # 1 top_level + 3 dummy_fn + 3 nested_dummy_fn + 3 FnOutputClass = 10
        assert_that(event_counts["function_start"]).is_equal_to(10)
        assert_that(event_counts["function_end"]).is_equal_to(10)

        # Find root
        top_level = reader.find_root("top_level_async_tracing")
        assert_that(top_level).is_not_none()
        print(f"✓ top_level_async_tracing: call_stack = [{top_level.call_id}]")

        # Find all dummy_fn children - should have 3
        dummy_fns = reader.find_children(top_level.call_id, "dummy_fn")
        assert_that(len(dummy_fns)).is_equal_to(3)
        dummy_fn_ids = [d.call_id for d in dummy_fns]
        for dummy_fn in dummy_fns:
            reader.verify_parent_child(top_level, dummy_fn)

        # Find all nested_dummy_fn calls - each should be a child of a dummy_fn
        nested_dummy_fns = reader.find_by_function_name("nested_dummy_fn")
        assert_that(len(nested_dummy_fns)).is_equal_to(3)
        for nested in nested_dummy_fns:
            # Verify depth and that parent is one of the dummy_fn calls
            assert_that(nested.depth).is_equal_to(3)
            assert_that(nested.root_id).is_equal_to(top_level.call_id)
            assert_that(nested.parent_id).is_in(*dummy_fn_ids)
            print(
                f"✓ nested_dummy_fn: call_stack = [top_level_async_tracing, dummy_fn:{nested.parent_id}, {nested.call_id}]"
            )

        # Find all FnOutputClass calls - each should be a child of a dummy_fn
        fn_outputs = reader.find_by_function_name("FnOutputClass")
        assert_that(len(fn_outputs)).is_equal_to(3)
        for fn_output in fn_outputs:
            # Verify depth and that parent is one of the dummy_fn calls
            assert_that(fn_output.depth).is_equal_to(3)
            assert_that(fn_output.root_id).is_equal_to(top_level.call_id)
            assert_that(fn_output.parent_id).is_in(*dummy_fn_ids)
            print(
                f"✓ FnOutputClass: call_stack = [top_level_async_tracing, dummy_fn:{fn_output.parent_id}, {fn_output.call_id}]"
            )

        print("✓ Callstack verification complete!")
    finally:
        pass


# ============================================================================
# TEST 7: Complex thread pool test
# ============================================================================
@trace
def sync_dummy_func(dummyFuncArg: str):
    return "pythonDummyFuncOutput"


@trace
def parent_sync(myStr: str):
    time.sleep(0.5 + random.random())
    sync_dummy_func(myStr)
    return "hello world parentsync"


@trace
def trace_thread_pool():
    with concurrent.futures.ThreadPoolExecutor() as executor:
        # Create 10 tasks and execute them
        futures = [
            executor.submit(parent_sync, "second-dummycall-arg") for _ in range(10)
        ]
        for future in concurrent.futures.as_completed(futures):
            future.result()


def test_tracing_thread_pool_complex():
    """Complex thread pool test: workers get fresh contexts, maintain parent-child within same thread"""
    # Set up trace file for verification
    trace_file = os.environ["BAML_TRACE_FILE"]
    if os.path.exists(trace_file):
        os.remove(trace_file)

    try:
        # Clear any existing traces
        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
        _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

        trace_thread_pool()

        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()

        # Verify trace events using helper
        reader = TraceFileReader(trace_file)
        event_counts = reader.count_events()
        print(f"Trace event counts: {event_counts}")
        # Expected: 1 trace_thread_pool + 10 parent_sync + 10 sync_dummy_func = 21
        assert_that(event_counts["function_start"]).is_equal_to(21)
        assert_that(event_counts["function_end"]).is_equal_to(21)

        # Find root
        root = reader.find_root("trace_thread_pool")
        assert_that(root).is_not_none()
        print(f"✓ trace_thread_pool: call_stack = [{root.call_id}]")

        # parent_sync functions should be independent roots (not children of trace_thread_pool)
        # because they run in thread pool workers with fresh contexts
        all_events = reader.get_function_starts()
        parent_syncs = [e for e in all_events if e.function_name == "parent_sync"]
        assert_that(len(parent_syncs)).is_equal_to(10)

        # Verify parent_syncs are independent roots
        for parent_sync in parent_syncs:
            assert_that(parent_sync.is_root()).is_true()
            assert_that(parent_sync.depth).is_equal_to(1)
            print(f"✓ parent_sync: independent root with call_stack = [{parent_sync.call_id}]")

        # sync_dummy_func calls should be children of parent_sync (same thread)
        sync_dummies = reader.find_by_function_name("sync_dummy_func")
        assert_that(len(sync_dummies)).is_equal_to(10)

        parent_sync_ids = [p.call_id for p in parent_syncs]
        for sync_dummy in sync_dummies:
            # Should be depth 2 (child of parent_sync within same thread)
            assert_that(sync_dummy.depth).is_equal_to(2)
            # Parent should be one of the parent_sync calls
            assert_that(sync_dummy.parent_id).is_in(*parent_sync_ids)
            print(f"✓ sync_dummy_func: child of parent_sync with call_stack length {sync_dummy.depth}")

        print("✓ Thread pool complex test: correct independent contexts with proper nesting!")
    finally:
        pass


# ============================================================================
# TEST 8: Async gather at top level
# ============================================================================
@trace
async def async_dummy_func(myArgggg: str):
    await asyncio.sleep(0.5 + random.random())
    return "asyncDummyFuncOutput"


@pytest.mark.asyncio
async def test_tracing_async_gather_top_level():
    """Test async gather at top level without explicit root function"""
    # Set up trace file
    trace_file = os.environ["BAML_TRACE_FILE"]
    if os.path.exists(trace_file):
        os.remove(trace_file)

    try:
        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
        _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

        await asyncio.gather(
            *[async_dummy_func("second-dummycall-arg") for _ in range(10)]
        )

        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()

        # Verify 10 async_dummy_func calls
        reader = TraceFileReader(trace_file)
        event_counts = reader.count_events()
        assert_that(event_counts["function_start"]).is_equal_to(10)
        assert_that(event_counts["function_end"]).is_equal_to(10)

        # Verify all are async_dummy_func calls (no root)
        async_dummies = reader.find_by_function_name("async_dummy_func")
        assert_that(len(async_dummies)).is_equal_to(10)

        print("✓ Top-level async gather test complete!")
    finally:
        pass


# ============================================================================
# TEST 9: Async gather with root
# ============================================================================
@trace
async def trace_async_gather():
    await asyncio.gather(
        *[async_dummy_func("handcrafted-artisan-arg") for _ in range(10)]
    )


@pytest.mark.asyncio
async def test_tracing_async_gather():
    """Test async gather with explicit root function"""
    # Set up trace file
    trace_file = os.environ["BAML_TRACE_FILE"]
    if os.path.exists(trace_file):
        os.remove(trace_file)

    try:
        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
        _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

        await trace_async_gather()

        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()

        # Verify: 1 root + 10 children = 11
        reader = TraceFileReader(trace_file)
        event_counts = reader.count_events()
        assert_that(event_counts["function_start"]).is_equal_to(11)
        assert_that(event_counts["function_end"]).is_equal_to(11)

        # Find root and verify
        root = reader.find_root("trace_async_gather")
        assert_that(root).is_not_none()
        assert_that(root.is_root()).is_true()
        print(f"✓ trace_async_gather: call_stack = [{root.call_id}]")

        # Verify all async_dummy_func calls are children of root
        children = reader.find_children(root.call_id, "async_dummy_func")
        assert_that(len(children)).is_equal_to(10)
        for child in children:
            reader.verify_parent_child(root, child)

        print("✓ Async gather with root test complete!")
    finally:
        pass


# ============================================================================
# TEST 10: Thread pool async (ThreadPoolExecutor calling async functions)
# ============================================================================
@trace
async def trace_thread_pool_async():
    with concurrent.futures.ThreadPoolExecutor() as executor:
        # Create 10 tasks and execute them
        futures = [executor.submit(trace_async_gather) for _ in range(10)]
        for future in concurrent.futures.as_completed(futures):
            _ = await future.result()


@pytest.mark.asyncio
async def test_tracing_thread_pool_async():
    """Test thread pool calling async functions"""
    # Set up trace file
    trace_file = os.environ["BAML_TRACE_FILE"]
    if os.path.exists(trace_file):
        os.remove(trace_file)

    try:
        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
        _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

        await trace_thread_pool_async()

        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()

        # Verify: 1 trace_thread_pool_async + 10 trace_async_gather + 100 async_dummy_func = 111
        reader = TraceFileReader(trace_file)
        event_counts = reader.count_events()
        assert_that(event_counts["function_start"]).is_equal_to(111)
        assert_that(event_counts["function_end"]).is_equal_to(111)

        # Find root
        root = reader.find_root("trace_thread_pool_async")
        assert_that(root).is_not_none()
        print(f"✓ trace_thread_pool_async: call_stack = [{root.call_id}]")

        # Verify all trace_async_gather calls are children
        gather_children = reader.find_children(root.call_id, "trace_async_gather")
        assert_that(len(gather_children)).is_equal_to(10)
        print(f"✓ Found {len(gather_children)} trace_async_gather children")

        # Verify all async_dummy_func calls are descendants
        async_dummies = reader.find_by_function_name("async_dummy_func")
        assert_that(len(async_dummies)).is_equal_to(100)
        for dummy in async_dummies:
            assert_that(dummy.is_descendant_of(root.call_id)).is_true()
        print(f"✓ Found {len(async_dummies)} async_dummy_func descendants")

        print("✓ Thread pool async test complete!")
    finally:
        pass


# ============================================================================
# TEST 11: Test with tags - verify tag propagation to children
# ============================================================================
@trace
async def parent_async(myStr: str):
    set_tags(myKey="myVal")
    await async_dummy_func(myStr)
    await b.FnOutputClass(myStr)
    sync_dummy_func(myStr)
    return "hello world parentasync"


@pytest.mark.asyncio
async def test_tracing_with_tags():
    """Test that tags set in parent are propagated to all child functions"""
    # Set up trace file
    trace_file = os.environ["BAML_TRACE_FILE"]
    if os.path.exists(trace_file):
        os.remove(trace_file)

    try:
        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
        _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

        res = await parent_async("test-with-tags")
        assert_that(res).is_equal_to("hello world parentasync")

        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()

        # Verify trace events using helper
        reader = TraceFileReader(trace_file)
        event_counts = reader.count_events()
        assert_that(event_counts["function_start"]).is_equal_to(4)
        assert_that(event_counts["function_end"]).is_equal_to(4)

        # Find parent and verify it has the tag
        parent = reader.find_root("parent_async")
        assert_that(parent).is_not_none()
        expected_tags = {"myKey": "myVal"}
        reader.verify_tags(parent, expected_tags)

        # Verify all children have the tag propagated
        child_functions = ["async_dummy_func", "FnOutputClass", "sync_dummy_func"]
        for child_name in child_functions:
            children = reader.find_children(parent.call_id, child_name)
            assert_that(len(children)).is_equal_to(1)
            child = children[0]
            # Verify the tag was propagated
            reader.verify_tags(child, expected_tags)

        # Print trace hierarchy with tags for visualization
        print("\n=== Trace Hierarchy with Tags ===")
        reader.print_trace_hierarchy(show_ids=True, show_tags=True)

        print("✓ Tag propagation verification complete!")
    finally:
        pass


def test_tracing_sync():
    """Simple sync tracing test"""
    _ = sync_dummy_func("second-dummycall-arg")


# ============================================================================
# Example: Trace Hierarchy Visualization
# ============================================================================
@pytest.mark.asyncio
async def test_trace_hierarchy_visualization():
    """Example test demonstrating trace hierarchy visualization features"""

    @trace
    async def leaf_function(x: int):
        await asyncio.sleep(0.01)
        return x * 2

    @trace
    async def branch_function(x: int):
        set_tags(level="branch")
        a = await leaf_function(x)
        b = await leaf_function(x + 1)
        return a + b

    @trace
    async def root_function():
        set_tags(level="root", env="test")
        results = await asyncio.gather(
            branch_function(1),
            branch_function(2),
            branch_function(3),
        )
        return sum(results)

    # Set up trace file
    trace_file = os.environ["BAML_TRACE_FILE"]
    if os.path.exists(trace_file):
        os.remove(trace_file)

    try:
        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
        _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

        result = await root_function()
        assert result > 0

        DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()

        reader = TraceFileReader(trace_file)

        print("\n" + "=" * 60)
        print("Example 1: Basic hierarchy (just function names)")
        print("=" * 60)
        reader.print_trace_hierarchy()

        print("\n" + "=" * 60)
        print("Example 2: With call IDs")
        print("=" * 60)
        reader.print_trace_hierarchy(show_ids=True)

        print("\n" + "=" * 60)
        print("Example 3: With depth information")
        print("=" * 60)
        reader.print_trace_hierarchy(show_depth=True)

        print("\n" + "=" * 60)
        print("Example 4: With tags (showing tag propagation)")
        print("=" * 60)
        reader.print_trace_hierarchy(show_tags=True)

        print("\n" + "=" * 60)
        print("Example 5: All options together with custom indentation")
        print("=" * 60)
        reader.print_trace_hierarchy(
            show_ids=True,
            show_tags=True,
            show_depth=True,
            indent_str="│   ",  # Tree-like indentation
        )

        print("\n" + "=" * 60)
        print("Example 6: Get as string for programmatic use")
        print("=" * 60)
        hierarchy_str = reader.get_trace_hierarchy_string(show_ids=True)
        print(f"String length: {len(hierarchy_str)} characters")
        print("First 200 chars:")
        print(hierarchy_str[:200] + "...")

    finally:
        pass


# ============================================================================
# TEST 12: Event log hook
# ============================================================================
@pytest.mark.asyncio
async def test_event_log_hook():
    """Test event log hook functionality"""
    import baml_py

    def event_log_hook(event: baml_py.baml_py.BamlLogEvent):
        print("Event log hook1: ")
        print("Event log event ", event)

    flush()  # clear any existing hooks
    on_log_event(event_log_hook)
    res = await b.TestFnNamedArgsSingleStringList(["a", "b", "c"])
    assert res
    flush()  # clear the hook
    on_log_event(None)


# ============================================================================
# Cleanup fixture
# ============================================================================
@pytest.fixture(scope="session", autouse=True)
def flush_traces():
    """Ensure traces are flushed when pytest exits."""
    yield
    print("[python] Flushing traces")
    from baml_client.tracing import flush

    print("Flushing traces (after import)")
    flush()
    print("[python] Traces flushed")
