"""Decoder-side tests for `BamlPyHandle` round-tripping (15j Phase 8).

These tests exercise `_decode_handle` with handle kinds that have no
Python-facing constructor (FunctionRef, ADT_MEDIA_GENERIC). The
HANDLE_TABLE entries are seeded via the `_seed_*_handle` PyO3 helpers.
"""

from __future__ import annotations

from baml.baml_core import BamlPyHandle
from baml.baml_core.baml_py import (
    _seed_function_ref_handle,
    _seed_generic_media_handle,
)
from baml.baml_core.proto import _decode_handle
from baml.cffi.v1 import baml_inbound_pb2


def _make_handle(key: int) -> "baml_inbound_pb2.BamlHandle":
    h = baml_inbound_pb2.BamlHandle()
    h.key = key
    return h


def test_function_ref_decodes_to_pyhandle():
    key = _seed_function_ref_handle(123)
    result = _decode_handle(_make_handle(key))
    assert isinstance(result, BamlPyHandle)
    assert result.handle_type() == baml_inbound_pb2.BamlHandleType.FUNCTION_REF


def test_adt_media_generic_decodes_to_pyhandle():
    key = _seed_generic_media_handle()
    result = _decode_handle(_make_handle(key))
    assert isinstance(result, BamlPyHandle)
    assert (
        result.handle_type() == baml_inbound_pb2.BamlHandleType.ADT_MEDIA_GENERIC
    )


def test_decode_handle_drains_the_table_entry():
    """A second drain on the same key fails — the first decode removed it."""
    import pytest

    key = _seed_function_ref_handle(7)
    first = _decode_handle(_make_handle(key))
    assert isinstance(first, BamlPyHandle)
    with pytest.raises(RuntimeError, match="not in HANDLE_TABLE"):
        _decode_handle(_make_handle(key))
