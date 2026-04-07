from __future__ import annotations

import typing
import typing_extensions

import baml_py

from .globals import (
    DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME as __runtime__,
)


class BamlCallOptions(typing.TypedDict, total=False):
    collector: typing_extensions.NotRequired[
        typing.Union[baml_py.Collector, typing.List[baml_py.Collector]]
    ]
    abort_controller: typing_extensions.NotRequired[baml_py.AbortController]


class DoNotUseDirectlyCallManager:
    def __init__(self, baml_options: BamlCallOptions):
        self.__baml_options = baml_options

    def __getstate__(self):
        return {"baml_options": self.__baml_options}

    def __setstate__(self, state):
        self.__baml_options = state["baml_options"]

    def merge_options(self, options: BamlCallOptions) -> "DoNotUseDirectlyCallManager":
        return DoNotUseDirectlyCallManager({**self.__baml_options, **options})

    def _resolve_collectors(
        self,
    ) -> typing.Optional[typing.List[baml_py.Collector]]:
        c = self.__baml_options.get("collector")
        if c is None:
            return None
        return c if isinstance(c, list) else [c]

    def call_function_sync(
        self, *, function_name: str, args: typing.Dict[str, typing.Any]
    ) -> typing.Any:
        return baml_py.call_function_sync(
            __runtime__,
            function_name,
            args,
            collectors=self._resolve_collectors(),
            abort_controller=self.__baml_options.get("abort_controller"),
        )

    async def call_function_async(
        self, *, function_name: str, args: typing.Dict[str, typing.Any]
    ) -> typing.Any:
        return await baml_py.call_function(
            __runtime__,
            function_name,
            args,
            collectors=self._resolve_collectors(),
            abort_controller=self.__baml_options.get("abort_controller"),
        )


__all__ = ["BamlCallOptions", "DoNotUseDirectlyCallManager"]
