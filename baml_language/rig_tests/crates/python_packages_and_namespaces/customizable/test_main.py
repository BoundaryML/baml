"""Packages and namespaces from BEP-030.

Post-phase-6 layout (09a §1 / 10g2 §3.1):
- `(user, [], Foo)`           → `baml_sdk.Foo`
- `(user, [ns], Foo)`         → `baml_sdk.<ns>.Foo`
- `(<pkg>, [ns], Foo)`        → `baml_sdk.vendor.<pkg>.<ns>.Foo`
- `(baml, [ns], Foo)`         → `baml_sdk.baml.<ns>.Foo`
- `$stream` companions        → `baml_sdk.stream_types.<…>.Foo` (only
                                 when any `$stream` symbol is present
                                 in the pool)
"""


def test_root_user_type_importable():
    from baml_sdk import Resume
    assert Resume is not None


def test_user_namespace_via_submodule():
    from baml_sdk.foo import Sentiment
    assert Sentiment is not None


def test_user_namespace_via_attribute_access():
    from baml_sdk import foo
    assert foo.Sentiment is not None


def test_vendor_package_at_top_level():
    # Non-`user`, non-`baml` pkg → `baml_sdk.vendor.<pkg>.<ns>.*`.
    from baml_sdk.vendor.other.foo import Address
    from baml_sdk import vendor
    assert vendor.other.foo.Address is Address


def test_baml_package():
    from baml_sdk.baml.http import Request
    from baml_sdk import baml
    assert baml.http.Request is Request
