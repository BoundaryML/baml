from __future__ import annotations

import pickle
from typing import Optional

import pytest

from baml_bridge import BamlType
from baml_sdk import HostParse, HostTypeEqual, HostTypeName, StaticNamed, StaticRecord, reflect


def test_runtime_enum_definition_crosses_and_decodes_loosely() -> None:
    red = reflect.enum.value("RED", alias="k7", description="warm")
    category = reflect.enum.new("Category", [red, "BLUE"])

    assert isinstance(category, BamlType)
    assert HostParse('"k7"', _types={"T": category}) == "RED"


def test_runtime_class_nested_definition_metadata_and_loose_mapping() -> None:
    category = reflect.enum.new("Category", [reflect.enum.value("RED", alias="k7")])
    record = reflect.class_.new(
        "RuntimeRecord",
        {
            "label": reflect.type_.of(str).meta(alias="display_label"),
            "category": category,
            "scores": reflect.type_.of(int).array(),
        },
    )

    parsed = HostParse(
        '{"display_label":"ok","category":"k7","scores":[1,2]}',
        _types={"T": record},
    )
    assert parsed == {"label": "ok", "category": "RED", "scores": [1, 2]}


def test_package_compile_class_crosses_with_definition_graph() -> None:
    package = reflect.Package.compile(
        {"runtime.baml": "class CompiledRow { amount int note string? }"}
    )
    compiled = package.get_class("CompiledRow")
    assert isinstance(compiled, BamlType)
    assert HostParse('{"amount":7}', _types={"T": compiled}) == {
        "amount": 7,
        "note": None,
    }


def test_each_wire_occurrence_is_fresh_and_handles_are_not_serializable() -> None:
    runtime_type = reflect.class_.new("Fresh", {"value": reflect.type_.of(int)})
    assert HostTypeEqual(_types={"A": runtime_type, "B": runtime_type}) is False
    with pytest.raises(TypeError, match="cannot be serialized"):
        pickle.dumps(runtime_type)


def test_python_token_rules_keywords_subclasses_and_typing_recursion() -> None:
    class ChildRecord(StaticRecord):
        pass

    assert HostTypeName(_types={"T": int}) == "int"
    assert HostTypeName(_types={"T": list[Optional[str]]}) == "(string | null)[]"
    assert HostTypeName(_types={"T": ChildRecord}) == "StaticRecord"
    assert HostTypeName(_types={"T": StaticNamed}) == "StaticNamed"
    assert reflect.type_.of(int).optional().array()
    assert hasattr(reflect, "class_") and not hasattr(reflect, "class")
    assert hasattr(reflect, "type_") and not hasattr(reflect, "type")

    class NotGenerated:
        pass

    with pytest.raises(TypeError, match="unsupported Python type token"):
        HostTypeName(_types={"T": NotGenerated})


def test_host_handle_has_only_composition_surface() -> None:
    runtime_type = reflect.type_.of(int)
    assert not hasattr(runtime_type, "kind")
    assert not hasattr(runtime_type, "fields")
    assert not hasattr(runtime_type, "as_type")
