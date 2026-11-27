"""Time calls into BAML from Python: today's SDK or the native bindings,
whichever `baml_sdk` PYTHONPATH finds, through the same generated API.

    bench.py <async|sync|py> <case> <n> [rounds]

Prints one line: `<case> <mode> <n> <median seconds per call>`. A case
calls one function `n` times one after the other (the array cases do one
call with `n` elements; their time is per element). `py` builds the same
Python objects with no bridge at all; `BENCH_MODEL` picks how
(`validate`, `construct`, or `native` for `object.__new__` and the slots,
as the native module does). For the native bindings `BENCH_MODEL` also
picks the module's model mode, and `BENCH_RELEASE_GIL=1` releases the GIL
while BAML runs.

Instructions per call come from running a case at two sizes under
`/usr/bin/time -l` and dividing the difference.
"""

import asyncio
import os
import statistics
import sys
import time

import pydantic

import baml_sdk
from baml_sdk.bridge_test import (
    Point,
    add,
    add_sync,
    count_ints,
    count_ints_sync,
    count_points,
    count_points_sync,
    make_ints,
    make_ints_sync,
    make_points,
    make_points_sync,
    nap,
)
from baml_sdk.class_refs import Inner, round_trip_inner, round_trip_inner_sync
from baml_sdk.complex_models import round_trip_complex_profile, round_trip_complex_profile_sync
from baml_sdk.primitives import round_trip_int, round_trip_int_sync
from roundtrip_tests_profile import profile

MODEL = os.environ.get("BENCH_MODEL", "validate")
NATIVE = "native" in os.path.dirname(baml_sdk.__file__).split(os.sep)
if NATIVE:
    import baml_native

    baml_native.set_model_mode(MODEL)
    baml_native.set_release_gil(os.environ.get("BENCH_RELEASE_GIL") == "1")

ARRAY_CASES = {"ints_out", "ints_in", "points_out", "points_in"}

_object_new = object.__new__


def build(cls, fields):
    """A model from its fields, the way `MODEL` says."""
    if MODEL == "validate":
        return cls.model_validate(fields)
    if MODEL == "construct":
        return cls.model_construct(**fields)
    instance = _object_new(cls)
    object.__setattr__(instance, "__dict__", fields)
    object.__setattr__(instance, "__pydantic_fields_set__", set(fields))
    object.__setattr__(instance, "__pydantic_extra__", None)
    object.__setattr__(instance, "__pydantic_private__", None)
    return instance


def rebuild(value):
    """Build `value` again bottom-up, as a bridge builds a result."""
    if isinstance(value, pydantic.BaseModel):
        return build(type(value), {name: rebuild(getattr(value, name)) for name in type(value).model_fields})
    if isinstance(value, list):
        return [rebuild(item) for item in value]
    if isinstance(value, dict):
        return {key: rebuild(item) for key, item in value.items()}
    return value


def prepare(case, n):
    """Arguments made before the clock starts."""
    if case == "profile":
        return profile()
    if case == "ints_in":
        return list(range(n))
    if case == "points_in":
        return [Point(x=i, y=n - i) for i in range(n)]
    return None


async def run_async(case, n, arg):
    if case == "add":
        for i in range(n):
            await add(i, 1)
    elif case == "int":
        for i in range(n):
            await round_trip_int(i)
    elif case == "inner":
        for i in range(n):
            await round_trip_inner(Inner(value=i))
    elif case == "profile":
        for _ in range(n):
            await round_trip_complex_profile(arg)
    elif case == "ints_out":
        await make_ints(n)
    elif case == "ints_in":
        await count_ints(arg)
    elif case == "points_out":
        await make_points(n)
    elif case == "points_in":
        await count_points(arg)
    elif case == "nap0":
        for _ in range(n):
            await nap(0)
    else:
        raise SystemExit(f"no case {case}")


def run_sync(case, n, arg):
    if case == "add":
        for i in range(n):
            add_sync(i, 1)
    elif case == "int":
        for i in range(n):
            round_trip_int_sync(i)
    elif case == "inner":
        for i in range(n):
            round_trip_inner_sync(Inner(value=i))
    elif case == "profile":
        for _ in range(n):
            round_trip_complex_profile_sync(arg)
    elif case == "ints_out":
        make_ints_sync(n)
    elif case == "ints_in":
        count_ints_sync(arg)
    elif case == "points_out":
        make_points_sync(n)
    elif case == "points_in":
        count_points_sync(arg)
    else:
        raise SystemExit(f"no case {case}")


def run_py(case, n, arg):
    """The same Python objects, with no bridge: what pydantic and Python
    cost by themselves."""
    if case in ("add", "int"):
        for i in range(n):
            int(i)
    elif case == "inner":
        for i in range(n):
            build(Inner, {"value": i})
    elif case == "profile":
        for _ in range(n):
            rebuild(arg)
    elif case == "ints_out":
        [int(i) for i in range(n)]
    elif case == "ints_in":
        for x in arg:
            int(x)
    elif case == "points_out":
        [build(Point, {"x": i, "y": n - i}) for i in range(n)]
    elif case == "points_in":
        for p in arg:
            p.__dict__["x"], p.__dict__["y"]
    else:
        raise SystemExit(f"no case {case}")


def main():
    mode, case, n = sys.argv[1], sys.argv[2], int(sys.argv[3])
    rounds = int(sys.argv[4]) if len(sys.argv) > 4 else 1
    arg = prepare(case, n)
    per = []
    for _ in range(rounds):
        started = time.perf_counter()
        if mode == "async":
            asyncio.run(run_async(case, n, arg))
        elif mode == "sync":
            run_sync(case, n, arg)
        else:
            run_py(case, n, arg)
        per.append((time.perf_counter() - started) / n)
    print(f"{case} {mode} {n} {statistics.median(per):.9f}")


if __name__ == "__main__":
    main()
