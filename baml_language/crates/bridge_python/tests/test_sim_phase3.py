"""Sim tests: prove the three-arg factory contract + 09d/09e encoding.

Phase 3 wired the call chain using dict in / dict out. Phase 4 extends
`encode_call_args` / `decode_call_result` so a typed `MyLorem` round-trips
as a `MyLorem` — the minimum bar for phase 4 exit.

The dict-shaped calls still work (Rust coerces Map→Instance at the project
boundary, per `bex_project::bex::coerce_arg_to_declared_type`), so we
keep both call forms exercised.
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

SIM_ROOT = Path(__file__).parent / "sim"
if str(SIM_ROOT) not in sys.path:
    sys.path.insert(0, str(SIM_ROOT))


def test_import_surface():
    """Every symbol named in the phase-3 exit criteria resolves."""
    from baml.baml_core import (
        BamlPyHandle,
        BamlRuntime,
        define_function,
        get_runtime,
    )

    assert BamlPyHandle is not None
    assert BamlRuntime is not None
    assert callable(define_function)
    assert callable(get_runtime)


def test_factory_roundtrip_sync():
    """Dict-in call: Rust Map→Instance coercion still works; the outbound
    class_value gets decoded through _resolve_type into a typed MyLorem."""
    import baml_sdk

    result = baml_sdk.lorem.add_three_to_field_a({"a": 5})
    assert isinstance(result, baml_sdk.lorem.MyLorem)
    assert result.a == 8


@pytest.mark.asyncio
async def test_factory_roundtrip_async():
    """Async factory variant also round-trips."""
    import baml_sdk

    result = await baml_sdk.lorem.add_three_to_field_a_async({"a": 10})
    assert isinstance(result, baml_sdk.lorem.MyLorem)
    assert result.a == 13


def test_typed_roundtrip_sync():
    """Phase-4 exit bar: typed `MyLorem` in → typed `MyLorem` out.

    Uses the same `baml_sdk.*` import path as the dict tests above.
    Importing the same file via two paths (`baml_sdk.lorem` vs.
    `sim.baml_sdk.lorem`) yields distinct class objects in Python's
    module system, so `isinstance` only holds when sdk_root and the test
    import agree — they agree here because sys.path exposes SIM_ROOT
    as the root."""
    from baml_sdk.lorem import MyLorem, add_three_to_field_a

    result = add_three_to_field_a(MyLorem(a=5))
    assert isinstance(result, MyLorem)
    assert result.a == 8


@pytest.mark.asyncio
async def test_typed_roundtrip_async():
    from baml_sdk.lorem import MyLorem, add_three_to_field_a_async

    result = await add_three_to_field_a_async(MyLorem(a=10))
    assert isinstance(result, MyLorem)
    assert result.a == 13
