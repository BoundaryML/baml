import abc
import json
import traceback
import typing
import aiohttp
from baml_core_ffi import RenderData_Client, TemplateStringMacro
from pydantic import BaseModel
from typeguard import typechecked

from ..configs.retry_policy import WrappedFn

from ..errors.llm_exc import LLMException, ProviderErrorCode

from ..cache_manager import CacheManager
from ..services.api_types import CacheRequest, LLMChat
from ..otel.helper import try_serialize
from ..otel.provider import create_event
from .llm_response import LLMResponse
from ..jinja.render_prompt import RenderData


class BaseProvider(abc.ABC):
    def __to_error_code(
        self, e: Exception
    ) -> typing.Optional[typing.Union[ProviderErrorCode, int]]:
        if isinstance(e, LLMException):
            return e.code
        code = self._to_error_code(e)
        if code is not None:
            return code

        if isinstance(e, aiohttp.ClientError):
            return ProviderErrorCode.INTERNAL_ERROR
        if isinstance(e, aiohttp.ClientResponseError):
            return e.status
        return None

    @abc.abstractmethod
    def _to_error_code(
        self, e: Exception
    ) -> typing.Optional[typing.Union[ProviderErrorCode, int]]:
        raise NotImplementedError()

    def _raise_error(self, e: Exception) -> typing.NoReturn:
        formatted_traceback = "".join(
            traceback.format_exception(type(e), e, e.__traceback__)
        )
        create_event(
            "llm_request_error",
            {
                "traceback": formatted_traceback,
                "message": f"{type(e).__name__}: {e}",
                "code": self.__to_error_code(e) or 2,
            },
        )
        if isinstance(e, LLMException):
            raise e
        code = self.__to_error_code(e)
        if code is not None:
            raise LLMException(code=code, message=str(e))
        raise e


class LLMChatMessage(typing.TypedDict):
    role: str
    content: str


def update_template_with_vars(
    *, template: str, updates: typing.Mapping[str, str]
) -> str:
    prompt = str(template)
    for k, v in updates.items():
        prompt = prompt.replace(k, v)
    return prompt


def _redact(value: typing.Any) -> typing.Any:
    if isinstance(value, str):
        if len(value) > 4:
            return value[:2] + ("*" * 4)
        return "****"
    if isinstance(value, dict):
        return {k: _redact(v) for k, v in value.items()}
    if isinstance(value, list):
        return [_redact(v) for v in value]
    return _redact(str(value))


class AbstractLLMProvider(BaseProvider, abc.ABC):
    """
    Abstract base class to ensure both LLMProvider and LLMChatProvider
    have run_prompt and run_chat methods.
    """

    __client_args: typing.Dict[str, typing.Any]

    @typechecked
    def __init__(
        self,
        provider: str,
        retry_policy: typing.Optional[typing.Callable[[WrappedFn], WrappedFn]],
        redactions: typing.Optional[typing.List[str]] = None,
        **kwargs: typing.Any,
    ) -> None:
        self.__provider = provider
        self.__client_args = {}
        self.__redactions = redactions or []
        self.__retry_policy = retry_policy
        # This is optional due to backwards compatibility
        self.__render_client = RenderData.client(
            name=kwargs.pop("client_name", "<unknown>"), provider=provider
        )
        assert not kwargs, f"Unhandled provider settings: {', '.join(kwargs.keys())}"

    @property
    def client(self) -> RenderData_Client:
        return self.__render_client

    @property
    def provider(self) -> str:
        return self.__provider

    #
    # Public API
    #
    @typing.final
    @typechecked
    async def run_jinja_template(
        self,
        *,
        jinja_template: str,
        # Params for the jinja template
        args: typing.Dict[str, typing.Any],
        output_format: str,
        # Other template macros
        template_macros: typing.List[TemplateStringMacro],
    ) -> LLMResponse:
        return await self._run_jinja_template_internal(
            jinja_template=jinja_template,
            args=args,
            template_macros=template_macros,
            output_format=output_format,
        )

    @typing.final
    @typechecked
    async def run_jinja_template_stream(
        self,
        *,
        jinja_template: str,
        # Params for the jinja template
        args: typing.Dict[str, typing.Any],
        output_format: str,
        # Other template macros
        template_macros: typing.List[TemplateStringMacro],
    ) -> typing.AsyncIterator[LLMResponse]:
        async for r in self._run_jinja_template_internal_stream(
            jinja_template=jinja_template,
            args=args,
            template_macros=template_macros,
            output_format=output_format,
        ):
            yield r

    @typing.final
    @typechecked
    async def run_prompt_template(
        self,
        *,
        template: str,
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        return await self._run_prompt_template_internal(
            template=template,
            replacers=replacers,
            params=params,
        )

    @typing.final
    @typechecked
    async def run_prompt_template_stream(
        self,
        *,
        template: str,
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> typing.AsyncIterator[LLMResponse]:
        async for r in self._run_prompt_template_internal_stream(
            template=template,
            replacers=replacers,
            params=params,
        ):
            yield r

    @typing.final
    @typechecked
    async def run_chat_template(
        self,
        *message_templates: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]],
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        return await self._run_chat_template_internal(
            *message_templates,
            replacers=replacers,
            params=params,
        )

    @typing.final
    @typechecked
    async def run_chat_template_stream(
        self,
        *message_templates: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]],
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> typing.AsyncIterator[LLMResponse]:
        async for r in self._run_chat_template_internal_stream(
            *message_templates,
            replacers=replacers,
            params=params,
        ):
            yield r

    @typing.final
    @typechecked
    async def run_prompt(self, prompt: str) -> LLMResponse:
        return await self._run_prompt_internal(prompt=prompt)

    @typing.final
    @typechecked
    async def run_prompt_stream(self, prompt: str) -> typing.AsyncIterator[LLMResponse]:
        async for r in self._run_prompt_internal_stream(prompt=prompt):
            yield r

    @typing.final
    @typechecked
    async def run_chat(
        self, *messages: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]]
    ) -> LLMResponse:
        return await self._run_chat_internal(*messages)

    @typing.final
    @typechecked
    async def run_chat_stream(
        self, *messages: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]]
    ) -> typing.AsyncIterator[LLMResponse]:
        async for r in self._run_chat_internal_stream(*messages):
            yield r

    #
    # Internal API
    #
    @abc.abstractmethod
    async def _run_jinja_template_internal(
        self,
        *,
        jinja_template: str,
        args: typing.Dict[str, typing.Any],
        output_format: str,
        template_macros: typing.List[TemplateStringMacro],
    ) -> LLMResponse:
        pass

    @abc.abstractmethod
    def _run_jinja_template_internal_stream(
        self,
        *,
        jinja_template: str,
        args: typing.Dict[str, typing.Any],
        output_format: str,
        template_macros: typing.List[TemplateStringMacro],
    ) -> typing.AsyncIterator[LLMResponse]:
        pass

    @abc.abstractmethod
    async def _run_prompt_internal(self, *, prompt: str) -> LLMResponse:
        pass

    @abc.abstractmethod
    def _run_prompt_internal_stream(
        self,
        *,
        prompt: str,
    ) -> typing.AsyncIterator[LLMResponse]:
        pass

    @abc.abstractmethod
    def _run_prompt_template_internal_stream(
        self,
        *,
        template: str,
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> typing.AsyncIterator[LLMResponse]:
        pass

    @abc.abstractmethod
    async def _run_prompt_template_internal(
        self,
        *,
        template: str,
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        pass

    @abc.abstractmethod
    async def _run_chat_internal(
        self, *messages: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]]
    ) -> LLMResponse:
        pass

    @abc.abstractmethod
    def _run_chat_internal_stream(
        self, *messages: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]]
    ) -> typing.AsyncIterator[LLMResponse]:
        pass

    @abc.abstractmethod
    async def _run_chat_template_internal(
        self,
        *message_templates: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]],
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        pass

    @abc.abstractmethod
    async def _run_chat_template_internal_stream(
        self,
        *message_templates: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]],
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> typing.AsyncIterator[LLMResponse]:
        raise NotImplementedError()
        yield

    @typing.final
    def validate(self) -> None:
        if self.__retry_policy:
            # Decorate the run_prompt and run_chat methods with the retry policy
            self.__dict__["run_prompt"] = self.__retry_policy(self.run_prompt)  # type: ignore
            self.__dict__["run_prompt_template"] = self.__retry_policy(
                self.run_prompt_template  # type: ignore
            )
            self.__dict__["run_chat"] = self.__retry_policy(self.run_chat)  # type: ignore
            self.__dict__["run_chat_template"] = self.__retry_policy(
                self.run_chat_template  # type: ignore
            )
        self._validate()

    @abc.abstractmethod
    def _validate(self) -> None:
        """
        Run any validation checks on the provider. This is called via
        baml_init() and should raise an exception if the provider is
        not configured correctly.
        """
        pass

    @typing.final
    def _start_run(
        self, prompt: typing.Union[str, typing.List[LLMChatMessage]]
    ) -> None:
        if isinstance(prompt, str):
            create_event(
                "llm_request_start",
                {"prompt": prompt, "provider": self.provider},
            )
        else:
            create_event(
                "llm_request_start",
                {
                    "chat_prompt": list(map(json.dumps, prompt)),
                    "provider": self.provider,
                },
            )
        create_event(
            "llm_request_args",
            {k: try_serialize(v)[0] for k, v in self.__client_args.items()},
        )

    @typing.final
    def _end_run(self, response: LLMResponse) -> None:
        create_event(
            "llm_request_end",
            {
                "generated": response.generated,
                "model_name": response.mdl_name,
                "meta": json.dumps(
                    response.meta,
                    default=lambda x: (
                        x.model_dump() if isinstance(x, BaseModel) else str(x)
                    ),
                ),
            },
        )

    @typing.final
    def _set_args(self, **kwargs: typing.Any) -> None:
        # Ensure all redactions should be hidden
        self.__client_args = {
            k: v if k not in self.__redactions else _redact(v)
            for k, v in kwargs.items()
        }

    def _check_cache(
        self,
        *,
        prompt: typing.Union[str, typing.List[LLMChat]],
        prompt_vars: typing.Dict[str, typing.Any],
    ) -> typing.Optional[LLMResponse]:
        if cached := CacheManager.get_llm_request(
            CacheRequest(
                provider=self.provider,
                prompt=prompt,
                prompt_vars=prompt_vars,
                invocation_params=self.__client_args,
            )
        ):
            self._start_run(prompt)
            create_event("llm_request_cache_hit", {"latency_ms": cached.latency_ms})
            reply = LLMResponse(
                generated=cached.llm_output.raw_text,
                model_name=cached.mdl_name,
                meta=cached.llm_output.metadata,
            )
            self._end_run(reply)
            return reply
        return None
