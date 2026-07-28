import importlib


def test_type_alias_declared_before_classes_is_importable():
    alias_order = importlib.import_module("baml_sdk.alias_order")
    importlib.import_module("baml_sdk.stream_types.alias_order")
    result = alias_order.UsesResult(value=alias_order.Success())
    assert isinstance(result.value, alias_order.Success)
