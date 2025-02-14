import json
import time
from typing import Any, Callable, List, TypedDict
import psutil
from baml_client import b  # Assuming similar client structure
import asyncio
from typing import TypeVar, Dict
import tracemalloc

# Optional: For plotting (if needed)
# import matplotlib.pyplot as plt
# from datetime import datetime


class MemoryEvent(TypedDict):
    timestamp: int
    event: str


def create_event_logger(events: List[MemoryEvent]) -> Callable[[str], None]:
    def log_event(event: str) -> None:
        print("logging event", event)
        events.append({"timestamp": int(time.time() * 1000), "event": event})

    return log_event


T = TypeVar("T")


async def measure_memory_usage(
    operation: Callable[[Callable[[str], None]], Any],
    options: Dict[str, int] = {"sample_interval": 50},
) -> Dict:
    tracemalloc.start()

    memory_usage_log = []
    events: List[MemoryEvent] = []
    max_heap_used = 0
    max_rss = 0
    max_python_mem = 0

    log_event = create_event_logger(events)
    process = psutil.Process()

    async def monitor():
        nonlocal max_heap_used, max_rss, max_python_mem
        while True:
            mem = process.memory_info()
            current, peak = tracemalloc.get_traced_memory()

            max_heap_used = max(max_heap_used, mem.rss)
            max_rss = max(max_rss, mem.rss)
            max_python_mem = max(max_python_mem, current)

            memory_usage_log.append(
                {
                    "timestamp": int(time.time() * 1000),
                    "heapUsed": mem.rss,
                    "rss": mem.rss,
                    "pythonMemory": current,
                    "pythonPeak": peak,
                }
            )

            print(
                f"mem (MB) {mem.rss / 1024 / 1024:.2f} "
                f"python mem (MB) {current / 1024 / 1024:.2f} "
                f"maxHeapUsed (MB) {max_heap_used / 1024 / 1024:.2f} "
                f"maxRss (MB) {max_rss / 1024 / 1024:.2f} "
                f"maxPythonMem (MB) {max_python_mem / 1024 / 1024:.2f}"
            )
            await asyncio.sleep(options["sample_interval"] / 1000)

    monitor_task = asyncio.create_task(monitor())

    try:
        print("starting operation")
        result = await operation(log_event)
        return {
            "result": result,
            "memoryUsage": {
                "peak": {
                    "heapUsed": max_heap_used,
                    "rss": max_rss,
                    "pythonMemory": max_python_mem,
                },
                "timeline": memory_usage_log,
                "events": events,
            },
        }
    finally:
        monitor_task.cancel()
        tracemalloc.stop()
        try:
            await monitor_task
        except asyncio.CancelledError:
            pass


# def generate_memory_plot(timeline: List[Dict], events: List[MemoryEvent]) -> None:
#     start_time = timeline[0]["timestamp"]
#     time_data = [(t["timestamp"] - start_time) / 1000 for t in timeline]
#     heap_data = [t["heapUsed"] / 1024 / 1024 for t in timeline]
#     rss_data = [t["rss"] / 1024 / 1024 for t in timeline]

#     plt.figure(figsize=(10, 6))
#     plt.plot(time_data, heap_data, label="Heap Used (MB)")
#     plt.plot(time_data, rss_data, label="RSS (MB)")

#     # Add event markers
#     for event in events:
#         event_time = (event["timestamp"] - start_time) / 1000
#         plt.axvline(x=event_time, color="r", linestyle="--", alpha=0.5)
#         plt.text(event_time, plt.ylim()[1], event["event"], rotation=90)

#     plt.title("Memory Usage Over Time")
#     plt.xlabel("Time (seconds)")
#     plt.ylabel("Memory (MB)")
#     plt.legend()
#     plt.tight_layout()
#     plt.savefig("memory-usage-plot.png")


async def main():
    print("Running memory usage test...")

    async def test_operation(log_event):
        log_event("Stream last started")
        stream = b.stream.TestMemory("poems")
        chunk_count = 0
        async for chunk in stream:
            chunk_count += 1
        await stream.get_final_response()
        log_event("Stream last complete")

        print("done streaming")
        print("waiting")

    result = await measure_memory_usage(test_operation, {"sample_interval": 1000})

    await asyncio.sleep(5)

    print("Memory Usage Summary (MB):")
    memory_usage = result["memoryUsage"]
    summary = {
        "Peak Heap Used": round(memory_usage["peak"]["heapUsed"] / 1024 / 1024 * 100)
        / 100,
        "Peak RSS": round(memory_usage["peak"]["rss"] / 1024 / 1024 * 100) / 100,
    }
    print(summary)

    # Save timeline and events to files
    with open("memory-usage-timeline.json", "w") as f:
        json.dump(memory_usage["timeline"], f, indent=2)

    with open("memory-usage-events.json", "w") as f:
        json.dump(memory_usage["events"], f, indent=2)

    # Optional: Generate plot
    # generate_memory_plot(memory_usage["timeline"], memory_usage["events"])
    # print("Memory usage plot has been saved to memory-usage-plot.png")


if __name__ == "__main__":
    asyncio.run(main())
