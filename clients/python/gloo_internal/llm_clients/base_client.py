from __future__ import annotations

import abc
import re
import traceback
import typing
import aiohttp

from ..tracer import set_ctx_error, set_llm_metadata, trace, update_trace_tags, get_ctx
from .. import api_types
from ..api import API
from ..logging import logger


def hide_secret(kwargs: typing.Dict[str, typing.Any]) -> typing.Dict[str, typing.Any]:
    copied = kwargs.copy()
    for x in ["api_key", "secret_key", "token", "auth"]:
        if x in copied:
            copied[x] = copied[x][:4] + "****"
    return copied


def safe_format(s: str, kwargs: typing.Dict[str, str]) -> str:
    for key, value in kwargs.items():
        s = s.replace("{@" + key + "}", value)
    # Throw error if there are any remaining placeholders of the form {@key}
    if re.search("{@.*?}", s):
        raise ValueError(f"Invalid template: {s}")
    return s


class LLMClient:
    def __init__(
        self,
        provider: str,
        __retry: int = 0,
        __default_fallback__: typing.Union["LLMClient", None] = None,
        __fallback__: typing.Union[typing.Dict[int, "LLMClient"], None] = None,
        **kwargs: typing.Any,
    ) -> None:
        self.__provider = provider
        self.__type = str(
            kwargs.pop(
                "__type",
                "chat" if provider == "openai" or provider == "azure" else "completion",
            )
        )
        self.__retry = __retry
        self.__default_fallback = __default_fallback__
        self.__fallback = __fallback__
        self.__kwargs = kwargs

    @property
    def provider(self) -> str:
        return self.__provider

    @property
    def kwargs(self) -> typing.Dict[str, typing.Any]:
        return self.__kwargs

    @property
    def type(self) -> str:
        return self.__type

    def is_chat(self) -> bool:
        return self.type == "chat"

    @abc.abstractmethod
    def get_model_name(self) -> str:
        raise NotImplementedError

    async def run(
        self,
        name: str,
        *,
        prompt: str | typing.List[api_types.LLMChat],
    ) -> str:
        return await trace(_name=name)(self._run)(prompt_template=prompt)

    async def _run_impl(
        self,
        prompt_template: str | typing.List[api_types.LLMChat],
        vars: typing.Dict[str, str] = {},
    ) -> str:
        event = api_types.LLMEventSchema(
            provider=self.provider,
            model_name=self.get_model_name(),
            input=api_types.LLMEventInput(
                prompt=api_types.LLMEventInputPrompt(
                    template=prompt_template, template_args=vars
                ),
                invocation_params=hide_secret(self.kwargs),
            ),
            output=None,
        )

        if isinstance(prompt_template, list):
            if not self.is_chat():
                raise ValueError("Pre/post prompts are only supported for chat models")

        set_llm_metadata(event)

        cached = await API.check_cache(
            payload=api_types.CacheRequest(
                provider=self.provider,
                prompt=prompt_template,
                prompt_vars=vars,
                invocation_params=event.input.invocation_params,
            )
        )

        if cached:
            model_name = cached.mdl_name
            response = cached.llm_output
            update_trace_tags(__cached="1", __cached_latency_ms=str(cached.latency_ms))
        else:
            if self.is_chat():
                if isinstance(prompt_template, list):
                    chat_prompt: typing.List[api_types.LLMChat] = [
                        {"role": x["role"], "content": safe_format(x["content"], vars)}
                        for x in prompt_template
                    ]
                else:
                    chat_prompt = [
                        {
                            "role": "user",
                            "content": safe_format(prompt_template, vars),
                        }
                    ]
                logger.info(f"Running {self.provider} with prompt:\n{chat_prompt}")
                model_name, response = await self._run_chat(chat_prompt)
            else:
                assert isinstance(prompt_template, str)
                prompt = safe_format(prompt_template, vars)
                logger.info(f"Running {self.provider} with prompt:\n{prompt}")
                model_name, response = await self._run_completion(prompt)

        logger.info(f"RESPONSE:\n{response.raw_text}")
        # Update event with output
        event.output = response
        event.mdl_name = model_name
        return response.raw_text

    async def _run(
        self,
        prompt_template: str | typing.List[api_types.LLMChat],
        vars: typing.Dict[str, str] = {},
        *,
        __max_tries__: None | int = None,
    ) -> str:
        max_tries = (
            self.__retry + 1
            if __max_tries__ is None
            else min(self.__retry + 1, __max_tries__)
        )
        assert max_tries > 0, "max_tries must be positive"
        try:
            return await self._run_impl(prompt_template, vars)
        except Exception as e:
            formatted_traceback = "".join(
                traceback.format_exception(e.__class__, e, e.__traceback__)
            )
            set_ctx_error(
                api_types.Error(
                    # TODO: For GlooErrors, we should have a list of error codes.
                    code=1,  # Unknown error.
                    message=f"{e.__class__.__name__}: {e}",
                    traceback=formatted_traceback,
                )
            )
            maybe_handler = self._handle_exception(max_tries, e)
            if maybe_handler is not None:
                handler, name = maybe_handler
                stk, ctx = get_ctx()
                return await trace(
                    _name=f"{stk.func}[{name}]",
                    _tags=dict(**ctx.tags),
                )(handler._run)(
                    prompt_template,
                    vars,
                    __max_tries__=max_tries - 1 if handler is self else None,
                )
            raise e

    def _handle_exception(
        self, max_tries: int, e: Exception
    ) -> typing.Optional[typing.Tuple["LLMClient", str]]:
        status_code = self._exception_to_code(e)
        if self._allow_retry(status_code):
            if max_tries - 1 > 1:
                return self, f"retry[{max_tries - 1}]"
        if self.__fallback and status_code is not None:
            fallback = self.__fallback.get(status_code, None)
            if fallback is not None:
                return fallback, f"fallback[{status_code}]"
        if self.__default_fallback:
            # Certain status codes are not retriable by default.
            if status_code is None or status_code not in [400, 401, 403, 404, 422]:
                return self.__default_fallback, "fallback"
        return None

    @abc.abstractmethod
    def _allow_retry(self, code: int | None) -> bool:
        if code is None:
            return True
        return code not in [400, 401, 403, 404, 422]

    @abc.abstractmethod
    def _exception_to_code(self, e: Exception) -> typing.Optional[int]:
        if isinstance(e, aiohttp.ClientError):
            return 500
        if isinstance(e, aiohttp.ClientResponseError):
            return e.status
        return None

    @abc.abstractmethod
    async def _run_completion(
        self, prompt: str
    ) -> typing.Tuple[str, api_types.LLMOutputModel]:
        raise NotImplementedError("Client must implement _run_completion method")

    @abc.abstractmethod
    async def _run_chat(
        self, chats: typing.List[api_types.LLMChat]
    ) -> typing.Tuple[str, api_types.LLMOutputModel]:
        raise NotImplementedError("Client must implement _run_chat method")
