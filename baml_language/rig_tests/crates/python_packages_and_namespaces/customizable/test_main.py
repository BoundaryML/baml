"""Packages and namespaces from BEP-030."""


def test_root_user_type_importable():
    from baml_sdk.baml_types import Resume
    assert Resume is not None


def test_user_namespace_via_submodule():
    from baml_sdk.baml_types.foo import Sentiment
    assert Sentiment is not None


def test_user_namespace_via_attribute_access():
    from baml_sdk import baml_types
    assert baml_types.foo.Sentiment is not None


def test_vendor_package_at_top_level():
    # `other.foo.Address` in BAML -> `baml_types.other.foo.Address` externally.
    from baml_sdk.baml_types.other.foo import Address
    from baml_sdk import baml_types
    assert baml_types.other.foo.Address is Address


def test_baml_package():
    from baml_sdk.baml_types.baml.http import Request
    from baml_sdk import baml_types
    assert baml_types.baml.http.Request is Request


def test_stream_types_mirrors_structure():
    import baml_sdk.baml_stream_types as st
    assert st is not None
