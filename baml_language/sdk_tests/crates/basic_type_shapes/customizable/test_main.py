"""Per-Ty-variant type-shape coverage.

One namespace per construct family; assertions use `typing.get_type_hints`
to pin how each BAML Ty variant renders in Python. Namespaces that only
assert import-reachability today are TODO landing zones for follow-up
coverage (literal values, map shapes, union shapes, cross-class refs).
"""

import typing
import collections.abc


def test_root_imports_cleanly():
    import baml_sdk  # noqa: F401


def test_all_namespaces_reachable():
    import baml_sdk
    # `ns_` prefix on the baml_src/ directory is stripped by
    # baml_project; the Python namespace is what's left.
    for ns in ("ty_variants", "literals", "maps", "complex", "unions"):
        assert hasattr(baml_sdk, ns), f"baml_sdk.{ns} missing"


# ---------- ns_ty_variants: every Ty variant round-trips ----------

def test_ty_variants_imports_cleanly():
    from baml_sdk.ty_variants import CoverAll, MyClass, RecursiveAlias  # noqa: F401


def test_ty_variants_unknown_field():
    from baml_sdk.ty_variants import CoverAll
    hints = typing.get_type_hints(CoverAll)
    assert hints["unknown_field"] is typing.Any


def test_ty_variants_callable_field():
    from baml_sdk.ty_variants import CoverAll
    hints = typing.get_type_hints(CoverAll)
    origin = typing.get_origin(hints["callable_field"])
    assert origin in (typing.Callable, collections.abc.Callable)


def test_ty_variants_literal_field():
    from baml_sdk.ty_variants import CoverAll
    hints = typing.get_type_hints(CoverAll)
    assert typing.get_origin(hints["literal_field"]) is typing.Literal


def test_ty_variants_self_reference_forward_ref():
    # Self-referential class must compile even though field type references
    # the class itself before it's fully defined. Uses model_construct to
    # bypass Pydantic validation for this test — we're testing that forward
    # references resolve correctly, not value validation.
    from baml_sdk.ty_variants import CoverAll
    c = CoverAll.model_construct(
        unknown_field=None,
        callable_field=None,
        alias_field=None,
        literal_field="Hello",
        optional_nested=[],
        union_field=1,
        self_ref=None,
    )
    assert c.self_ref is None


# ---------- ns_literals: string / int / bool Literal shapes ----------

def test_literals_imports_cleanly():
    from baml_sdk.literals import Literals  # noqa: F401


# TODO: multi-value typing.Literal assertions across string / int / bool.


# ---------- ns_maps: map<K, V> shapes ----------

def test_maps_imports_cleanly():
    from baml_sdk.maps import MapContainer  # noqa: F401


# TODO: typing introspection on each map field shape
# (dict[str, int] / dict[str, dict[str, str]] / dict[str, list[str]] / etc.).


# ---------- ns_complex: 30+ classes with cross-refs and deep nesting ----------

def test_complex_imports_cleanly():
    from baml_sdk.complex import KitchenSink, UltraComplex  # noqa: F401


# TODO: cross-class reference resolution + deep-nesting smoke tests
# + stable-ordering (alphabetical by .baml source filename) assertions.


# ---------- ns_unions: primitive + class union arms ----------

def test_unions_imports_cleanly():
    from baml_sdk.unions import Container, User, Company  # noqa: F401


# TODO: typing introspection on typing.Union / typing.Optional shapes.
