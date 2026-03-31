"""Integration tests that call real LLM APIs via the generated baml_client."""

import pytest
from dotenv import load_dotenv

load_dotenv()

from baml_client import b, sync_b, types


class TestExtractResume:
    """Test ExtractResume function against a real LLM."""

    def test_sync(self):
        result = sync_b.ExtractResume(
            text="John Doe is a software engineer skilled in Python, Rust, and TypeScript."
        )
        assert isinstance(result, types.Resume)
        assert len(result.name) > 0
        assert len(result.skills) > 0

    @pytest.mark.asyncio
    async def test_async(self):
        result = await b.ExtractResume(
            text="Jane Smith is a data scientist with expertise in ML, statistics, and SQL."
        )
        assert isinstance(result, types.Resume)
        assert len(result.name) > 0
        assert len(result.skills) > 0


class TestTypes:
    """Verify generated types are importable and well-formed."""

    def test_resume_fields(self):
        r = types.Resume(name="Test", skills=["a", "b"])
        assert r.name == "Test"
        assert r.skills == ["a", "b"]

    def test_stream_types_importable(self):
        from baml_client import stream_types
        assert hasattr(stream_types, "Resume")
