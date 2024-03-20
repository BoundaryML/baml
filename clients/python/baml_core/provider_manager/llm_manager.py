import typing

from .llm_provider_factory import llm_provider_factory
from .llm_provider_base import AbstractLLMProvider
from ..logger import logger


class _LLMManager:
    __llms: typing.Dict[str, AbstractLLMProvider]
    __validated: bool

    def __init__(self) -> None:
        self.__llms = {}
        self.__validated = False

    def add_llm(
        self, *, name: str, provider: str, **kwargs: typing.Any
    ) -> AbstractLLMProvider:
        if name in self.__llms:
            raise ValueError(f"client<llm> {repr(name)} already exists")
        self.__llms[name] = llm_provider_factory(provider=provider, **kwargs)
        return self.__llms[name]

    def get_llm(self, name: str) -> AbstractLLMProvider:
        if name not in self.__llms:
            raise ValueError(
                f"client<llm> {repr(name)} not found. You can use one of {list(self.__llms.keys())}"
            )
        return self.__llms[name]

    def validate(self) -> None:
        if self.__validated:
            assert False, "LLMManager already validated"

        errors: typing.List[typing.Tuple[str, Exception]] = []
        for name, llm in self.__llms.items():
            try:
                llm.validate()
            except Exception as e:
                errors.append((name, e))
        if len(errors) > 0:
            # Print all errors
            for name, err in errors:
                logger.error(f"Validating {name} Failed: {err}")
            raise ValueError("LLM validation failed")
        self.__validated = True


LLMManager = _LLMManager()
