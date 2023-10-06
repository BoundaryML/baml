import abc
from textwrap import dedent
import typing
from .llm_client import LLMClient
from .tracer import trace

InputType = typing.TypeVar("InputType")
OutputType = typing.TypeVar("OutputType")

T = typing.TypeVar("T")


class GlooVariant(typing.Generic[InputType, OutputType]):
    __func_name: str
    __name: str

    def __init__(self, *, func_name: str, name: str):
        self.__func_name = func_name
        self.__name = name

    @property
    def name(self) -> str:
        return self.__name

    @property
    def func_name(self) -> str:
        return self.__func_name

    @abc.abstractmethod
    async def _run(self, arg: InputType) -> OutputType:
        raise NotImplementedError

    async def run(self, arg: InputType) -> OutputType:
        response = await trace(_name=self.func_name, _tags={"__variant": self.name})(
            self._run
        )(arg)
        return response


class CodeVariant(GlooVariant[InputType, OutputType]):
    __func: typing.Callable[[InputType], typing.Awaitable[OutputType]]

    def __init__(
        self,
        func_name: str,
        name: str,
        *,
        func: typing.Callable[[InputType], typing.Awaitable[OutputType]],
    ):
        super().__init__(func_name=func_name, name=name)
        self.__func = func

    async def _run(self, arg: InputType) -> OutputType:
        return await self.__func(arg)


class LLMVariant(GlooVariant[InputType, OutputType]):
    __prompt: str
    __client: LLMClient

    def __init__(
        self,
        func_name: str,
        name: str,
        *,
        prompt: str,
        client: LLMClient,
        prompt_vars: typing.Callable[
            [InputType], typing.Awaitable[typing.Dict[str, str]]
        ],
        parser: typing.Callable[[str], typing.Awaitable[OutputType]],
    ):
        super().__init__(func_name=func_name, name=name)
        self.__prompt = prompt
        self.__client = client
        self.__prompt_vars = prompt_vars
        self.__parser = parser

    async def _run(self, arg: InputType) -> OutputType:
        prompt_vars = await self.__prompt_vars(arg)

        # Determine which prompt vars are used in the prompt string.
        # format is {@var_name}
        used_vars = set()
        for var_name in prompt_vars:
            if f"{{@{var_name}}}" in self.__prompt:
                used_vars.add(var_name)

        # If there are unused vars, log a warning
        prompt_vars_copy = {
            var_name: dedent(prompt_vars[var_name].lstrip("\n").rstrip())
            for var_name in used_vars
        }

        response = await self.__client._run(self.__prompt, vars=prompt_vars_copy)
        return await self.__parser(response)
