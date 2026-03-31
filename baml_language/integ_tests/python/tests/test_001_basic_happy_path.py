"""Integration tests that call real LLM APIs via the generated baml_client."""

import pytest
from dotenv import load_dotenv

load_dotenv()

from baml_client import b, sync_b, types


class TestExtractBasic:
    """Test ExtractTest001Basic function against a real LLM."""

    def test_sync(self):
        resume = sync_b.ExtractTest001Basic(
            text="John Doe is a software engineer skilled in Python, Rust, and TypeScript."
        )
        assert isinstance(resume, types.Test001Basic)
        assert len(resume.name) > 0
        assert len(resume.skills) > 0

    @pytest.mark.asyncio
    async def test_async(self):
        resume = await b.ExtractTest001Basic(
            text="Jane Smith is a data scientist with expertise in ML, statistics, and SQL."
        )
        assert isinstance(resume, types.Test001Basic)
        assert len(resume.name) > 0
        assert len(resume.skills) > 0


class TestTypes:
    """Verify generated types are importable and well-formed."""

    def test_resume_fields(self):
        r = types.Test001Basic(name="Test", skills=["a", "b"])
        assert r.name == "Test"
        assert r.skills == ["a", "b"]

    def test_stream_types_importable(self):
        from baml_client import stream_types

        assert hasattr(stream_types, "Test001Basic")
