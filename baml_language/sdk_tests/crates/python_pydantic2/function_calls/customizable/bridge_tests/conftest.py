"""Fixtures for the tests of the Python bridge package (`baml_bridge`)."""

import pytest

from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map


@pytest.fixture
def no_generated_classes():
    """The type map of a process that has no generated `baml_sdk`: the bridge
    alone has a Python class for no BAML class. The generated type map of the
    fixture comes back after the test."""
    generated = get_type_map()
    set_type_map(BamlTypeMap())
    try:
        yield
    finally:
        set_type_map(generated)
