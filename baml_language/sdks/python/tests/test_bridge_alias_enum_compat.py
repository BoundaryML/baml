"""Bridge wire-identity coverage for the generator-provenance marker fix.

Companion to `test_reserved_keywords.py`. The bridge encoder decides the wire
name of a class field / enum member from a single, explicit signal: a generated
`__baml_wire_names__` (classes) / `__baml_wire_values__` (enums) marker that the
codegen stamps on an escaped artifact only. There is NO shape/alias/value
heuristic anymore (`_is_keyword_escape_of` is deleted). The rule is:

* a name listed in the marker wires its raw BAML name (the keyword the generator
  escaped, e.g. attr `None_` -> `None`, member `None_` -> `None`), and
* a name with no marker entry wires its own attribute / member NAME, which is
  exactly the pre-PR bridge behavior, so an ordinary hand-written model or custom
  enum, which carries no marker, encodes as it always did; a hand-written type
  that declares a well-formed marker, or inherits one via the MRO, is honored on
  the same lookup.

This file pins that contract from both sides:

* MARKER PATH, a generated-shape class/enum carrying the marker wires the raw
  BAML name (single- and collision-bumped).
* FALLBACK PATH (marker absent), hand-written models/enums, arbitrary user
  aliases, custom str/int enums, and an ordinary BAML
  `None_` identity the old shape heuristic misread as an escape, all wire their
  attribute / member name. The int-subclass enum value whose `str()` is a
  keyword can no longer TypeError because `.value` is never forwarded.
* MARKER HYGIENE, the markers do not pollute the Enum member set, the pydantic
  field set, or the JSON schema, and they ride the MRO to user subclasses.

Like `test_reserved_keywords.py`, these drive the pure-Python `baml_bridge.proto`
encode helpers with hand-rolled models/enums, no runtime or typemap is
initialized. Generated-STYLE fixtures declare the dunder markers by hand exactly
as the generator emits them; this file does not depend on the
generator emission itself.
"""

from __future__ import annotations

import enum
import typing

import pydantic
import pytest

from baml_bridge.proto import _set_inbound_value
from baml_bridge.cffi.v1 import baml_inbound_pb2


def _encode(value):
    inbound = baml_inbound_pb2.InboundValue()
    _set_inbound_value(inbound, value, kwarg_name="x")
    return inbound


# ---------------------------------------------------------------------------
# Fixtures, GENERATED-SHAPE (carry the dunder marker) vs HAND-WRITTEN (no marker)
# ---------------------------------------------------------------------------


class _MarkedEscapedFieldModel(pydantic.BaseModel):
    """Generated shape for `class M { pass int }`: keyword field `pass` renamed to
    attr `pass_` (alias `pass` for decode) and stamped with the wire-name marker."""

    model_config = pydantic.ConfigDict(extra="forbid", populate_by_name=True)
    __baml_wire_names__ = {"pass_": "pass"}
    pass_: int = pydantic.Field(alias="pass")


class _MarkedTwinFieldModel(pydantic.BaseModel):
    """Generated shape for `class C { None, None_ }`: the keyword field `None`
    collides with a sibling `None_`, so the collision-aware escape double-bumps
    its attribute to `None__` (marker -> raw `None`); the ordinary sibling `None_`
    has no marker entry."""

    model_config = pydantic.ConfigDict(extra="forbid", populate_by_name=True)
    __baml_wire_names__ = {"None__": "None"}
    None__: int = pydantic.Field(alias="None")
    None_: int


class _MarkedKeywordEnum(str, enum.Enum):
    """Generated shape for `enum E { None, Ok }`: escaped member `None_` carries a
    `__baml_wire_values__` marker; the non-keyword member `Ok` has no entry."""

    __baml_wire_values__ = {"None_": "None"}
    None_ = "None"
    Ok = "Ok"


class _MarkedTwinEnum(str, enum.Enum):
    """Generated shape for `enum E { None, None_ }`: keyword member `None`
    double-bumps to `None__` (marker -> raw `None`); the ordinary sibling `None_`
    has no marker entry and keeps its own name."""

    __baml_wire_values__ = {"None__": "None"}
    None__ = "None"
    None_ = "None_"


class _HandwrittenNoneAliasedModel(pydantic.BaseModel):
    """HAND-WRITTEN model for BAML `class C { None_ int }`. The ordinary BAML
    identifier `None_` maps to Python attr `None_`; the user attaches
    `Field(alias="None")` for their own reasons. NO marker (not generated), so the
    bridge must wire the attribute name `None_`, the pre-PR identity. This is the
    exact runtime shape the deleted shape heuristic misread as a keyword escape and
    wired as `None`, breaking the call."""

    model_config = pydantic.ConfigDict(extra="forbid", populate_by_name=True)
    None_: int = pydantic.Field(alias="None")


class _HandwrittenNoneValuedEnum(str, enum.Enum):
    """HAND-WRITTEN enum for BAML `enum E { None_ }`: member `None_` with value
    `"None"`, NO marker. The bridge must wire the member name `None_` (pre-PR
    identity), not the value `"None"` (the enum half)."""

    None_ = "None"


class _GeneratedModel(pydantic.BaseModel):
    """Codegen shape for `class Model { value int }`, no escaped field, no marker."""

    model_config = pydantic.ConfigDict(extra="forbid")
    value: int


class _UserAliasedModel(_GeneratedModel):
    """User subclass that attaches an ARBITRARY (non-keyword) field alias, a
    general Pydantic feature unrelated to keyword escaping, and NO marker. Pre-PR
    the bridge wired the attribute name `value`; the marker-absent fallback
    restores exactly that."""

    model_config = pydantic.ConfigDict(extra="forbid", populate_by_name=True)
    value: int = pydantic.Field(alias="wire_label")


class _UserSubclassOfMarked(_MarkedEscapedFieldModel):
    """A user subclass of a marked generated model. It inherits
    `__baml_wire_names__` through the MRO, so the escaped field still wires under
    its raw BAML name while a newly-added plain field wires under its own name."""

    extra_field: int = 0


class _StringWireEnum(str, enum.Enum):
    """A custom str enum whose member VALUE diverges from its NAME, NO marker.
    Pre-PR the bridge wired the member name `Foo`; the fallback restores that."""

    Foo = "wire_label"


class _IntegerWireEnum(int, enum.Enum):
    """An int-backed enum, NO marker. Pre-PR the bridge wired the member name
    `Foo`; the fallback restores that and never assigns a non-str to the wire."""

    Foo = 7


class KeywordStringifyingInt(int):
    """An int subclass whose `str()` is a Python hard keyword. Exercises the
    non-str enum-value path: the deleted shape gate evaluated `str(value.value)`
    and, on the keyword match, forwarded the non-str `value.value` into the
    protobuf string field, raising `TypeError`. The marker path never touches
    `.value`, so the member NAME is sent and the TypeError path ceases to exist."""

    def __str__(self) -> str:
        return "pass"


class _StringifyingIntValueEnum(enum.Enum):
    """Custom enum whose member value is a `KeywordStringifyingInt`; NO marker."""

    pass_ = KeywordStringifyingInt(7)


# ---------------------------------------------------------------------------
# Regression pinned: hand-written `None_`/alias-`None` keeps pre-PR
# identity (wires `None_`). The hand-written regression, as a test.
# ---------------------------------------------------------------------------


def test_handwritten_none_field_aliased_none_wires_attribute_name():
    inbound = _encode(_HandwrittenNoneAliasedModel(None_=5))
    assert inbound.WhichOneof("value") == "class_value"
    cv = inbound.class_value
    assert len(cv.fields) == 1
    # Marker absent -> attribute name `None_`, NOT the alias `None`.
    assert cv.fields[0].string_key == "None_"
    assert cv.fields[0].value.int_value == 5


def test_handwritten_none_enum_scalar_wires_member_name():
    inbound = _encode(_HandwrittenNoneValuedEnum.None_)
    assert inbound.WhichOneof("value") == "enum_value"
    # Marker absent -> member name `None_`, NOT the value `None`.
    assert inbound.enum_value.value == "None_"


def test_handwritten_none_enum_map_key_wires_member_name():
    inbound = _encode({_HandwrittenNoneValuedEnum.None_: 1})
    assert inbound.WhichOneof("value") == "map_value"
    entry = inbound.map_value.entries[0]
    assert entry.WhichOneof("key") == "enum_key"
    assert entry.enum_key.value == "None_"
    assert entry.value.int_value == 1


# ---------------------------------------------------------------------------
# Marker path: a marked generated artifact wires its raw BAML name,
# single- and collision-bumped, across the class + both enum sites.
# ---------------------------------------------------------------------------


def test_marker_class_field_wires_raw_baml_name():
    inbound = _encode(_MarkedEscapedFieldModel(**{"pass": 7}))
    cv = inbound.class_value
    assert len(cv.fields) == 1
    # Marker `{"pass_": "pass"}` -> raw BAML name `pass`.
    assert cv.fields[0].string_key == "pass"
    assert cv.fields[0].value.int_value == 7


def test_marker_twin_class_field_multibump_wires_raw_baml_name():
    inbound = _encode(_MarkedTwinFieldModel(None__=7, None_=8))
    cv = inbound.class_value
    keys = {f.string_key: f.value.int_value for f in cv.fields}
    # Double-bumped attr `None__` -> raw `None` via marker; the Python identifier
    # `None__` must never reach the wire.
    assert keys["None"] == 7
    assert "None__" not in keys
    # Sibling `None_` has no marker entry -> its own name.
    assert keys["None_"] == 8


def test_marker_enum_member_scalar_wires_raw_baml_name():
    inbound = _encode(_MarkedKeywordEnum.None_)
    assert inbound.WhichOneof("value") == "enum_value"
    # Marker `{"None_": "None"}` -> raw BAML variant `None`.
    assert inbound.enum_value.value == "None"


def test_marker_enum_member_map_key_wires_raw_baml_name():
    inbound = _encode({_MarkedKeywordEnum.None_: 1})
    entry = inbound.map_value.entries[0]
    assert entry.WhichOneof("key") == "enum_key"
    assert entry.enum_key.value == "None"
    assert entry.value.int_value == 1


def test_marker_twin_enum_member_scalar_wires_raw_baml_name():
    inbound = _encode(_MarkedTwinEnum.None__)
    assert inbound.WhichOneof("value") == "enum_value"
    # Double-bumped member `None__` -> raw `None` via marker.
    assert inbound.enum_value.value == "None"


def test_marker_twin_enum_member_map_key_wires_raw_baml_name():
    inbound = _encode({_MarkedTwinEnum.None__: 1})
    entry = inbound.map_value.entries[0]
    assert entry.WhichOneof("key") == "enum_key"
    assert entry.enum_key.value == "None"
    assert entry.value.int_value == 1


# ---------------------------------------------------------------------------
# Regression pinned: an int-subclass enum value whose `str()` is a
# keyword encodes as the member NAME, no TypeError, scalar and map-key.
# ---------------------------------------------------------------------------


def test_keyword_stringifying_int_enum_scalar_wires_member_name():
    inbound = _encode(_StringifyingIntValueEnum.pass_)
    assert inbound.WhichOneof("value") == "enum_value"
    # Member name `pass_`; `.value` (a non-str KeywordStringifyingInt) is never
    # forwarded, so no `TypeError: bad argument type` on the string wire field.
    assert inbound.enum_value.value == "pass_"


def test_keyword_stringifying_int_enum_map_key_wires_member_name():
    inbound = _encode({_StringifyingIntValueEnum.pass_: 9})
    entry = inbound.map_value.entries[0]
    assert entry.WhichOneof("key") == "enum_key"
    assert entry.enum_key.value == "pass_"
    assert entry.value.int_value == 9


# ---------------------------------------------------------------------------
# Marker hygiene: the dunder markers are not Enum members, not pydantic
# fields, and not in the JSON schema.
# ---------------------------------------------------------------------------


def test_wire_values_marker_is_not_enum_member():
    # EnumMeta excludes `__dunder__` names from membership, verified, not assumed.
    assert "__baml_wire_values__" not in _MarkedKeywordEnum.__members__
    assert "__baml_wire_values__" not in {m.name for m in _MarkedKeywordEnum}
    # Still reachable as a plain class attribute for the encoder's getattr.
    assert _MarkedKeywordEnum.__baml_wire_values__ == {"None_": "None"}
    # Membership is exactly the two declared variants.
    assert {m.name for m in _MarkedKeywordEnum} == {"None_", "Ok"}


def test_wire_names_marker_is_not_pydantic_field_or_in_schema():
    assert "__baml_wire_names__" not in _MarkedEscapedFieldModel.model_fields
    schema = _MarkedEscapedFieldModel.model_json_schema()
    assert "__baml_wire_names__" not in schema.get("properties", {})
    # Still reachable as a ClassVar for the encoder's getattr.
    assert _MarkedEscapedFieldModel.__baml_wire_names__ == {"pass_": "pass"}


# ---------------------------------------------------------------------------
# Inheritance: a user subclass of a marked model inherits the marker via
# MRO and still wires the escaped field correctly.
# ---------------------------------------------------------------------------


def test_user_subclass_of_marked_model_wires_escaped_field():
    inbound = _encode(_UserSubclassOfMarked(**{"pass": 7}))
    cv = inbound.class_value
    keys = {f.string_key: f.value.int_value for f in cv.fields}
    # Inherited marker still maps `pass_` -> `pass`.
    assert keys["pass"] == 7
    assert "pass_" not in keys
    # A field added by the subclass with no marker entry wires under its own name.
    assert keys["extra_field"] == 0


# ---------------------------------------------------------------------------
# Non-keyword controls (marker absent OR name not listed) keep pre-PR
# behavior: attribute name for fields, member NAME for enums.
# ---------------------------------------------------------------------------


def test_user_subclass_arbitrary_alias_wires_attribute_name():
    inbound = _encode(_UserAliasedModel(value=7))
    assert inbound.WhichOneof("value") == "class_value"
    cv = inbound.class_value
    assert len(cv.fields) == 1
    # Marker absent -> attribute name `value`, NOT the arbitrary alias `wire_label`.
    assert cv.fields[0].string_key == "value"
    assert cv.fields[0].value.int_value == 7


def test_custom_string_enum_scalar_wires_member_name():
    inbound = _encode(_StringWireEnum.Foo)
    assert inbound.WhichOneof("value") == "enum_value"
    # Member NAME `Foo`, not the diverging value `wire_label`.
    assert inbound.enum_value.value == "Foo"


def test_custom_string_enum_map_key_wires_member_name():
    inbound = _encode({_StringWireEnum.Foo: 9})
    entry = inbound.map_value.entries[0]
    assert entry.WhichOneof("key") == "enum_key"
    assert entry.enum_key.value == "Foo"
    assert entry.value.int_value == 9


def test_integer_backed_enum_scalar_wires_member_name():
    inbound = _encode(_IntegerWireEnum.Foo)
    assert inbound.WhichOneof("value") == "enum_value"
    assert inbound.enum_value.value == "Foo"


def test_integer_backed_enum_map_key_wires_member_name():
    inbound = _encode({_IntegerWireEnum.Foo: 9})
    entry = inbound.map_value.entries[0]
    assert entry.WhichOneof("key") == "enum_key"
    assert entry.enum_key.value == "Foo"
    assert entry.value.int_value == 9


def test_marker_present_but_member_not_listed_wires_member_name():
    # `Ok` is absent from `_MarkedKeywordEnum`'s marker -> its own member name.
    inbound = _encode(_MarkedKeywordEnum.Ok)
    assert inbound.enum_value.value == "Ok"


def test_marker_twin_enum_sibling_not_listed_wires_member_name():
    # `None_` is the ordinary sibling, absent from the twin marker -> member name.
    inbound = _encode(_MarkedTwinEnum.None_)
    assert inbound.enum_value.value == "None_"


# ---------------------------------------------------------------------------
# Defense-in-depth: a `__baml_wire_names__` attribute that is not a dict (for
# example a same-named method that shadowed the marker, or a malformed
# hand-written marker) is ignored rather than `.get`-called, so encode falls
# back to the attribute name instead of raising.
# ---------------------------------------------------------------------------


class _CallableWireNamesModel(pydantic.BaseModel):
    """A model whose `__baml_wire_names__` attribute is a callable rather than the
    marker dict, reproducing the class-creation collision a same-named user method
    would cause (and any malformed hand-written marker). The bridge must treat a
    non-dict marker as absent and wire the attribute name `pass_`."""

    model_config = pydantic.ConfigDict(extra="forbid", populate_by_name=True)
    pass_: int = pydantic.Field(alias="pass")

    def __baml_wire_names__(self):
        return 1


def test_non_dict_wire_names_marker_is_ignored_and_wires_attribute_name():
    inbound = _encode(_CallableWireNamesModel(**{"pass": 7}))
    assert inbound.WhichOneof("value") == "class_value"
    cv = inbound.class_value
    assert len(cv.fields) == 1
    # Callable (non-dict) marker ignored -> attribute name `pass_`, no
    # `AttributeError: 'function' object has no attribute 'get'`.
    assert cv.fields[0].string_key == "pass_"
    assert cv.fields[0].value.int_value == 7


# ---------------------------------------------------------------------------
# A generated leaf that lowers a user `type dict = int` into a module-level
# `dict = int` binding must not carry a bare-builtin `dict[...]` subscript in the
# wire-name marker annotation, or `typing.get_type_hints` on the escaped class
# resolves the subscript against the shadowed `dict` and raises. The generator
# emits a plain, unannotated dunder dict, which is immune.
# ---------------------------------------------------------------------------


def _leaf_module_with_dict_shadow(name: str, marker_line: str):
    """Build and register an importable module mirroring a generated leaf: the
    user alias `type dict = int` lowered to `dict = int`, then an escaped-field
    class carrying the wire-name marker as `marker_line`."""
    import sys
    import types

    source = (
        "from __future__ import annotations\n"
        "import typing\n"
        "import pydantic\n"
        "dict = int\n"
        "class Victim(pydantic.BaseModel):\n"
        "    model_config = pydantic.ConfigDict(extra='forbid', populate_by_name=True)\n"
        f"    {marker_line}\n"
        "    pass_: int = pydantic.Field(alias='pass')\n"
    )
    module = types.ModuleType(name)
    sys.modules[name] = module
    exec(compile(source, f"<{name}>", "exec"), module.__dict__)
    return module


def test_plain_wire_names_marker_survives_get_type_hints_under_dict_shadow():
    import sys

    try:
        plain = _leaf_module_with_dict_shadow(
            "_leaf_probe_plain_marker", "__baml_wire_names__ = {'pass_': 'pass'}"
        )
        # Plain dunder dict carries no annotation, so introspection is unaffected.
        assert typing.get_type_hints(plain.Victim) == {"pass_": int}

        # The abandoned annotated form is exactly what breaks under the shadow:
        # the bare `dict[str, str]` subscript resolves against `dict = int`.
        annotated = _leaf_module_with_dict_shadow(
            "_leaf_probe_annotated_marker",
            "__baml_wire_names__: typing.ClassVar[dict[str, str]] = {'pass_': 'pass'}",
        )
        with pytest.raises(TypeError):
            typing.get_type_hints(annotated.Victim)
    finally:
        sys.modules.pop("_leaf_probe_plain_marker", None)
        sys.modules.pop("_leaf_probe_annotated_marker", None)
