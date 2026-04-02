"""Integration tests that call real LLM APIs via the generated baml_client."""

import pytest
from dotenv import load_dotenv

load_dotenv()

from baml_client import async_b, sync_b, types


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
        resume = await async_b.ExtractTest001Basic(
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

    def test_stream_done_nested_uses_non_stream_types(self):
        """@stream.done fields should reference types.X (non-stream) at all nesting depths."""
        from baml_client import stream_types
        import typing

        hints = typing.get_type_hints(stream_types.StreamDoneNested)

        # done_class: should be types.Inner001 (non-stream)
        assert hints["done_class"] is types.Inner001

        # done_list: should be list[types.Inner001]
        origin = typing.get_origin(hints["done_list"])
        assert origin is list
        (arg,) = typing.get_args(hints["done_list"])
        assert arg is types.Inner001

        # done_map: should be dict[str, types.Inner001]
        origin = typing.get_origin(hints["done_map"])
        assert origin is dict
        _, val_arg = typing.get_args(hints["done_map"])
        assert val_arg is types.Inner001

        # regular: should be Union[stream_types.Inner001, None] (stream variant)
        args = typing.get_args(hints["regular"])
        non_none_args = [a for a in args if a is not type(None)]
        assert len(non_none_args) == 1
        assert non_none_args[0] is stream_types.Inner001
