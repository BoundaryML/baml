"""Tests for functions returning non-class types (list, int, string)."""

import pytest
from dotenv import load_dotenv

load_dotenv()

from baml_client import b, sync_b, types


class TestExtractKeywords:
    """Test ExtractKeywords — returns string[]."""

    def test_sync(self):
        result = sync_b.ExtractKeywords(
            text="Rust is a systems programming language focused on safety, speed, and concurrency."
        )
        assert isinstance(result, list)
        assert len(result) > 0
        assert all(isinstance(k, str) for k in result)

    @pytest.mark.asyncio
    async def test_async(self):
        result = await b.ExtractKeywords(
            text="Python is great for machine learning, data science, and web development."
        )
        assert isinstance(result, list)
        assert len(result) > 0


class TestExtractMultipleResumes:
    """Test ExtractMultipleResumes — returns Test001Basic[]."""

    def test_sync(self):
        result = sync_b.ExtractMultipleResumes(
            text="Alice is a backend engineer skilled in Go and Postgres. "
                 "Bob is a designer who knows Figma and CSS."
        )
        assert isinstance(result, list)
        assert len(result) >= 2
        assert all(isinstance(r, types.Test001Basic) for r in result)
        assert all(len(r.name) > 0 for r in result)
        assert all(len(r.skills) > 0 for r in result)

    @pytest.mark.asyncio
    async def test_async(self):
        result = await b.ExtractMultipleResumes(
            text="Carol is a data scientist with Python and R. "
                 "Dave is a DevOps engineer skilled in Kubernetes and Terraform."
        )
        assert isinstance(result, list)
        assert len(result) >= 2
        assert all(isinstance(r, types.Test001Basic) for r in result)


class TestCountWords:
    """Test CountWords — returns int."""

    def test_sync(self):
        result = sync_b.CountWords(text="one two three four five")
        assert isinstance(result, int)
        assert result == 5

    @pytest.mark.asyncio
    async def test_async(self):
        result = await b.CountWords(text="hello world")
        assert isinstance(result, int)
        assert result == 2


class TestSummarize:
    """Test Summarize — returns string."""

    def test_sync(self):
        result = sync_b.Summarize(
            text="The quick brown fox jumps over the lazy dog. This sentence contains every letter of the alphabet."
        )
        assert isinstance(result, str)
        assert len(result) > 0

    @pytest.mark.asyncio
    async def test_async(self):
        result = await b.Summarize(
            text="BAML is a domain-specific language for defining LLM functions with structured inputs and outputs."
        )
        assert isinstance(result, str)
        assert len(result) > 0
