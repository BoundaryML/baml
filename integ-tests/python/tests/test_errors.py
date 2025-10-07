import pytest

from ..baml_client import b
from baml_py.errors import BamlClientHttpError, BamlError
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

@pytest.mark.asyncio
async def test_error_on_missing_url_env_var():
    with pytest.raises(BamlError) as err:
        await b.OpenAIGPT4oMissingBaseUrlEnvVar("computers")

    assert_that(str(err.value), equal_to("LLM client 'GPT4oBaseUrlNotSet' requires environment variable 'OPEN_API_BASE_DO_NOT_SET_THIS' to be set but it is not"))