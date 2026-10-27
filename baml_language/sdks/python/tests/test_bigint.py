"""Round-trip tests for the `bigint` primitive through the Python SDK.

The wire format is hex (base sixteen) via `bigint_value`. These calls go by
name to a runtime with no generated descriptor, so the bridge has no declared
type to encode against and a Python `int` means what the host type says: an
`int` where it fits the engine's integer, a `bigint` where it does not. The
engine converts nothing, so a small `int` is refused by a `bigint` parameter
here; the generated SDK, which knows the declared type, encodes it as the
`bigint` it is meant as.
"""

import pytest

from baml_bridge import BamlRuntime, call_function_sync, call_function


BIGINT_BAML = """\
function EchoBigint(x: bigint) -> bigint {
    x
}
"""


def make_runtime(baml_source: str) -> BamlRuntime:
    return BamlRuntime.initialize_runtime(".", {"main.baml": baml_source})


class TestBigintRoundTripSync:
    @pytest.mark.parametrize("small", [42, -42, 0])
    def test_small_int_is_no_bigint_without_a_declared_type(self, small):
        rt = make_runtime(BIGINT_BAML)
        with pytest.raises(TypeError, match="does not match the declared type `bigint`"):
            call_function_sync(rt, "EchoBigint", {"x": small})

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


# Without a declared type the bridge sends a small Python `int` as an `int`,
# and the engine does not widen it: a `bigint` parameter refuses it, as the
# type system does (`int` and `bigint` are atomic). An `int` beyond the
# engine's integer is a `bigint` as it is.
PLUS_ONE_BAML = """\
function PlusOne(x: bigint) -> bigint {
    x + 1n
}
"""


class TestBigintParameterIsExact:
    @pytest.mark.parametrize("small", [42, -5])
    def test_bigint_param_refuses_a_small_python_int(self, small):
        rt = make_runtime(PLUS_ONE_BAML)
        with pytest.raises(TypeError, match="does not match the declared type `bigint`"):
            call_function_sync(rt, "PlusOne", {"x": small})

    def test_bigint_param_takes_a_python_int_beyond_the_engine_integer(self):
        rt = make_runtime(PLUS_ONE_BAML)
        huge = 2**70
        result = call_function_sync(rt, "PlusOne", {"x": huge})
        assert result.result() == huge + 1
