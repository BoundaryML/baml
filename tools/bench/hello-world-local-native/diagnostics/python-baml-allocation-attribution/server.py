import asyncio
import gc as python_gc
import os
from pathlib import Path
import re
import sys
import threading
import tracemalloc

from starlette.applications import Starlette
from starlette.responses import PlainTextResponse
from starlette.routing import Route

from baml_bridge import baml_py
from baml_sdk import collect_and_report_async, collect_only_async, hello_world_async, sample_heap_stats_async


if not tracemalloc.is_tracing():
    tracemalloc.start(10)

SNAPSHOT_DIR = Path(os.environ["BAML_ALLOCATION_SNAPSHOT_DIR"])
SNAPSHOT_DIR.mkdir(parents=True, exist_ok=True)
LABEL = re.compile(r"[a-z0-9][a-z0-9_-]*")
REQUESTS_STARTED = 0
REQUESTS_COMPLETED = 0
REQUESTS_IN_FLIGHT = 0


def text_response(body, headers=None):
    return PlainTextResponse(body, headers={"Cache-Control": "no-store", **(headers or {})})


def allocation_stats(prefix):
    python_current, python_peak = tracemalloc.get_traced_memory()
    python_blocks = sys.getallocatedblocks()
    tracer_bytes = tracemalloc.get_tracemalloc_memory()
    gc_counts = python_gc.get_count()
    gc_stats = python_gc.get_stats()
    bridge = dict(baml_py._allocation_stats())
    values = {
        "python_allocated_blocks": python_blocks,
        "python_tracemalloc_current_bytes": python_current,
        "python_tracemalloc_peak_bytes": python_peak,
        "python_tracemalloc_overhead_bytes": tracer_bytes,
        "python_gc_count_gen0": gc_counts[0],
        "python_gc_count_gen1": gc_counts[1],
        "python_gc_count_gen2": gc_counts[2],
        "python_gc_collections_gen0": gc_stats[0]["collections"],
        "python_gc_collections_gen1": gc_stats[1]["collections"],
        "python_gc_collections_gen2": gc_stats[2]["collections"],
        "python_gc_collected_gen0": gc_stats[0]["collected"],
        "python_gc_collected_gen1": gc_stats[1]["collected"],
        "python_gc_collected_gen2": gc_stats[2]["collected"],
        "python_gc_uncollectable_gen0": gc_stats[0]["uncollectable"],
        "python_gc_uncollectable_gen1": gc_stats[1]["uncollectable"],
        "python_gc_uncollectable_gen2": gc_stats[2]["uncollectable"],
        "python_gc_garbage": len(python_gc.garbage),
        "python_asyncio_tasks": len(asyncio.all_tasks()),
        "python_threads": threading.active_count(),
        "python_requests_started": REQUESTS_STARTED,
        "python_requests_completed": REQUESTS_COMPLETED,
        "python_requests_in_flight": REQUESTS_IN_FLIGHT,
        "pyo3_live_handles": baml_py._live_handle_count(),
        **bridge,
    }
    return "".join(f"{prefix}_{name}={value}\n" for name, value in values.items())


async def hello(request):
    global REQUESTS_STARTED, REQUESTS_COMPLETED, REQUESTS_IN_FLIGHT
    REQUESTS_STARTED += 1
    REQUESTS_IN_FLIGHT += 1
    try:
        body = await hello_world_async()
        REQUESTS_COMPLETED += 1
        return text_response(body)
    finally:
        REQUESTS_IN_FLIGHT -= 1


async def gc(request):
    report = await collect_and_report_async()
    collected = python_gc.collect(2)
    return text_response(report, {"X-Python-GC-Collected": str(collected)})


async def baml_gc(request):
    report = await collect_and_report_async()
    # The formatted report is itself a BAML rope. Collect once more after it has
    # crossed the Python boundary so the diagnostic response cannot pollute the
    # next allocation floor.
    await collect_only_async()
    return text_response(report)


async def collect_python(request):
    before = sys.getallocatedblocks()
    collected = python_gc.collect(2)
    after = sys.getallocatedblocks()
    return text_response(
        f"python_collected={collected}\npython_allocated_blocks_before={before}\npython_allocated_blocks_after={after}\n"
    )


async def stats(request):
    allocations = allocation_stats("sample")
    heap = await sample_heap_stats_async()
    return text_response(heap + allocations)


async def allocation_only_stats(request):
    return text_response(allocation_stats("sample"))


async def snapshot(request):
    label = request.query_params.get("label", "")
    if not LABEL.fullmatch(label):
        return text_response("invalid snapshot label\n", {"X-Error": "invalid-label"})
    value = tracemalloc.take_snapshot()
    path = SNAPSHOT_DIR / f"{label}.tracemalloc"
    value.dump(str(path))
    return text_response(f"path={path.name}\ntraced_blocks={len(value.traces)}\n")


app = Starlette(
    routes=[
        Route("/", hello, methods=["GET"]),
        Route("/gc", gc, methods=["GET"]),
        Route("/gc/baml", baml_gc, methods=["GET"]),
        Route("/gc/python", collect_python, methods=["GET"]),
        Route("/stats", stats, methods=["GET"]),
        Route("/stats/allocation", allocation_only_stats, methods=["GET"]),
        Route("/snapshot", snapshot, methods=["GET"]),
    ]
)
