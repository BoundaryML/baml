import pytest

from ..baml_client import b
from baml_py.errors import BamlClientHttpError
from hamcrest import assert_that, equal_to


@pytest.mark.asyncio
async def test_always_fails_errors():
    with pytest.raises(BamlClientHttpError) as exc_info:
        await b.FnAlwaysFails("lorem ipsum")
    e = exc_info.value
    assert_that(
        e.detailed_message,
        equal_to("""LLM client "openai/gpt-0-noexist" failed with status code: Unspecified error code: 404
Message: Request failed with status code: 404 Not Found. {"error":{"message":"The model `gpt-0-noexist` does not exist or you do not have access to it.","type":"invalid_request_error","param":null,"code":"model_not_found"}}"""),
    )


@pytest.mark.asyncio
async def test_fallback_errors():
    with pytest.raises(Exception) as exc_info:
        await b.FnFallbackAlwaysFails("lorem ipsum")
    e = exc_info.value
    assert_that(
        e.detailed_message,
        equal_to("""3 failed attempts:

Attempt 0: LLM client "openai/gpt-0-noexist" failed with status code: Unspecified error code: 404
    Message: Request failed with status code: 404 Not Found. {"error":{"message":"The model `gpt-0-noexist` does not exist or you do not have access to it.","type":"invalid_request_error","param":null,"code":"model_not_found"}}
Attempt 1: LLM client "openai/gpt-1-noexist" failed with status code: Unspecified error code: 404
    Message: Request failed with status code: 404 Not Found. {"error":{"message":"The model `gpt-1-noexist` does not exist or you do not have access to it.","type":"invalid_request_error","param":null,"code":"model_not_found"}}
Attempt 2: LLM client "openai/gpt-2-noexist" failed with status code: Unspecified error code: 404
    Message: Request failed with status code: 404 Not Found. {"error":{"message":"The model `gpt-2-noexist` does not exist or you do not have access to it.","type":"invalid_request_error","param":null,"code":"model_not_found"}}"""),
    )
