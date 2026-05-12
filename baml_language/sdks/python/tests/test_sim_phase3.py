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
    from baml_core import (
        BamlPyHandle,
        BamlRuntime,
        define_function,
        get_runtime,
    )

    assert BamlPyHandle is not None
    assert BamlRuntime is not None
    assert callable(define_function)
    assert callable(get_runtime)


def test_factory_keyword_only_omission_contract():
    from baml_core import UNSET, _build_kwargs

    assert _build_kwargs(("cats",), {"max_results": 5}, ["query", "max_results"], 1) == {
        "query": "cats",
        "max_results": 5,
    }
    assert _build_kwargs(("cats",), {"max_results": UNSET}, ["query", "max_results"], 1) == {
        "query": "cats",
    }
    assert _build_kwargs(("cats",), {"query": UNSET}, ["query"], 1) == {
        "query": "cats",
    }
    assert _build_kwargs(("cats",), {"max_results": None}, ["query", "max_results"], 1) == {
        "query": "cats",
        "max_results": None,
    }

    with pytest.raises(TypeError):
        _build_kwargs(("cats", 5), {}, ["query", "max_results"], 1)


def test_generated_client_defaulted_params_sync():
    from baml_core import UNSET
    from baml_sdk.lorem import default_score, mutate_default

    assert default_score("cats") == 10
    assert default_score("cats", max_results=5) == 5
    assert default_score(query="cats", filter="recent") == 110
    assert default_score("cats", filter=None) == 10
    assert default_score("cats", max_results=UNSET, filter="recent") == 110

    with pytest.raises(TypeError):
        default_score("cats", 5)

    assert mutate_default() == 1
    assert mutate_default() == 1
    assert mutate_default(items=[1, 2]) == 3


@pytest.mark.asyncio
async def test_generated_client_defaulted_params_async():
    from baml_core import UNSET
    from baml_sdk.lorem import default_score_async, mutate_default_async

    assert await default_score_async("cats") == 10
    assert await default_score_async("cats", max_results=5) == 5
    assert await default_score_async(query="cats", filter="recent") == 110
    assert await default_score_async("cats", filter=None) == 10
    assert await default_score_async("cats", max_results=UNSET, filter="recent") == 110

    with pytest.raises(TypeError):
        await default_score_async("cats", 5)

    assert await mutate_default_async() == 1
    assert await mutate_default_async() == 1
    assert await mutate_default_async(items=[1, 2]) == 3


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
