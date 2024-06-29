from __future__ import annotations
from .baml_py import (
    FunctionResult,
    FunctionResultStream,
    RuntimeContextManager,
    TypeBuilder,
)
from typing import Callable, Generic, Optional, TypeVar
import threading
import asyncio
import concurrent.futures

import queue

PartialOutputType = TypeVar("PartialOutputType")
FinalOutputType = TypeVar("FinalOutputType")


class BamlStream(Generic[PartialOutputType, FinalOutputType]):
    __ffi_stream: FunctionResultStream
    __partial_coerce: Callable[[FunctionResult], PartialOutputType]
    __final_coerce: Callable[[FunctionResult], FinalOutputType]
    __ctx_manager: RuntimeContextManager
    __task: Optional[threading.Thread]
    __event_queue: queue.Queue[Optional[FunctionResult]]
    __tb: Optional[TypeBuilder]
    __future: concurrent.futures.Future[FunctionResult]

    def __init__(
        self,
        ffi_stream: FunctionResultStream,
        partial_coerce: Callable[[FunctionResult], PartialOutputType],
        final_coerce: Callable[[FunctionResult], FinalOutputType],
        ctx_manager: RuntimeContextManager,
        tb: Optional[TypeBuilder],
    ):
        self.__ffi_stream = ffi_stream.on_event(self.__enqueue)
        self.__partial_coerce = partial_coerce
        self.__final_coerce = final_coerce
        self.__ctx_manager = ctx_manager
        self.__task = None
        self.__event_queue = queue.Queue()
        self.__tb = tb
        self.__future = concurrent.futures.Future()  # Initialize the future here

    def __enqueue(self, data: FunctionResult) -> None:
        self.__event_queue.put_nowait(data)

    async def __drive_to_completion(self) -> FunctionResult:

        try:
            retval = await self.__ffi_stream.done(self.__ctx_manager)

            self.__future.set_result(retval)
            return retval
        except Exception as e:
            self.__future.set_exception(e)
            raise
        finally:
            self.__event_queue.put_nowait(None)

    def __drive_to_completion_in_bg(self) -> concurrent.futures.Future[FunctionResult]:
        if self.__task is None:
            self.__task = threading.Thread(target=self.threading_target, daemon=True)
            self.__task.start()
        return self.__future

    def threading_target(self):
        asyncio.run(self.__drive_to_completion(), debug=True)

    async def __aiter__(self):
        # TODO: This is deliberately __aiter__ and not __iter__ because we want to
        # ensure that the caller is using an async for loop.
        # Eventually we do not want to create a new thread for each stream.
        self.__drive_to_completion_in_bg()
        while True:
            event = self.__event_queue.get()
            if event is None:
                break
            yield self.__partial_coerce(event.parsed())

    async def get_final_response(self):
        final = self.__drive_to_completion_in_bg()
        return self.__final_coerce((await asyncio.wrap_future(final)).parsed())
