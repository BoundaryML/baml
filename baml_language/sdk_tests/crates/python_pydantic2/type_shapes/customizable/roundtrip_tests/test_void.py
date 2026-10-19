"""Roundtrip coverage for `baml_sdk.void` — `void` return lowers to `None`."""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.unit import no_op


def test_void_no_op():
    assert no_op() is None
