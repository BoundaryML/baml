"""A generic class the runtime keeps to itself (`baml.sap._ParseCache<T>`): an
instance crosses as a reference that carries its type arguments, so its
methods bind `T` from the receiver, and one engine instance is one reference.
"""

import pytest

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_bridge.proto import receiver_type_args
from baml_sdk.baml.sap import _ParseCache, _new_parse_cache
from baml_sdk.generic_tests import identity


@pytest.mark.asyncio
async def test_generic_references_method_binds_the_class_type_argument_from_its_receiver():
    ints = _new_parse_cache(_types={"T": int})
    assert isinstance(ints, _ParseCache)
    assert ints._parse_final_("41") == 41
    assert await ints._parse_final__async("41") == 41
    # The same text read at another type argument: the binding is the
    # receiver's, not a default.
    strings = _new_parse_cache(_types={"T": str})
    assert strings._parse_final_("41") == "41"


def test_generic_references_reference_carries_its_type_arguments():
    # The type argument a reference came with binds another call: a cache
    # built at the strings cache's `T` reads text as a string.
    [t] = receiver_type_args(_new_parse_cache(_types={"T": str}))
    assert _new_parse_cache(_types={"T": t})._parse_final_("41") == "41"


def test_generic_references_one_engine_instance_is_one_reference():
    cache = _new_parse_cache(_types={"T": int})
    assert identity(cache) == cache
    assert _new_parse_cache(_types={"T": int}) != cache
