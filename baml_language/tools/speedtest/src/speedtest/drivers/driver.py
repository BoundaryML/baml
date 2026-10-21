"""speedtest driver for Python workloads: python3 -S driver.py MAIN_PY JOBS WARMUP

One call verifies, ``warmup - 1`` more are untimed, then ``jobs`` calls are
timed with a monotonic clock. An ``async def main()`` is run to completion per
call. A workload that defines ``setup()`` and ``run(state)`` has ``setup()``
called once, untimed, and ``run(state)`` timed instead of ``main()``. Prints one
JSON report line.
"""
import asyncio
import inspect
import json
import sys
import time


def text(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    return str(value)


def run(path, jobs, warmup):
    scope = {"__name__": "workload", "__file__": path}
    with open(path) as f:
        exec(compile(f.read(), path, "exec"), scope)
    main = scope["main"]
    if "setup" in scope and "run" in scope:
        state = scope["setup"]()
        main = lambda: scope["run"](state)
    elif inspect.iscoroutinefunction(main):
        coroutine = main
        main = lambda: asyncio.run(coroutine())
    result = text(main())
    for _ in range(warmup - 1):
        main()
    start = time.perf_counter_ns()
    for _ in range(jobs):
        main()
    elapsed = time.perf_counter_ns() - start
    return {"result": result, "elapsed_ns": elapsed, "jobs": jobs}


if __name__ == "__main__":
    sys.setrecursionlimit(100000)
    print(json.dumps(run(sys.argv[1], int(sys.argv[2]), int(sys.argv[3]))))
