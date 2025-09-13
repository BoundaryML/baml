import pytest

from ..baml_client import b


@pytest.mark.asyncio
async def test_fallback_errors():
    try:
        await b.TestFallbackAlwaysFails("lorem ipsum")
    except Exception as e:
        assert "openai/gpt-0-noexist" in str(e)
        assert "openai/gpt-1-noexist" in str(e)
        assert "openai/gpt-2-noexist" in str(e)
        assert "openai/gpt-3-noexist" in str(e)
    assert False
