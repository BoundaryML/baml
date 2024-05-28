from __future__ import annotations
from .baml_py import FunctionResult, FunctionResultStream, RuntimeContextManager
from typing import Callable, Generic, Optional, TypeVar

import asyncio

PartialOutputType = TypeVar("PartialOutputType")
FinalOutputType = TypeVar("FinalOutputType")


class BamlStream(Generic[PartialOutputType, FinalOutputType]):
    __ffi_stream: FunctionResultStream
    __partial_coerce: Callable[[FunctionResult], PartialOutputType]
    __final_coerce: Callable[[FunctionResult], FinalOutputType]
    __ctx_manager: RuntimeContextManager
    __task: Optional[asyncio.Task[FunctionResult]]
    __event_queue: asyncio.Queue[Optional[FunctionResult]]

    def __init__(
        self,
        ffi_stream: FunctionResultStream,
        partial_coerce: Callable[[FunctionResult], PartialOutputType],
        final_coerce: Callable[[FunctionResult], FinalOutputType],
        ctx_manager: RuntimeContextManager,
    ):
        self.__ffi_stream = ffi_stream.on_event(self.__enqueue)
        self.__partial_coerce = partial_coerce
        self.__final_coerce = final_coerce
        self.__ctx_manager = ctx_manager
        self.__task = None
        self.__event_queue = asyncio.Queue()

    def __enqueue(self, data: FunctionResult) -> None:
        self.__event_queue.put_nowait(data)

    async def __drive_to_completion(self) -> FunctionResult:
        try:
            retval = await self.__ffi_stream.done(self.__ctx_manager)
            return retval
        finally:
            self.__event_queue.put_nowait(None)

    def __drive_to_completion_in_bg(self) -> asyncio.Task[FunctionResult]:
        # Doing this without using a compare-and-swap or lock is safe,
        # because we don't cross an await point during it
        if self.__task is None:
            self.__task = asyncio.create_task(self.__drive_to_completion())

        return self.__task

    async def __aiter__(self):
        self.__drive_to_completion_in_bg()
        while True:
            event = await self.__event_queue.get()
            if event is None:
                break
            yield self.__partial_coerce(event.parsed())

    async def get_final_response(self):
        final = await self.__drive_to_completion_in_bg()
        return self.__final_coerce(final.parsed())
