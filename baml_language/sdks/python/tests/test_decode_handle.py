"""Decoder-side tests for `BamlPyHandle` round-tripping.

These tests exercise `_decode_handle` with handle kinds that have no
Python-facing constructor (FunctionRef, ADT_MEDIA_GENERIC). The
HANDLE_TABLE entries are seeded via the `_seed_*_handle` PyO3 helpers,
which return `(key, handle_type)` so tests can stage a wire
`BamlHandle` and exercise the decoder dispatch.
"""

from __future__ import annotations

import typing

import pytest

from baml_core import BamlPyHandle
from baml_core.baml_py import (
    _seed_function_ref_handle,
    _seed_generic_media_handle,
)
from baml_core.proto import _decode_handle
from baml_core.typemap import BamlTypeMap
from baml_core.cffi.v1 import baml_inbound_pb2


def _make_handle(key: int, handle_type: int) -> "baml_inbound_pb2.BamlHandle":
    h = baml_inbound_pb2.BamlHandle()
    h.key = key
    # `BamlHandleType` is an `int` subclass at runtime; the proto field
    # accepts bare ints. Cast for the static checker.
    h.handle_type = typing.cast(baml_inbound_pb2.BamlHandleType, handle_type)
    return h


def test_function_ref_decodes_to_pyhandle():
    key, ht = _seed_function_ref_handle(123)
    result = _decode_handle(_make_handle(key, ht), BamlTypeMap())
    assert isinstance(result, BamlPyHandle)


def test_adt_media_generic_decodes_to_pyhandle():
    key, ht = _seed_generic_media_handle()
    result = _decode_handle(_make_handle(key, ht), BamlTypeMap())
    assert isinstance(result, BamlPyHandle)


def test_decoded_pyhandle_releases_on_drop():
    """Dropping a `BamlPyHandle` removes its row from `HANDLE_TABLE` —
    a subsequent decode of the same key fails because the entry is gone.
    """
    key, ht = _seed_function_ref_handle(7)
    pyh = _decode_handle(_make_handle(key, ht), BamlTypeMap())
    assert isinstance(pyh, BamlPyHandle)
    del pyh  # CPython refcount drops to 0; Drop runs HANDLE_TABLE.release.
    with pytest.raises(RuntimeError, match="not in HANDLE_TABLE"):
        _decode_handle(_make_handle(key, ht), BamlTypeMap())
