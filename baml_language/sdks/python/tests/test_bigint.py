"""Round-trip tests for the `bigint` primitive through the Python SDK.

These exercise the host-SDK encode/decode added in Phase 11 of the bigint
plan: the wire format is hex (base sixteen) via `bigint_value`, and Python
`int` is arbitrary-precision so the encoder routes values outside i64 range
through `bigint_value`.
"""

from baml_core import BamlRuntime, call_function_sync, call_function


BIGINT_BAML = """\
function EchoBigint(x: bigint) -> bigint {
    x
}
"""


def make_runtime(baml_source: str) -> BamlRuntime:
    return BamlRuntime.initialize_runtime(
        ".", {"main.baml": baml_source}, sdk_root="__bridge_python_tests__"
    )


class TestBigintRoundTripSync:
    def test_small_positive(self):
        rt = make_runtime(BIGINT_BAML)
        result = call_function_sync(rt, "EchoBigint", {"x": 42})
        assert result.result() == 42
        assert isinstance(result.result(), int)

    def test_small_negative(self):
        rt = make_runtime(BIGINT_BAML)
        result = call_function_sync(rt, "EchoBigint", {"x": -42})
        assert result.result() == -42

    def test_large_positive(self):
        """Value outside i64 range — routed through bigint_value."""
        rt = make_runtime(BIGINT_BAML)
        huge = 99999999999999999999  # 20 decimal digits, > 2**63
        result = call_function_sync(rt, "EchoBigint", {"x": huge})
        assert result.result() == huge

    def test_large_negative(self):
        rt = make_runtime(BIGINT_BAML)
        huge_neg = -99999999999999999999
        result = call_function_sync(rt, "EchoBigint", {"x": huge_neg})
        assert result.result() == huge_neg

    def test_zero(self):
        rt = make_runtime(BIGINT_BAML)
        result = call_function_sync(rt, "EchoBigint", {"x": 0})
        assert result.result() == 0


class TestBigintRoundTripAsync:
    async def test_large_positive(self):
        rt = make_runtime(BIGINT_BAML)
        huge = 99999999999999999999
        result = await call_function(rt, "EchoBigint", {"x": huge})
        assert result.result() == huge

    async def test_large_negative(self):
        rt = make_runtime(BIGINT_BAML)
        huge_neg = -(2**128)
        result = await call_function(rt, "EchoBigint", {"x": huge_neg})
        assert result.result() == huge_neg
