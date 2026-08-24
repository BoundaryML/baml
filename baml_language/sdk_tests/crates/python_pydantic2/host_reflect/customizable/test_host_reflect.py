from __future__ import annotations

import pickle
from typing import Optional

import pytest

from baml_bridge import BamlError, BamlType
from baml_sdk import HostParse, HostTypeEqual, HostTypeName, StaticNamed, StaticRecord, reflect


# SDK_PARITY_LINT(skip): BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go
def test_runtime_enum_definition_decodes_alias():
    red = reflect.enum.value("RED", alias="k7", description="warm")
    category = reflect.enum.new("Category", [red, "BLUE"])

    assert isinstance(category, BamlType)
    assert HostParse('"k7"', _types={"T": category}) == "RED"


# SDK_PARITY_LINT(skip): BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go
def test_runtime_class_definition_preserves_nested_metadata():
    category = reflect.enum.new("Category", [reflect.enum.value("RED", alias="k7")])
    record = reflect.class_.new(
        "RuntimeRecord",
        {
            "label": reflect.Type.of(str).meta(alias="display_label"),
            "category": category,
            "scores": reflect.Type.of(int).array(),
        },
    )

    parsed = HostParse(
        '{"display_label":"ok","category":"k7","scores":[1,2]}',
        _types={"T": record},
    )
    assert parsed == {"label": "ok", "category": "RED", "scores": [1, 2]}


# SDK_PARITY_LINT(skip): BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go
def test_compiled_package_returns_class_graph():
    package = reflect.Package.compile(
        {"runtime.baml": "class CompiledRow { amount int note string? }"}
    )
    compiled = package.get_class("CompiledRow")
    assert isinstance(compiled, BamlType)
    assert HostParse('{"amount":7}', _types={"T": compiled}) == {
        "amount": 7,
        "note": None,
    }


# SDK_PARITY_LINT(skip): BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go
def test_wire_occurrences_are_fresh_and_handles_reject_serialization():
    runtime_type = reflect.class_.new("Fresh", {"value": reflect.Type.of(int)})
    assert HostTypeEqual(_types={"A": runtime_type, "B": runtime_type}) is False
    with pytest.raises(TypeError, match="cannot be serialized"):
        pickle.dumps(runtime_type)


# SDK_PARITY_LINT(skip): BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go
def test_known_type_tokens_compose_and_reject_unknowns():
    assert HostTypeName(_types={"T": int}) == "int"
    assert HostTypeName(_types={"T": list[Optional[str]]}) == "(string | null)[]"
    assert HostTypeName(_types={"T": StaticNamed}) == "StaticNamed"
    assert reflect.Type.of(int).optional().array()
    assert hasattr(reflect, "class_") and not hasattr(reflect, "class")
    assert hasattr(reflect, "Type") and not hasattr(reflect, "type_")

    class NotGenerated:
        pass

    with pytest.raises(TypeError, match="unsupported Python type token"):
        HostTypeName(_types={"T": NotGenerated})


# SDK_PARITY_LINT(skip): Python and TypeScript have generated-class subclass tokens; Go has no subclass construct
def test_generated_class_subclasses_resolve_to_declared_type():
    class ChildRecord(StaticRecord):
        pass

    assert HostTypeName(_types={"T": ChildRecord}) == "StaticRecord"


# SDK_PARITY_LINT(skip): BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go
def test_host_handles_expose_composition_only():
    runtime_type = reflect.Type.of(int)
    assert not hasattr(runtime_type, "kind")
    assert not hasattr(runtime_type, "fields")
    assert not hasattr(runtime_type, "as_type")


# SDK_PARITY_LINT(skip): BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go
def test_reflection_compile_errors_are_typed():
    with pytest.raises(BamlError) as exc_info:
        reflect.Package.compile({"broken.baml": "class {"})

    assert exc_info.value.class_name == "reflect.errors.CompilationError"
