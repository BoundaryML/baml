# type: ignore
import typing
from openai import error as o_error

from baml_core.errors.llm_exc import ProviderErrorCode


def to_error_code(e: Exception) -> typing.Optional[int]:
    if isinstance(e, (o_error.RateLimitError, o_error.TryAgain)):
        return ProviderErrorCode.RATE_LIMITED
    if isinstance(e, o_error.APIConnectionError):
        return (
            ProviderErrorCode.SERVICE_UNAVAILABLE
            if e.should_retry
            else ProviderErrorCode.INTERNAL_ERROR
        )
    if isinstance(e, o_error.ServiceUnavailableError):
        return ProviderErrorCode.SERVICE_UNAVAILABLE
    if isinstance(
        e,
        (
            o_error.AuthenticationError,
            o_error.SignatureVerificationError,
            o_error.PermissionError,
        ),
    ):
        return ProviderErrorCode.UNAUTHORIZED
    if isinstance(e, o_error.InvalidRequestError):
        return ProviderErrorCode.BAD_REQUEST
    if isinstance(e, o_error.OpenAIError) and isinstance(e.code, int):
        return e.code
    return None
