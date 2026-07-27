import baml_sdk
from baml_bridge import BamlRuntime, call_function_sync


def _runtime_source(value: str) -> str:
    return f"""
function RuntimeRegistryValue() -> string {{
    "{value}"
}}
"""


# SDK_PARITY_LINT(skip): exercises Python bridge runtime-construction APIs
def test_runtime_instances_remain_independent():
    runtime_a = BamlRuntime.initialize_runtime(
        ".", {"runtime_a.baml": _runtime_source("runtime-a")}
    )
    runtime_b = BamlRuntime.initialize_runtime(
        ".", {"runtime_b.baml": _runtime_source("runtime-b")}
    )

    assert runtime_a.runtime_key != runtime_b.runtime_key
    assert call_function_sync(runtime_a, "RuntimeRegistryValue", {}).result() == "runtime-a"
    assert call_function_sync(runtime_b, "RuntimeRegistryValue", {}).result() == "runtime-b"
    assert call_function_sync(runtime_a, "RuntimeRegistryValue", {}).result() == "runtime-a"


# SDK_PARITY_LINT(skip): exercises Python bridge runtime-construction APIs
def test_generated_sdk_keeps_using_runtime_zero():
    BamlRuntime.initialize_runtime(
        ".", {"dynamic.baml": _runtime_source("dynamic")}
    )

    assert baml_sdk.hello_world() == "hello world"
