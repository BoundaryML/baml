"""Phase-3 sim test: prove the three-arg factory contract works end-to-end.

Imports the hand-rolled `sim/baml_sdk` tree (stand-in for `baml generate`
output) and verifies that calling
`ns_lorem.add_three_to_field_a({"a": 5})` returns a value whose `a`
field is `8` — i.e. the call actually goes through `baml.baml_core` →
PyO3 → bridge_cffi → bex_engine and back.

Dict-in / dict-out exercises the Map→Instance coercion at the project
boundary (bex_project::bex::coerce_arg_to_declared_type) so the host
side can stay dict-shaped; Pydantic class round-tripping lands later.
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
        BamlHandle,
        BamlRuntime,
        UnknownHandle,
        define_function,
        get_runtime,
    )

    assert BamlHandle is not None
    assert BamlRuntime is not None
    assert UnknownHandle is not None
    assert callable(define_function)
    assert callable(get_runtime)


def test_factory_roundtrip_sync():
    """`ns_lorem.add_three_to_field_a({"a": 5})` → value with `a == 8`."""
    import baml_sdk

    result = baml_sdk.ns_lorem.add_three_to_field_a({"a": 5})
    assert result["a"] == 8, f"expected a=8, got {result!r}"


@pytest.mark.asyncio
async def test_factory_roundtrip_async():
    """Async factory variant also round-trips."""
    import baml_sdk

    result = await baml_sdk.ns_lorem.add_three_to_field_a_async({"a": 10})
    assert result["a"] == 13, f"expected a=13, got {result!r}"
