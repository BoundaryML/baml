import typing

from .llm_provider_factory import llm_provider_factory
from .llm_provider import AbstractLLMProvider
from ..logger import logger


class _LLMManager:
    __llms: typing.Dict[str, AbstractLLMProvider]

    def __init__(self) -> None:
        self.__llms = {}

    def add_llm(
        self, *, name: str, provider: str, **kwargs: typing.Any
    ) -> AbstractLLMProvider:
        if name in self.__llms:
            raise ValueError(f"LLM with {name} already exists")
        self.__llms[name] = llm_provider_factory(provider=provider, **kwargs)
        return self.__llms[name]

    def get_llm(self, name: str) -> AbstractLLMProvider:
        if name not in self.__llms:
            raise ValueError(
                f"LLM {name} not found. You can use one of {self.__llms.keys()}"
            )
        return self.__llms[name]

    def validate(self) -> None:
        errors = []
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


LLMManager = _LLMManager()
