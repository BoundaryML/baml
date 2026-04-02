"""Tests for BAML types defined in a namespace directory (ns_test003/)."""

import pytest
from dotenv import load_dotenv

load_dotenv()

from baml_client import async_b, sync_b, types


class TestAnalyzeSentiment:
    """Test AnalyzeSentiment — defined in ns_test003/ namespace."""

    @pytest.mark.xfail(reason="Namespaced function lookup not yet wired: bex_engine registers qualified names but codegen emits bare names")
    def test_sync_positive(self):
        result = sync_b.AnalyzeSentiment(
            text="I absolutely love this product, it's amazing!"
        )
        assert isinstance(result, types.Sentiment)
        assert result.label in ("positive", "negative", "neutral")
        assert 0.0 <= result.confidence <= 1.0

    @pytest.mark.xfail(reason="Namespaced function lookup not yet wired")
    def test_sync_negative(self):
        result = sync_b.AnalyzeSentiment(
            text="This is terrible, worst experience ever."
        )
        assert isinstance(result, types.Sentiment)
        assert result.label in ("positive", "negative", "neutral")

    @pytest.mark.xfail(reason="Namespaced function lookup not yet wired")
    @pytest.mark.asyncio
    async def test_async(self):
        result = await async_b.AnalyzeSentiment(
            text="The weather is okay today, nothing special."
        )
        assert isinstance(result, types.Sentiment)
        assert result.label in ("positive", "negative", "neutral")
        assert 0.0 <= result.confidence <= 1.0


class TestSentimentTypes:
    """Verify Sentiment type shape."""

    def test_literal_label(self):
        s = types.Sentiment(label="positive", confidence=0.95)
        assert s.label == "positive"

    def test_confidence_range(self):
        s = types.Sentiment(label="neutral", confidence=0.5)
        assert s.confidence == 0.5
