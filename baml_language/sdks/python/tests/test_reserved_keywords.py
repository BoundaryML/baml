"""Reserved-keyword wire-encoding coverage for issue #4059.

Companion to the pydantic2 generator escape (which renames keyword
identifiers on the Python side, e.g. field `pass` -> `pass_`, enum member
`None` -> `None_`) and the `reserved_keywords` sdk-test fixture (which
exercises the full generate + import + round-trip). These tests pin the
*bridge* half: an escaped class field or enum member must go on the wire
under its raw BAML name, not the escaped Python identifier, so the engine
still recognizes it.

They drive the pure-Python `baml_bridge.proto` encode helpers with
hand-rolled models/enums, mirroring `test_proto_generics.py`. No runtime
or typemap is initialized: the encode branches tolerate an empty typemap
(`py_type_to_baml_type` returns "" for the informational FQN field), and
what we assert is the field key / enum value placed on the wire.
"""

from __future__ import annotations

import enum
import keyword

import pydantic

from baml_bridge.proto import _set_inbound_value
from baml_bridge.cffi.v1 import baml_inbound_pb2


# The 35 CPython hard keywords the generator escapes. MUST stay in lockstep
# with `PYTHON_HARD_KEYWORDS` in
# sdks/python/rust/sdkgen_python_pydantic2/src/emit/mod.rs (whose own Rust
# test `python_hard_keyword_set_is_exactly_the_35_cpython_hard_keywords`
# asserts the same 35). `keyword.kwlist` is the cross-language anchor.
_EXPECTED_HARD_KEYWORDS = frozenset(
    {
        "False", "None", "True", "and", "as", "assert", "async", "await",
        "break", "class", "continue", "def", "del", "elif", "else", "except",
        "finally", "for", "from", "global", "if", "import", "in", "is",
        "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try",
        "while", "with", "yield",
    }
)


def test_rust_keyword_list_matches_python_kwlist():
    """Cross-check the generator's escape set against CPython's own list.

    Closes the loop the Rust-side test opens: Rust asserts its const equals
    these 35; this asserts these 35 equal `keyword.kwlist`; together they
    prove the Rust escape set equals `keyword.kwlist`.
    """
    assert set(keyword.kwlist) == _EXPECTED_HARD_KEYWORDS
    assert len(_EXPECTED_HARD_KEYWORDS) == 35
    # Soft keywords are valid identifiers and must NEVER be escaped.
    assert set(keyword.softkwlist) == {"_", "case", "match", "type"}
    assert _EXPECTED_HARD_KEYWORDS.isdisjoint(keyword.softkwlist)


class _EscapedFieldModel(pydantic.BaseModel):
    """Mirrors generated codegen for a class with a keyword field `pass`."""

    model_config = pydantic.ConfigDict(extra="forbid", populate_by_name=True)
    pass_: int = pydantic.Field(alias="pass")


class _PlainModel(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    value: int


class _KeywordEnum(str, enum.Enum):
    """Mirrors generated codegen for an enum with a keyword member `None`."""

    None_ = "None"
    Ok = "Ok"


def test_escaped_class_field_goes_on_wire_under_raw_baml_name():
    inbound = baml_inbound_pb2.InboundValue()
    _set_inbound_value(inbound, _EscapedFieldModel(**{"pass": 7}), kwarg_name="x")
    assert inbound.WhichOneof("value") == "class_value"
    cv = inbound.class_value
    assert len(cv.fields) == 1
    # Raw BAML name `pass` on the wire, not the escaped attribute `pass_`.
    assert cv.fields[0].string_key == "pass"
    assert cv.fields[0].value.int_value == 7


def test_non_escaped_field_wire_key_unchanged():
    inbound = baml_inbound_pb2.InboundValue()
    _set_inbound_value(inbound, _PlainModel(value=3), kwarg_name="x")
    cv = inbound.class_value
    assert cv.fields[0].string_key == "value"
    assert cv.fields[0].value.int_value == 3


def test_escaped_enum_member_encodes_by_raw_value():
    inbound = baml_inbound_pb2.InboundValue()
    _set_inbound_value(inbound, _KeywordEnum.None_, kwarg_name="x")
    assert inbound.WhichOneof("value") == "enum_value"
    # Raw BAML variant `None` on the wire, not the Python member name `None_`.
    assert inbound.enum_value.value == "None"


def test_non_escaped_enum_member_wire_value_unchanged():
    inbound = baml_inbound_pb2.InboundValue()
    _set_inbound_value(inbound, _KeywordEnum.Ok, kwarg_name="x")
    assert inbound.enum_value.value == "Ok"


def test_escaped_enum_map_key_encodes_by_raw_value():
    inbound = baml_inbound_pb2.InboundValue()
    _set_inbound_value(inbound, {_KeywordEnum.None_: 1}, kwarg_name="x")
    assert inbound.WhichOneof("value") == "map_value"
    entry = inbound.map_value.entries[0]
    assert entry.enum_key.value == "None"
    assert entry.value.int_value == 1
