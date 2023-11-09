import datetime
import typing
from tenacity import (
    retry,
    stop_after_attempt,
    wait_fixed,
    wait_exponential_jitter,
    wait_exponential,
)
from ..errors.llm_exc import LLMException, TerminalErrorCode

WrappedFn = typing.TypeVar("WrappedFn", bound=typing.Callable[..., typing.Any])


def should_retry_on_exception(exception: Exception) -> bool:
    if not isinstance(exception, LLMException):
        return False
    return exception.code not in TerminalErrorCode


def create_retry_policy_constant_delay(
    *, delay_ms: int, max_retries: int = 3
) -> typing.Callable[[WrappedFn], WrappedFn]:
    return retry(
        stop=stop_after_attempt(max_retries),
        wait=wait_fixed(datetime.timedelta(milliseconds=delay_ms)),
    )


def create_retry_policy_exponential_backoff(
    *,
    delay_ms: int,
    max_delay_ms: int,
    max_retries: int = 3,
    multiplier: float = 2,
    exp_base: float = 2,
) -> typing.Callable[[WrappedFn], WrappedFn]:
    return retry(
        stop=stop_after_attempt(max_retries),
        wait=wait_exponential(
            min=delay_ms / 1000,
            multiplier=multiplier,
            exp_base=exp_base,
            max=max_delay_ms / 1000,
        ),
    )


def create_retry_policy_exponential_backoff_jitter(
    *,
    delay_ms: int,
    max_delay_ms: int,
    max_retries: int = 3,
    exp_base: float = 2,
    jitter_ms: int,
) -> typing.Callable[[WrappedFn], WrappedFn]:
    return retry(
        stop=stop_after_attempt(max_retries),
        wait=wait_exponential_jitter(
            initial=delay_ms / 1000,
            exp_base=exp_base,
            max=max_delay_ms / 1000,
            jitter=jitter_ms / 1000,
        ),
    )
