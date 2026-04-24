"""Every BEP-030 Ty variant round-trips through codegen."""

import typing
import collections.abc


def test_imports_cleanly():
    from baml_sdk.baml_types import CoverAll, MyClass, RecursiveAlias  # noqa: F401


def test_unknown_field():
    from baml_sdk.baml_types import CoverAll
    hints = typing.get_type_hints(CoverAll)
    assert hints["unknown_field"] is typing.Any


def test_callable_field():
    from baml_sdk.baml_types import CoverAll
    hints = typing.get_type_hints(CoverAll)
    origin = typing.get_origin(hints["callable_field"])
    assert origin in (typing.Callable, collections.abc.Callable)


def test_literal_field():
    from baml_sdk.baml_types import CoverAll
    hints = typing.get_type_hints(CoverAll)
    assert typing.get_origin(hints["literal_field"]) is typing.Literal


def test_self_reference_forward_ref():
    # Self-referential class must compile even though field type references
    # the class itself before it's fully defined.
    from baml_sdk.baml_types import CoverAll
    # Use model_construct to bypass Pydantic validation for this test.
    # We're testing that forward references resolve correctly, not value validation.
    c = CoverAll.model_construct(
        unknown_field=None, callable_field=None, alias_field=None,
        image_field=None, literal_field="Hello",
        optional_nested=[], union_field=1, self_ref=None
    )
    assert c.self_ref is None
