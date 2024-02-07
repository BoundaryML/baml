import pytest
from baml_core.stream.baml_stream import AsyncStream, PartialValueWrapper
from baml_core.provider_manager import LLMResponse
from baml_lib._impl.deserializer import Deserializer, register_deserializer
from baml_core.stream.partialjson import JSONParser
from typing import List, Dict, Optional, Union


async def async_generator():
    yield None


async_stream = AsyncStream(
    stream=async_generator(),  # type: ignore
    partial_deserializer=Deserializer[str](str),
    final_deserializer=Deserializer[str](str),
)


def create_async_stream(partial_deserializer, final_deserializer):
    return AsyncStream(
        stream=async_generator(),  # type: ignore
        partial_deserializer=partial_deserializer,
        final_deserializer=final_deserializer,
    )


@pytest.mark.asyncio
async def test_input_str_output_str():
    text = "The answer is: hello"
    result = await async_stream.parse_stream_chunk(text, text[-3:])

    # Check the result
    assert isinstance(result, PartialValueWrapper)
    assert result.parsed == text


@pytest.mark.asyncio
async def test_input_str_with_quotes_output_str():
    text = '"The answer is: hello"'
    result = await async_stream.parse_stream_chunk(text, text[-3:])

    # Check the result
    assert isinstance(result, PartialValueWrapper)
    assert result.parsed == '"The answer is: hello"'


from pydantic import BaseModel


@register_deserializer({})
class User(BaseModel):
    name: Optional[str] = None
    age: Optional[int] = None


# Create deserializers for the User model
user_partial_deserializer = Deserializer[User](User)
user_final_deserializer = Deserializer[User](User)

# Create an async stream for the User model
user_async_stream = create_async_stream(
    user_partial_deserializer, user_final_deserializer
)


@pytest.mark.asyncio
async def test_input_dict_output_user():
    user_dict = '{"name": "John", "age": 30}'
    result = await user_async_stream.parse_stream_chunk(user_dict, user_dict[-3:])

    # Check the result
    assert isinstance(result, PartialValueWrapper)
    assert isinstance(result.parsed, User)
    assert result.parsed.name == "John"
    assert result.parsed.age == 30


@pytest.mark.asyncio
async def test_input_dict_with_text_prefix_output_user():
    user_dict = 'The output is: {"name": "John", "age": 30}'
    result = await user_async_stream.parse_stream_chunk(user_dict, user_dict[-3:])

    # Check the result
    assert isinstance(result, PartialValueWrapper)
    assert isinstance(result.parsed, User)
    assert result.parsed.name == "John"
    assert result.parsed.age == 30


@pytest.mark.asyncio
async def test_input_dict_with_text_prefix_output_user_pretty_json():
    user_dict = """
    The output is: 
    {
        "name": "John", 
        "age": 30
    }
    """
    result = await user_async_stream.parse_stream_chunk(user_dict, user_dict[-3:])

    # Check the result
    assert isinstance(result, PartialValueWrapper)
    assert isinstance(result.parsed, User)
    assert result.parsed.name == "John"
    assert result.parsed.age == 30


@pytest.mark.asyncio
async def test_input_dict_with_text_prefix_output_user_truncated():
    user_dict = 'The output is: {"name": "John", '
    result = await user_async_stream.parse_stream_chunk(user_dict, user_dict[-3:])

    # Check the result
    assert isinstance(result, PartialValueWrapper)
    assert isinstance(result.parsed, User)
    assert result.parsed.name == "John"
    assert result.parsed.age is None


@pytest.mark.asyncio
async def test_input_dict_with_text_prefix_output_user_truncated2():
    user_dict = 'The output is: \n{"name": "John", "'
    result = await user_async_stream.parse_stream_chunk(user_dict, user_dict[-3:])

    # Check the result
    assert isinstance(result, PartialValueWrapper)
    assert isinstance(result.parsed, User)
    assert result.parsed.name == "John"
    assert result.parsed.age is None


@pytest.mark.asyncio
async def test_input_two_dict_with_text_prefix_output_user():
    user_dict = 'The output is: {"name": "John", "age": 30}, but it could also be {"name": "John", age: 25}\nYeah.'
    result = await user_async_stream.parse_stream_chunk(user_dict, user_dict[-3:])

    # Check the result
    assert isinstance(result, PartialValueWrapper)
    assert isinstance(result.parsed, User)
    assert result.parsed.name == "John"
    assert result.parsed.age == 30


@pytest.mark.asyncio
async def test_input_two_dict_with_text_prefix_output_user_truncated():
    user_dict = 'The output is: {"name": "John", "age": 30}, but it could also be {"name": "John"'
    result = await user_async_stream.parse_stream_chunk(user_dict, user_dict[-3:])

    # Check the result
    assert isinstance(result, PartialValueWrapper)
    assert isinstance(result.parsed, User)
    assert result.parsed.name == "John"
    assert result.parsed.age == 30


@pytest.mark.asyncio
async def test_input_dict_with_text_before_and_after_output_user():
    user_dict = 'The output is: {"name": "John", "age": 30}, and that is the end'
    result = await user_async_stream.parse_stream_chunk(user_dict, user_dict[-3:])

    # Check the result
    assert isinstance(result, PartialValueWrapper)
    assert isinstance(result.parsed, User)
    assert result.parsed.name == "John"
    assert result.parsed.age == 30


@register_deserializer({})
class AllRequiredFields(BaseModel):
    name: str
    age: int


@register_deserializer({})
class NestedUser(BaseModel):
    user: Optional[AllRequiredFields] = None
    status: Optional[str] = None


# Create deserializers for the NestedUser model
nested_user_partial_deserializer = Deserializer[NestedUser](NestedUser)
nested_user_final_deserializer = Deserializer[NestedUser](NestedUser)

# Create an async stream for the NestedUser model
nested_user_async_stream = create_async_stream(
    nested_user_partial_deserializer, nested_user_final_deserializer
)


@pytest.mark.asyncio
async def test_input_dict_output_nested_user():
    nested_user_dict = '{"user": {"name": "John", "age": 30}, "status": "active"}'
    result = await nested_user_async_stream.parse_stream_chunk(
        nested_user_dict, nested_user_dict[-3:]
    )

    # Check the result
    assert isinstance(result, PartialValueWrapper)
    assert isinstance(result.parsed, NestedUser)
    assert isinstance(result.parsed.user, AllRequiredFields)
    assert result.parsed.user.name == "John"
    assert result.parsed.user.age == 30
    assert result.parsed.status == "active"


@pytest.mark.asyncio
async def test_input_dict_output_nested_user_truncated():
    nested_user_dict = '{"user": {"name": "John", '
    result = await nested_user_async_stream.parse_stream_chunk(
        nested_user_dict, nested_user_dict[-3:]
    )

    # Check the result
    assert isinstance(result, PartialValueWrapper)
    assert isinstance(result.parsed, NestedUser)

    assert result.parsed.user is None
    assert result.parsed.status is None


# Create deserializers for different types
int_deserializer = Deserializer[int](int)
float_deserializer = Deserializer[float](float)
list_deserializer = Deserializer[List[str]](List[str])

# Create async streams for different types
int_async_stream = create_async_stream(int_deserializer, int_deserializer)
float_async_stream = create_async_stream(float_deserializer, float_deserializer)
list_async_stream = create_async_stream(list_deserializer, list_deserializer)


@pytest.mark.asyncio
async def test_input_str_output_int():
    text = "123"
    result = await int_async_stream.parse_stream_chunk(text, text[-3:])

    assert result.parsed == 123


@pytest.mark.asyncio
async def test_input_str_output_float():
    text = "123.45"
    result = await float_async_stream.parse_stream_chunk(text, text[-3:])

    assert result.parsed == 123.45


@pytest.mark.asyncio
async def test_input_str_output_list():
    text = '["1", "2", "3"]'
    result = await list_async_stream.parse_stream_chunk(text, text[-3:])

    assert result.parsed == ["1", "2", "3"]


@pytest.mark.asyncio
async def test_input_str_output_list2():
    text = ' ["1", "2", "3"] '
    result = await list_async_stream.parse_stream_chunk(text, text[-3:])

    assert result.parsed == ["1", "2", "3"]


@pytest.mark.asyncio
async def test_input_str_with_prefix_output_list():
    text = 'The output is: ["1", "2", "3"]\n. Let me know if you need anything else.'
    result = await list_async_stream.parse_stream_chunk(text, text[-3:])

    assert result.parsed == ["1", "2", "3"]


@pytest.mark.asyncio
async def test_input_dict_output_user_truncated():
    user_dict = '{"name": "John"'
    result = await user_async_stream.parse_stream_chunk(user_dict, user_dict[-3:])

    # Check the result
    assert isinstance(result, PartialValueWrapper)
    assert isinstance(result.parsed, User)
    assert result.parsed.name == "John"
    assert result.parsed.age is None


@pytest.mark.asyncio
async def test_input_str_output_int_truncated():
    text = "12"
    result = await int_async_stream.parse_stream_chunk(text, text[-3:])

    assert result.parsed == 12


# TODO
# @pytest.mark.asyncio
# async def test_input_str_output_float_truncated():
#     text = "123."
#     result = await float_async_stream.parse_stream_chunk(text, text[-3:])

#     assert result.parsed == 123.0


@pytest.mark.asyncio
async def test_input_str_output_list_truncated():
    text = '["1", "2"'
    result = await list_async_stream.parse_stream_chunk(text, text[-3:])

    assert result.parsed == ["1", "2"]


# Test empty whitespace at the end
