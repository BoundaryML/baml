# version 1.x
import typing
import openai._exceptions as o_error

from baml_core.errors.llm_exc import ProviderErrorCode


def to_error_code(e: Exception) -> typing.Optional[int]:
    if isinstance(e, (o_error.RateLimitError)):
        return ProviderErrorCode.RATE_LIMITED
    if isinstance(e, o_error.APIConnectionError):
        return ProviderErrorCode.SERVICE_UNAVAILABLE
    if isinstance(e, o_error.InternalServerError):
        return ProviderErrorCode.SERVICE_UNAVAILABLE
    if isinstance(
        e,
        (
            o_error.AuthenticationError,
            o_error.PermissionDeniedError,
        ),
    ):
        return ProviderErrorCode.UNAUTHORIZED
    if isinstance(e, o_error.BadRequestError):
        return ProviderErrorCode.BAD_REQUEST
    if isinstance(e, o_error.APIError) and isinstance(e.code, int):
        return e.code
    return ProviderErrorCode.UNKNOWN
